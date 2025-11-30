use anyhow::Result;
use async_trait::async_trait;
use crate::models::TransactionType;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;

/// Stored transaction with timestamp for hot/cold tiering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTransaction {
    pub client: u16,
    pub tx_type: TransactionType,
    pub amount: Decimal,
    pub disputed: bool,
    #[serde(default)]
    pub held_amount: Option<Decimal>,
    #[serde(with = "systemtime_serde")]
    pub created_at: SystemTime,
}

mod systemtime_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    pub fn serialize<S>(time: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let duration = time.duration_since(UNIX_EPOCH)
            .map_err(|_| serde::ser::Error::custom("SystemTime before Unix epoch"))?;
        duration.as_secs().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SystemTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(UNIX_EPOCH + Duration::from_secs(secs))
    }
}

/// Trait for transaction storage backends
#[async_trait]
pub trait TransactionStore: Send + Sync {
    async fn get(&self, tx_id: u32) -> Option<StoredTransaction>;
    async fn put(&self, tx_id: u32, tx: StoredTransaction) -> Result<()>;
    async fn remove(&self, tx_id: u32) -> Result<()>;
}

/// In-memory storage (simple, fast, no persistence needed for cold tier in CLI mode)
pub struct InMemoryStore {
    cache: Arc<RwLock<HashMap<u32, StoredTransaction>>>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl TransactionStore for InMemoryStore {
    async fn get(&self, tx_id: u32) -> Option<StoredTransaction> {
        let cache = self.cache.read().await;
        cache.get(&tx_id).cloned()
    }
    
    async fn put(&self, tx_id: u32, tx: StoredTransaction) -> Result<()> {
        let mut cache = self.cache.write().await;
        cache.insert(tx_id, tx);
        Ok(())
    }
    
    async fn remove(&self, tx_id: u32) -> Result<()> {
        let mut cache = self.cache.write().await;
        cache.remove(&tx_id);
        Ok(())
    }
}
