use anyhow::Result;
use clap::Parser;
use payments_engine::{cli, server};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "payments-engine")]
#[command(about = "Process payment transactions")]
enum Cli {
    #[command(name = "cli")]
    CliMode { input: PathBuf },
    /// Run TCP server
    #[command(name = "server")]
    Server {
        #[arg(long, default_value = "0.0.0.0:8080")]
        bind: String,
        #[arg(long, default_value = "1000")]
        max_connections: usize,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    
    if args.len() == 2 && !args[1].starts_with('-') {
        // Direct file argument as per spec, no logging for clean stdout
        cli::run(PathBuf::from(&args[1])).await?;
    } else {
        match Cli::parse() {
            Cli::CliMode { input } => {
                // CLI mode, no logging for clean stdout
                cli::run(input).await?;
            }
            Cli::Server {
                bind,
                max_connections,
            } => {
                // Initialize logging only for server mode
                tracing_subscriber::fmt()
                    .with_writer(std::io::stderr)
                    .with_env_filter(
                        EnvFilter::from_default_env()
                            .add_directive(tracing::Level::INFO.into()),
                    )
                    .init();
                
                server::run(bind, max_connections).await?;
            }
        }
    }
    
    Ok(())
}
