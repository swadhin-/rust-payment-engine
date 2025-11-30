use crate::csv_io::{stream_transactions, write_accounts};
use crate::models::AccountOutput;
use crate::scalable_engine::ScalableEngine;
use crate::storage::{InMemoryStore, TransactionStore};
use anyhow::Result;
use futures::StreamExt;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::BufReader;

pub async fn run(input_path: PathBuf) -> Result<()> {
    // Clean up all old temp files from previous runs as they persist across runs
    let temp_dir = PathBuf::from("/tmp");
    if let Ok(mut entries) = tokio::fs::read_dir(&temp_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with("payments-engine-cli-") && name.ends_with(".log") {
                    let _ = tokio::fs::remove_file(entry.path()).await;
                }
            }
        }
    }
    
    // Create unique temporary event store to avoid race conditions
    let temp_log = PathBuf::from(format!(
        "/tmp/payments-engine-cli-{}.log",
        std::process::id()
    ));
    
    // Use in-memory cold storage for CLI (no persistence needed)
    let cold_storage: Arc<dyn TransactionStore> = Arc::new(InMemoryStore::new());
    
    // Initialize scalable engine with 16 shards for parallel processing
    let engine = ScalableEngine::new(temp_log.clone(), 16, cold_storage).await?;
    
    // Open and process input file
    let file = File::open(&input_path).await?;
    let reader = BufReader::new(file);
    let mut stream = stream_transactions(reader);
    
    while let Some(result) = stream.next().await {
        match result {
            Ok(row) => {
                // Process with scalable engine (parallel via actors)
                let _ = engine.process(row).await;
            }
            Err(_) => {
                // Ignore parse errors
            }
        }
    }
    
    let mut accounts: Vec<AccountOutput> = engine
        .get_accounts()
        .await
        .iter()
        .map(AccountOutput::from)
        .collect();
    
    // Sort accounts by client ID for simplicity
    accounts.sort_by_key(|a| a.client);

    write_accounts(tokio::io::stdout(), accounts).await?;
    
    let _ = tokio::fs::remove_file(&temp_log).await;
    
    Ok(())
}
