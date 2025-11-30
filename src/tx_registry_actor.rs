use anyhow::Result;
use std::collections::HashSet;
use tokio::sync::{mpsc, oneshot};

/// Message types for transaction registry actor
pub enum TxRegistryMessage {
    Register {
        tx_id: u32,
        // true if new, false if duplicate (for duplicate, we reject the transaction)
        reply: oneshot::Sender<bool>, 
    },
    Unregister {
        tx_id: u32,
        // true if was present (for duplicate, we reject the transaction)
        reply: oneshot::Sender<bool>, 
    },
    Shutdown,
}

/// Actor managing a shard of transaction IDs
pub struct TxRegistryActor {
    seen_tx_ids: HashSet<u32>,
    receiver: mpsc::Receiver<TxRegistryMessage>,
}

impl TxRegistryActor {
    pub fn new(receiver: mpsc::Receiver<TxRegistryMessage>) -> Self {
        Self {
            seen_tx_ids: HashSet::new(),
            receiver,
        }
    }
    
    pub async fn run(mut self) {
        while let Some(msg) = self.receiver.recv().await {
            match msg {
                TxRegistryMessage::Register { tx_id, reply } => {
                    let is_new = self.seen_tx_ids.insert(tx_id);
                    let _ = reply.send(is_new);
                }
                TxRegistryMessage::Unregister { tx_id, reply } => {
                    let was_present = self.seen_tx_ids.remove(&tx_id);
                    let _ = reply.send(was_present);
                }
                TxRegistryMessage::Shutdown => break,
            }
        }
    }
}

#[derive(Clone)]
pub struct TxRegistryHandle {
    sender: mpsc::Sender<TxRegistryMessage>,
}

impl TxRegistryHandle {
    pub fn new(sender: mpsc::Sender<TxRegistryMessage>) -> Self {
        Self { sender }
    }
    
    pub async fn register(&self, tx_id: u32) -> Result<bool> {
        let (reply_tx, reply_rx) = oneshot::channel();
        
        self.sender
            .send(TxRegistryMessage::Register { tx_id, reply: reply_tx })
            .await?;
        
        Ok(reply_rx.await?)
    }
    
    pub async fn unregister(&self, tx_id: u32) -> Result<bool> {
        let (reply_tx, reply_rx) = oneshot::channel();
        
        self.sender
            .send(TxRegistryMessage::Unregister { tx_id, reply: reply_tx })
            .await?;
        
        Ok(reply_rx.await?)
    }
}

/// Sharded transaction registry with multiple actors for parallel processing
#[derive(Clone)]
pub struct ShardedTxRegistry {
    shards: Vec<TxRegistryHandle>,
}

impl ShardedTxRegistry {
    pub fn new(num_shards: usize) -> Self {
        let mut shards = Vec::new();
        
        for _ in 0..num_shards {
            let (tx, rx) = mpsc::channel(10_000);
            let handle = TxRegistryHandle::new(tx);
            let actor = TxRegistryActor::new(rx);
            
            tokio::spawn(async move {
                actor.run().await;
            });
            
            shards.push(handle);
        }
        
        Self { shards }
    }
    
    pub async fn register(&self, tx_id: u32) -> Result<bool> {
        // Route to appropriate shard by tx_id
        let shard_id = (tx_id as usize) % self.shards.len();
        self.shards[shard_id].register(tx_id).await
    }
    
    /// Unregister a transaction ID
    pub async fn unregister(&self, tx_id: u32) -> Result<bool> {
        let shard_id = (tx_id as usize) % self.shards.len();
        self.shards[shard_id].unregister(tx_id).await
    }
}
