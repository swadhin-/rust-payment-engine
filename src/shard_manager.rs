use crate::account_actor::{AccountActor, AccountHandle};
use crate::errors::ProcessingError;
use crate::models::{Account, TransactionRow};
use crate::storage::TransactionStore;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

/// Manages multiple shards for parallel processing
pub struct ShardManager {
    shards: Vec<Arc<RwLock<Shard>>>,
    num_shards: usize,
    cold_storage: Arc<dyn TransactionStore>,
}

struct Shard {
    actors: HashMap<u16, AccountHandle>,
}

impl ShardManager {
    pub fn new(num_shards: usize, cold_storage: Arc<dyn TransactionStore>) -> Self {
        let shards = (0..num_shards)
            .map(|_| {
                Arc::new(RwLock::new(Shard {
                    actors: HashMap::new(),
                }))
            })
            .collect();
        
        Self {
            shards,
            num_shards,
            cold_storage,
        }
    }
    
    /// Get or create actor for a client
    async fn get_or_create_actor(&self, client_id: u16) -> AccountHandle {
        let shard_id = (client_id as usize) % self.num_shards;
        let shard = &self.shards[shard_id];
        
        // Check if actor exists (read lock)
        {
            let shard_lock = shard.read().await;
            if let Some(handle) = shard_lock.actors.get(&client_id) {
                return handle.clone();
            }
        }
        
        // Create new actor (write lock)
        let mut shard_lock = shard.write().await;
        
        // Double-check (another task might have created it)
        if let Some(handle) = shard_lock.actors.get(&client_id) {
            return handle.clone();
        }
        
        // Create new actor with cold storage
        let (tx, rx) = mpsc::channel(1000);
        let handle = AccountHandle::new(tx);
        
        let actor = AccountActor::new(client_id, rx, self.cold_storage.clone());

        tokio::spawn(async move {
            actor.run().await;
        });
        
        shard_lock.actors.insert(client_id, handle.clone());
        handle
    }
    
    pub async fn process(&self, tx: TransactionRow) -> Result<(), ProcessingError> {
        let actor = self.get_or_create_actor(tx.client).await;
        actor.process(tx).await
    }
    
    /// Get all account states parallelly
    pub async fn get_all_accounts(&self) -> Vec<Account> {
        use futures::future::join_all;
        
        let futures: Vec<_> = self
            .shards
            .iter()
            .map(|shard| async move {
                let shard_lock = shard.read().await;
                let mut shard_accounts = Vec::new();
                
                for handle in shard_lock.actors.values() {
                    if let Ok(account) = handle.get_state().await {
                        shard_accounts.push(account);
                    }
                }
                
                shard_accounts
            })
            .collect();
        
        let results = join_all(futures).await;
        results.into_iter().flatten().collect()
    }
    
    pub async fn get_account(&self, client_id: u16) -> Option<Account> {
        let shard_id = (client_id as usize) % self.num_shards;
        let shard = &self.shards[shard_id];
        
        let shard_lock = shard.read().await;
        if let Some(handle) = shard_lock.actors.get(&client_id) {
            handle.get_state().await.ok()
        } else {
            None
        }
    }
}
