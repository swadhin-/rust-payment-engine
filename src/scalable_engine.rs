use crate::errors::ProcessingError;
use crate::event_store::EventStore;
use crate::models::{Account, TransactionRow};
use crate::shard_manager::ShardManager;
use crate::storage::TransactionStore;
use crate::tx_registry_actor::ShardedTxRegistry;
use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct ScalableEngine {
    event_store: Arc<EventStore>,
    shard_manager: Arc<ShardManager>,
    tx_registry: ShardedTxRegistry,
}

impl ScalableEngine {
    pub async fn new(
        storage_path: PathBuf,
        num_shards: usize,
        cold_storage: Arc<dyn TransactionStore>,
    ) -> Result<Self> {
        let event_store = Arc::new(EventStore::new(storage_path).await?);
        let shard_manager = Arc::new(ShardManager::new(num_shards, cold_storage));
        let tx_registry = ShardedTxRegistry::new(num_shards);
        
        Ok(Self {
            event_store,
            shard_manager,
            tx_registry,
        })
    }
    
    /// Rebuild state from event log (on startup)
    pub async fn rebuild_from_events(&self) -> Result<()> {
        use crate::models::TransactionType;
        
        let events = self.event_store.replay().await?;
        
        for event in events {
            // Register TX ID only for deposits/withdrawals (consistent with process logic)
            let is_new_tx = matches!(event.tx_type, TransactionType::Deposit | TransactionType::Withdrawal);
            if is_new_tx {
                let _ = self.tx_registry.register(event.tx).await;
            }
            
            // Replay through shard manager (rebuilds actor state)
            let _ = self.shard_manager.process(event).await;
        }
        
        Ok(())
    }
    
    pub async fn process(&self, tx: TransactionRow) -> Result<(), ProcessingError> {
        use crate::models::TransactionType;
        
        // Check global TX ID uniqueness (only for deposit/withdrawal, they create new TXs)
        // Disputes/resolves/chargebacks reference existing TXs, so skip uniqueness check
        let is_new_tx = matches!(tx.tx_type, TransactionType::Deposit | TransactionType::Withdrawal);
        
        if is_new_tx {
            let is_new = self
                .tx_registry
                .register(tx.tx)
                .await
                .map_err(|_| ProcessingError::TransactionNotFound)?;
            
            if !is_new {
                return Err(ProcessingError::DuplicateTransaction);
            }
        }
        
        // Apply to account actor
        let result = self.shard_manager.process(tx.clone()).await;
        
        if let Err(e) = result {
            // Processing failed, unregister TX ID if it was a new transaction
            if is_new_tx {
                let _ = self.tx_registry.unregister(tx.tx).await;
            }
            return Err(e);
        }
        
        // Persist to event store only successfully processed transactions
        self.event_store
            .append(&tx)
            .await
            .map_err(|_| ProcessingError::TransactionNotFound)?;
        
        Ok(())
    }
    
    // TODO: won't scale, future improvement
    pub async fn get_accounts(&self) -> Vec<Account> {
        self.shard_manager.get_all_accounts().await
    }
    
    pub async fn get_account(&self, client_id: u16) -> Option<Account> {
        self.shard_manager.get_account(client_id).await
    }
}
