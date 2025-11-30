use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use payments_engine::{ScalableEngine, TransactionRow, TransactionType};
use rust_decimal_macros::dec;
use std::path::PathBuf;
use tokio::runtime::Runtime;

fn benchmark_parallel_processing(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("parallel_processing");
    
    for num_clients in [10, 100, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_clients),
            num_clients,
            |b, &num_clients| {
                b.to_async(&rt).iter(|| async move {
                    let temp_path = PathBuf::from(format!("/tmp/bench_{}.log", num_clients));
                    let engine = ScalableEngine::new(temp_path, 16).await.unwrap();
                    
                    for client_id in 1..=num_clients {
                        let _ = engine.process(TransactionRow {
                            tx_type: TransactionType::Deposit,
                            client: client_id,
                            tx: client_id as u32,
                            amount: Some(dec!(100.0)),
                        }).await;
                    }
                    
                    black_box(engine.get_accounts().await.len())
                });
            },
        );
    }
    
    group.finish();
}

fn benchmark_actor_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("actor_1000_transactions", |b| {
        b.to_async(&rt).iter(|| async {
            let temp_path = PathBuf::from("/tmp/bench_throughput.log");
            let engine = ScalableEngine::new(temp_path, 16).await.unwrap();
            
            for i in 1..=1000 {
                let _ = engine.process(TransactionRow {
                    tx_type: TransactionType::Deposit,
                    client: (i % 100) as u16 + 1,
                    tx: i,
                    amount: Some(dec!(1.0)),
                }).await;
            }
            
            black_box(engine.get_accounts().await.len())
        });
    });
}

criterion_group!(benches, benchmark_parallel_processing, benchmark_actor_throughput);
criterion_main!(benches);

