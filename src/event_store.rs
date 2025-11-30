use crate::models::TransactionRow;
use anyhow::Result;
use std::path::PathBuf;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

/// Simple append-only event store using CSV format
pub struct EventStore {
    path: PathBuf,
    writer: Mutex<File>,
}

impl EventStore {
    pub async fn new(path: PathBuf) -> Result<Self> {
        // Create file if doesn't exist, append if exists
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;
        
        Ok(Self {
            path,
            writer: Mutex::new(file),
        })
    }
    
    /// Append transaction to event log
    pub async fn append(&self, tx: &TransactionRow) -> Result<()> {
        let mut writer = self.writer.lock().await;
        
        let line = format!(
            "{},{},{},{}\n",
            tx.tx_type_str(),
            tx.client,
            tx.tx,
            tx.amount.map(|a| a.to_string()).unwrap_or_default()
        );
        
        // TODO: add batched flushes for performance
        writer.write_all(line.as_bytes()).await?;
        
        Ok(())
    }
    
    /// Replay all events from the log
    pub async fn replay(&self) -> Result<Vec<TransactionRow>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        
        let file = File::open(&self.path).await?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();
        
        let mut transactions = Vec::new();
        
        // Skip header if exists
        if let Some(first_line) = lines.next_line().await? {
            if !first_line.starts_with("type") {

                if let Ok(tx) = parse_csv_line(&first_line) {
                    transactions.push(tx);
                }
            }
        }
        
        while let Some(line) = lines.next_line().await? {
            if let Ok(tx) = parse_csv_line(&line) {
                transactions.push(tx);
            }
        }
        
        Ok(transactions)
    }
}

fn parse_csv_line(line: &str) -> Result<TransactionRow> {
    use crate::models::parse_transaction_type;
    
    let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
    
    if parts.len() < 3 {
        anyhow::bail!("Invalid CSV line");
    }
    
    let tx_type = parse_transaction_type(parts[0])?;
    let client = parts[1].parse()?;
    let tx = parts[2].parse()?;
    let amount = if parts.len() > 3 && !parts[3].is_empty() {
        Some(parts[3].parse()?)
    } else {
        None
    };
    
    Ok(TransactionRow {
        tx_type,
        client,
        tx,
        amount,
    })
}
