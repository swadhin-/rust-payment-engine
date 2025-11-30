use crate::errors::ProcessingError;
use crate::models::{Account, TransactionRow, TransactionType};
use crate::storage::{StoredTransaction, TransactionStore};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{mpsc, oneshot};
use tracing::error;

pub enum AccountMessage {
    Process {
        tx: TransactionRow,
        reply: oneshot::Sender<Result<(), ProcessingError>>,
    },
    GetState {
        reply: oneshot::Sender<Account>,
    },
    MigrateCold,
    Shutdown,
}

pub struct AccountActor {
    client_id: u16,
    account: Account,
    hot_transactions: HashMap<u32, StoredTransaction>,
    cold_storage: Arc<dyn TransactionStore>,
    hot_cutoff_days: u64,
    idle_timeout: Duration,
    last_activity: SystemTime,
    receiver: mpsc::Receiver<AccountMessage>,
}

//TODO: Move to cuutoff and idle timeout to config
impl AccountActor {
    pub fn new(
        client_id: u16,
        receiver: mpsc::Receiver<AccountMessage>,
        cold_storage: Arc<dyn TransactionStore>,
    ) -> Self {
        Self {
            client_id,
            account: Account::new(client_id),
            hot_transactions: HashMap::new(),
            cold_storage,
            hot_cutoff_days: 90, // 90-day hot storage window
            idle_timeout: Duration::from_secs(3600), // 1 hour idle timeout
            last_activity: SystemTime::now(),
            receiver,
        }
    }
    
    /// Run the actor event loop with automatic background migration and idle timeout
    pub async fn run(mut self) {
        use tokio::time::{interval, Duration};
        
        // Trigger migration every hour to keep hot storage bounded
        let mut migration_timer = interval(Duration::from_secs(3600));
        migration_timer.tick().await; // Skip first immediate tick
        
        // Check for idle timeout every 5 minutes
        let mut idle_check_timer = interval(Duration::from_secs(300));
        
        // Skip first immediate tick
        idle_check_timer.tick().await;
        
        loop {
            tokio::select! {
                Some(msg) = self.receiver.recv() => {
                    
                    self.last_activity = SystemTime::now();
                    
                    match msg {
                        AccountMessage::Process { tx, reply } => {
                            let result = self.process_transaction(tx).await;
                            let _ = reply.send(result);
                        }
                        AccountMessage::GetState { reply } => {
                            let _ = reply.send(self.account.clone());
                        }
                        AccountMessage::MigrateCold => {
                            if let Err(e) = self.migrate_old_transactions().await {
                                error!(
                                    client_id = self.client_id,
                                    error = ?e,
                                    "Failed to migrate old transactions"
                                );
                            }
                        }
                        AccountMessage::Shutdown => break,
                    }
                }
                
                // Automatic periodic migration
                _ = migration_timer.tick() => {
                    if let Err(e) = self.migrate_old_transactions().await {
                        error!(
                            client_id = self.client_id,
                            error = ?e,
                            "Failed to migrate old transactions during periodic check"
                        );
                    }
                }
                
                // Check for idle timeout
                _ = idle_check_timer.tick() => {
                    let idle_duration = SystemTime::now()
                        .duration_since(self.last_activity)
                        .unwrap_or(Duration::ZERO);
                    
                    if idle_duration > self.idle_timeout {
                        tracing::info!(
                            "Actor for client {} idle for {:?}, shutting down",
                            self.client_id,
                            idle_duration
                        );
                        break; // Self-terminate
                    }
                }
            }
        }
        
        tracing::debug!("Actor for client {} terminated", self.client_id);
    }
    
    /// Migrate old transactions from hot to cold storage
    async fn migrate_old_transactions(&mut self) -> Result<(), ProcessingError> {
        let cutoff = SystemTime::now() - Duration::from_secs(self.hot_cutoff_days * 24 * 3600);
        
        let to_migrate: Vec<_> = self.hot_transactions.iter()
            .filter(|(_, tx)| tx.created_at < cutoff)
            .map(|(id, tx)| (*id, tx.clone()))
            .collect();
        
        for (tx_id, tx) in to_migrate {
            match self.cold_storage.put(tx_id, tx).await {
                Ok(_) => {
                    self.hot_transactions.remove(&tx_id);
                }
                Err(e) => {
                    error!(
                        client_id = self.client_id,
                        tx_id = tx_id,
                        error = ?e,
                        "Failed to migrate transaction to cold storage - keeping in hot storage"
                    );
                }
            }
        }
        
        Ok(())
    }
    
    async fn process_transaction(&mut self, tx: TransactionRow) -> Result<(), ProcessingError> {
        match tx.tx_type {
            TransactionType::Deposit => self.process_deposit(tx),
            TransactionType::Withdrawal => self.process_withdrawal(tx),
            TransactionType::Dispute => self.process_dispute(tx).await,
            TransactionType::Resolve => self.process_resolve(tx).await,
            TransactionType::Chargeback => self.process_chargeback(tx).await,
        }
    }
    
    fn validate_amount(&self, amount_opt: Option<Decimal>) -> Result<Decimal, ProcessingError> {
        let amount = amount_opt.ok_or(ProcessingError::MissingAmount)?;
        if amount <= Decimal::ZERO {
            return Err(ProcessingError::InvalidAmount);
        }
        Ok(amount)
    }
    
    fn store_transaction(&mut self, tx_id: u32, tx_type: TransactionType, amount: Decimal) {
        self.hot_transactions.insert(
            tx_id,
            StoredTransaction {
                client: self.client_id,
                tx_type,
                amount,
                disputed: false,
                held_amount: None,
                created_at: SystemTime::now(),
            },
        );
    }
    
    fn process_deposit(&mut self, tx: TransactionRow) -> Result<(), ProcessingError> {
        let amount = self.validate_amount(tx.amount)?;
        
        if self.account.locked {
            return Err(ProcessingError::AccountLocked);
        }
        
        self.account.available += amount;
        self.store_transaction(tx.tx, TransactionType::Deposit, amount);
        
        Ok(())
    }
    
    fn process_withdrawal(&mut self, tx: TransactionRow) -> Result<(), ProcessingError> {
        let amount = self.validate_amount(tx.amount)?;
        
        if self.account.locked {
            return Err(ProcessingError::AccountLocked);
        }
        
        if self.account.available < amount {
            return Err(ProcessingError::InsufficientFunds);
        }
        
        self.account.available -= amount;

        // Store withdrawal for audit trail (cannot be disputed)
        self.store_transaction(tx.tx, TransactionType::Withdrawal, amount);
        
        Ok(())
    }
    
    async fn get_stored_transaction(&self, tx_id: u32) -> Option<StoredTransaction> {
        if let Some(stored) = self.hot_transactions.get(&tx_id) {
            return Some(stored.clone());
        }
        
        self.cold_storage.get(tx_id).await
    }
    
    async fn update_stored_transaction(
        &mut self,
        tx_id: u32,
        stored: StoredTransaction,
    ) -> Result<(), ProcessingError> {
        if self.hot_transactions.contains_key(&tx_id) {
            self.hot_transactions.insert(tx_id, stored);
            return Ok(());
        }
        
        if let Err(e) = self.cold_storage.put(tx_id, stored).await {
            tracing::error!(
                client_id = self.client_id,
                tx_id = tx_id,
                error = ?e,
                "Failed to update transaction in cold storage"
            );
            return Err(ProcessingError::TransactionNotFound);
        }
        
        Ok(())
    }
    
    async fn remove_stored_transaction(&mut self, tx_id: u32) -> Result<(), ProcessingError> {
        if self.hot_transactions.contains_key(&tx_id) {
            self.hot_transactions.remove(&tx_id);
            return Ok(());
        }
        
        if let Err(e) = self.cold_storage.remove(tx_id).await {
            tracing::error!(
                client_id = self.client_id,
                tx_id = tx_id,
                error = ?e,
                "Failed to remove transaction from cold storage"
            );
        }
        
        Ok(())
    }
    
    async fn process_dispute(&mut self, tx: TransactionRow) -> Result<(), ProcessingError> {
        if self.account.locked {
            return Err(ProcessingError::AccountLocked);
        }
        
        let mut stored = self.get_stored_transaction(tx.tx).await
            .ok_or(ProcessingError::TransactionNotFound)?;
        
        if stored.client != self.client_id {
            return Err(ProcessingError::ClientMismatch);
        }
        
        // Only deposits can be disputed
        // Withdrawals are final and cannot be reversed
        if stored.tx_type != TransactionType::Deposit {
            return Err(ProcessingError::TransactionNotFound);
        }
        
        if stored.disputed {
            return Err(ProcessingError::AlreadyDisputed);
        }
        
        // Dispute full amount, available can go negative
        // This maintains total = available + held
        let dispute_amount = stored.amount;
        
        // Can go negative
        self.account.available -= dispute_amount; 
        self.account.held += dispute_amount;
        stored.disputed = true;
        stored.held_amount = Some(dispute_amount);
        
        self.update_stored_transaction(tx.tx, stored).await?;
        
        Ok(())
    }
    
    async fn process_resolve(&mut self, tx: TransactionRow) -> Result<(), ProcessingError> {
        // Block all operations on locked accounts
        if self.account.locked {
            return Err(ProcessingError::AccountLocked);
        }
        
        let mut stored = self.get_stored_transaction(tx.tx).await
            .ok_or(ProcessingError::TransactionNotFound)?;
        
        if stored.client != self.client_id {
            return Err(ProcessingError::ClientMismatch);
        }
        
        if !stored.disputed {
            return Err(ProcessingError::NotDisputed);
        }
        
        // Use the actual held amount, not the original deposit amount
        let amount_to_restore = stored.held_amount.unwrap_or(stored.amount);
        
        self.account.held -= amount_to_restore;
        self.account.available += amount_to_restore;
        stored.disputed = false;
        stored.held_amount = None;
        
        
        self.update_stored_transaction(tx.tx, stored).await?;
        
        Ok(())
    }
    
    async fn process_chargeback(&mut self, tx: TransactionRow) -> Result<(), ProcessingError> {
        //Block if already locked, first chargeback locks account
        if self.account.locked {
            return Err(ProcessingError::AccountLocked);
        }
        
        let stored = self.get_stored_transaction(tx.tx).await
            .ok_or(ProcessingError::TransactionNotFound)?;
        
        if stored.client != self.client_id {
            return Err(ProcessingError::ClientMismatch);
        }
        
        if !stored.disputed {
            return Err(ProcessingError::NotDisputed);
        }
        
        // Chargeback removes the held amount
        let held_amount = stored.held_amount.unwrap_or(Decimal::ZERO);
        
        self.account.held -= held_amount;

        // Total decreases automatically when held decreases
        self.account.locked = true;

        self.remove_stored_transaction(tx.tx).await?;
        
        Ok(())
    }
}

#[derive(Clone)]
pub struct AccountHandle {
    sender: mpsc::Sender<AccountMessage>,
}

impl AccountHandle {
    pub fn new(sender: mpsc::Sender<AccountMessage>) -> Self {
        Self { sender }
    }
    
    pub async fn process(&self, tx: TransactionRow) -> Result<(), ProcessingError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        
        self.sender
            .send(AccountMessage::Process { tx, reply: reply_tx })
            .await
            .map_err(|_| ProcessingError::ActorCommunicationError)?;
        
        reply_rx
            .await
            .map_err(|_| ProcessingError::ActorCommunicationError)?
    }
    
    pub async fn get_state(&self) -> Result<Account, ProcessingError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        
        self.sender
            .send(AccountMessage::GetState { reply: reply_tx })
            .await
            .map_err(|_| ProcessingError::ActorCommunicationError)?;
        
        reply_rx
            .await
            .map_err(|_| ProcessingError::ActorCommunicationError)
    }
}
