use payments_engine::{ScalableEngine, TransactionRow, TransactionType};
use payments_engine::storage::{InMemoryStore, TransactionStore};
use rust_decimal_macros::dec;
use std::sync::Arc;
use tempfile::TempDir;

// ============================================================================
// EVENT STORE & PERSISTENCE TESTS
// ============================================================================

#[tokio::test]
async fn test_event_store_persistence_and_recovery() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("test.log");
    
    // Create engine and process transactions
    {
        let cold_storage: Arc<dyn TransactionStore> = Arc::new(InMemoryStore::new());
        let engine = ScalableEngine::new(log_path.clone(), 4, cold_storage).await.unwrap();
        
        engine.process(TransactionRow {
            tx_type: TransactionType::Deposit,
            client: 1,
            tx: 1,
            amount: Some(dec!(100.0)),
        }).await.unwrap();
        
        engine.process(TransactionRow {
            tx_type: TransactionType::Withdrawal,
            client: 1,
            tx: 2,
            amount: Some(dec!(30.0)),
        }).await.unwrap();
        
        let accounts = engine.get_accounts().await;
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].available, dec!(70.0));
    }
    
    // Create new engine and rebuild from log (crash recovery simulation)
    {
        let cold_storage: Arc<dyn TransactionStore> = Arc::new(InMemoryStore::new());
        let engine = ScalableEngine::new(log_path.clone(), 4, cold_storage).await.unwrap();
        engine.rebuild_from_events().await.unwrap();
        
        let accounts = engine.get_accounts().await;
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].available, dec!(70.0));
    }
}

// ============================================================================
// PARALLEL PROCESSING & SCALABILITY TESTS
// ============================================================================

#[tokio::test]
async fn test_parallel_processing_different_clients() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("parallel.log");
    
    let cold_storage: Arc<dyn TransactionStore> = Arc::new(InMemoryStore::new());
    let engine = ScalableEngine::new(log_path, 16, cold_storage).await.unwrap();
    
    // Process transactions for different clients in parallel
    let mut handles = vec![];
    
    for client_id in 1..=10 {
        let engine_clone = engine.clone();
        let handle = tokio::spawn(async move {
            for tx_id in 1..=100 {
                let _ = engine_clone.process(TransactionRow {
                    tx_type: TransactionType::Deposit,
                    client: client_id,
                    tx: (client_id as u32) * 1000 + tx_id,
                    amount: Some(dec!(1.0)),
                }).await;
            }
        });
        handles.push(handle);
    }
    
    // Wait for all to complete
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Verify all clients have correct balance (no race conditions)
    let accounts = engine.get_accounts().await;
    assert_eq!(accounts.len(), 10);
    
    for account in accounts {
        assert_eq!(account.available, dec!(100.0));
    }
}

// ============================================================================
// ACTOR ISOLATION TESTS
// ============================================================================

#[tokio::test]
async fn test_actor_isolation() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("isolation.log");
    
    let cold_storage: Arc<dyn TransactionStore> = Arc::new(InMemoryStore::new());
    let engine = ScalableEngine::new(log_path, 4, cold_storage).await.unwrap();
    
    // Process for client 1
    engine.process(TransactionRow {
        tx_type: TransactionType::Deposit,
        client: 1,
        tx: 1,
        amount: Some(dec!(100.0)),
    }).await.unwrap();
    
    // Process for client 2
    engine.process(TransactionRow {
        tx_type: TransactionType::Deposit,
        client: 2,
        tx: 2,
        amount: Some(dec!(200.0)),
    }).await.unwrap();
    
    // Dispute for client 1 shouldn't affect client 2
    engine.process(TransactionRow {
        tx_type: TransactionType::Dispute,
        client: 1,
        tx: 1,
        amount: None,
    }).await.unwrap();
    
    let accounts = engine.get_accounts().await;
    
    // Client 1: disputed
    let client1 = accounts.iter().find(|a| a.client == 1).unwrap();
    assert_eq!(client1.available, dec!(0.0));
    assert_eq!(client1.held, dec!(100.0));
    
    // Client 2: unaffected
    let client2 = accounts.iter().find(|a| a.client == 2).unwrap();
    assert_eq!(client2.available, dec!(200.0));
    assert_eq!(client2.held, dec!(0.0));
}

// ============================================================================
// TRANSACTION REGISTRY TESTS
// ============================================================================

#[tokio::test]
async fn test_duplicate_transaction_rejection() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("duplicate.log");
    
    let cold_storage: Arc<dyn TransactionStore> = Arc::new(InMemoryStore::new());
    let engine = ScalableEngine::new(log_path, 4, cold_storage).await.unwrap();
    
    // First deposit
    engine.process(TransactionRow {
        tx_type: TransactionType::Deposit,
        client: 1,
        tx: 100,
        amount: Some(dec!(50.0)),
    }).await.unwrap();
    
    // Duplicate deposit with same tx ID - should be rejected
    let result = engine.process(TransactionRow {
        tx_type: TransactionType::Deposit,
        client: 1,
        tx: 100,
        amount: Some(dec!(75.0)),
    }).await;
    
    assert!(result.is_err());
    
    // Verify only first deposit was processed
    let account = engine.get_account(1).await.unwrap();
    assert_eq!(account.available, dec!(50.0));
}

// ============================================================================
// INTEGRATION TEST: NEGATIVE BALANCE HANDLING
// ============================================================================

#[tokio::test]
async fn test_negative_balance_from_dispute() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("negative.log");
    
    let cold_storage: Arc<dyn TransactionStore> = Arc::new(InMemoryStore::new());
    let engine = ScalableEngine::new(log_path, 4, cold_storage).await.unwrap();
    
    // Deposit $100, withdraw $60, then dispute the deposit
    engine.process(TransactionRow {
        tx_type: TransactionType::Deposit,
        client: 1,
        tx: 1,
        amount: Some(dec!(100.0)),
    }).await.unwrap();
    
    engine.process(TransactionRow {
        tx_type: TransactionType::Withdrawal,
        client: 1,
        tx: 2,
        amount: Some(dec!(60.0)),
    }).await.unwrap();
    
    // Full dispute allowed - available can go negative
    let result = engine.process(TransactionRow {
        tx_type: TransactionType::Dispute,
        client: 1,
        tx: 1,
        amount: None,
    }).await;
    
    assert!(result.is_ok());
    
    // Verify negative balance is handled correctly
    let account = engine.get_account(1).await.unwrap();
    assert_eq!(account.available, dec!(-60.0));  // Negative balance (overdraft)
    assert_eq!(account.held, dec!(100.0));       // Full dispute amount held
    assert_eq!(account.available + account.held, dec!(40.0));  // Invariant maintained
}
