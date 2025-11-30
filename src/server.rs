use crate::csv_io::{stream_transactions, write_accounts};
use crate::models::AccountOutput;
use crate::scalable_engine::ScalableEngine;
use crate::storage::{InMemoryStore, TransactionStore};
use anyhow::Result;
use futures::StreamExt;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{BufReader, BufWriter};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;

pub async fn run(bind: String, max_connections: usize) -> Result<()> {
    tracing::info!("Server mode: binding to {}", bind);
    
    // Use in-memory cold storage for server
    let cold_storage: Arc<dyn TransactionStore> = Arc::new(InMemoryStore::new());
    
    let event_log_path = PathBuf::from("server_transactions.log");
    let engine = Arc::new(ScalableEngine::new(event_log_path, 16, cold_storage).await?);
    
    // Rebuild state from previous runs
    engine.rebuild_from_events().await?;
    
    let listener = TcpListener::bind(&bind).await?;
    let semaphore = Arc::new(Semaphore::new(max_connections));
    
    tracing::info!("Listening on {}, max {} connections", bind, max_connections);
    
    loop {
        let permit = semaphore.clone().acquire_owned().await?;
        let (socket, addr) = listener.accept().await?;
        tracing::info!("Accepted connection from {}", addr);
        
        let engine = engine.clone();
        
        tokio::spawn(async move {
            if let Err(e) = handle_connection(socket, engine).await {
                tracing::error!("Connection {} error: {}", addr, e);
            }
            drop(permit);
        });
    }
}

async fn handle_connection(
    socket: TcpStream,
    engine: Arc<ScalableEngine>,
) -> Result<()> {
    let (reader, writer) = socket.into_split();
    let reader = BufReader::new(reader);
    
    // Stream CSV from socket
    let mut stream = stream_transactions(reader);
    
    while let Some(result) = stream.next().await {
        match result {
            Ok(row) => {
                // Process via parallel actors
                let _ = engine.process(row).await;
            }
            Err(e) => {
                tracing::warn!("CSV parse error: {}", e);
            }
        }
    }
    
    // Read final state and return to client
    let mut accounts: Vec<AccountOutput> = engine
        .get_accounts()
        .await
        .iter()
        .map(AccountOutput::from)
        .collect();
    
    // Sort accounts by client ID for simplicity in CLI output
    accounts.sort_by_key(|a| a.client);
    
    let writer = BufWriter::new(writer);
    write_accounts(writer, accounts).await?;
    
    Ok(())
}
