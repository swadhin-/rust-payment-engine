use crate::models::{AccountOutput, TransactionRow};
use csv_async::AsyncReaderBuilder;
use futures::stream::Stream;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio_util::compat::TokioAsyncReadCompatExt;

/// Stream transactions from async reader
pub fn stream_transactions<R: AsyncRead + Unpin + Send + 'static>(
    reader: R,
) -> impl Stream<Item = Result<TransactionRow, csv_async::Error>> {
    let compat_reader = reader.compat();
    let csv_reader = AsyncReaderBuilder::new()
        .trim(csv_async::Trim::All)
        .flexible(true)
        .create_deserializer(compat_reader);
    
    csv_reader.into_deserialize::<TransactionRow>()
}

pub async fn write_accounts<W: AsyncWrite + Unpin>(
    mut writer: W,
    accounts: Vec<AccountOutput>,
) -> Result<(), anyhow::Error> {
    writer.write_all(b"client,available,held,total,locked\n").await?;
    
    for account in accounts {
        let line = format!(
            "{},{:.4},{:.4},{:.4},{}\n",
            account.client,
            account.available,
            account.held,
            account.total,
            account.locked
        );
        writer.write_all(line.as_bytes()).await?;
    }
    
    writer.flush().await?;
    Ok(())
}
