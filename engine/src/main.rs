use blunder_core::{AccountMeta, Bundle, Pubkey, Transaction};
use blunder_engine::IntegratedPipeline;
use clap::Parser;
use rustls::crypto::{ring::default_provider, CryptoProvider};
use std::net::SocketAddr;

#[derive(Parser)]
#[command(name = "blunder-tpu")]
struct Args {
    #[arg(short, long, default_value_t = 4)]
    workers: usize,

    #[arg(short, long)]
    external_scheduler: Option<String>,

    #[arg(long, default_value_t = false)]
    stress_test: bool,
}

#[tokio::main]
async fn main() {
    CryptoProvider::install_default(default_provider()).expect("failed to install crypto provider");

    let args = Args::parse();

    println!("=== Blunder TPU - MEV Optimizer ===");
    println!("Workers: {}", args.workers);

    let external_scheduler_addr = args
        .external_scheduler
        .as_ref()
        .and_then(|s| s.parse::<SocketAddr>().ok());

    if let Some(addr) = &external_scheduler_addr {
        println!("Using EXTERNAL scheduler at {}", addr);
    } else {
        println!("Using INTERNAL scheduler");
    }

    let pipeline = IntegratedPipeline::new(args.workers, external_scheduler_addr)
        .await
        .expect("Failed to create pipeline");

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    if args.stress_test {
        stress_test_continuous(&pipeline).await;
    } else {
        test_no_conflicts(&pipeline).await;
        test_conflicts(&pipeline).await;
        test_bundles(&pipeline).await;
        test_high_load(&pipeline).await;
        test_mixed(&pipeline).await;
        test_adaptive_switching(&pipeline).await;
    }

    println!("\n=== Shutting down... ===");
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    println!("Done!");
}

async fn test_no_conflicts(pipeline: &IntegratedPipeline) {
    let accounts: Vec<_> = (0..4).map(|_| Pubkey::new_unique()).collect();
    let txs = accounts
        .iter()
        .enumerate()
        .map(|(i, pubkey)| {
            Transaction::new(
                format!("tx_{}", i),
                vec![AccountMeta::new(*pubkey, true)],
                100_000,
                (5000 - i * 100) as u64,
            )
        })
        .collect();
    println!("Submitting 4 non-conflicting transactions");
    let _ = pipeline.process_batch(txs, vec![]).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
}

async fn test_conflicts(pipeline: &IntegratedPipeline) {
    let shared_account = Pubkey::new_unique();
    let txs = (0..4)
        .map(|i| {
            Transaction::new(
                format!("conflict_tx_{}", i),
                vec![AccountMeta::new(shared_account, true)],
                100_000,
                5000,
            )
        })
        .collect();
    println!("Submitting 4 transactions with the same account (conflicts expected)");
    let _ = pipeline.process_batch(txs, vec![]).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
}

async fn test_bundles(pipeline: &IntegratedPipeline) {
    let accounts: Vec<_> = (0..3).map(|_| Pubkey::new_unique()).collect();
    let bundle1 = Bundle::new(
        1,
        vec![
            Transaction::new(
                "bundle1_tx1".to_string(),
                vec![AccountMeta::new(accounts[0], true)],
                100_000,
                1000,
            ),
            Transaction::new(
                "bundle1_tx2".to_string(),
                vec![AccountMeta::new(accounts[1], true)],
                100_000,
                1000,
            ),
        ],
        100_000,
        "searcher1".to_string(),
    );
    let bundle2 = Bundle::new(
        2,
        vec![Transaction::new(
            "bundle2_tx1".to_string(),
            vec![AccountMeta::new(accounts[2], true)],
            100_000,
            1000,
        )],
        80_000,
        "searcher2".to_string(),
    );
    println!("Submitting 2 bundles");
    let _ = pipeline.process_batch(vec![], vec![bundle1, bundle2]).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
}

async fn test_high_load(pipeline: &IntegratedPipeline) {
    let txs = (0..50)
        .map(|i| {
            Transaction::new(
                format!("load_tx_{}", i),
                vec![AccountMeta::new(Pubkey::new_unique(), true)],
                100_000,
                (10000 - i * 10) as u64,
            )
        })
        .collect();
    println!("Submitting 50 transactions with varying priorities");
    let _ = pipeline.process_batch(txs, vec![]).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
}

async fn test_mixed(pipeline: &IntegratedPipeline) {
    let accounts: Vec<_> = (0..10).map(|_| Pubkey::new_unique()).collect();
    let bundles = (0..3)
        .map(|i| {
            Bundle::new(
                i as u64,
                vec![Transaction::new(
                    format!("mixed_bundle{}_tx1", i),
                    vec![AccountMeta::new(accounts[i], true)],
                    100_000,
                    1000,
                )],
                (100_000 - i * 10000) as u64,
                format!("searcher{}", i),
            )
        })
        .collect();
    let txs = (3..13)
        .map(|i| {
            Transaction::new(
                format!("mixed_tx_{}", i),
                vec![AccountMeta::new(accounts[i % 10], true)],
                100_000,
                (8000 - i * 200) as u64,
            )
        })
        .collect();
    println!("Submitting 3 bundles + 10 transactions");
    let _ = pipeline.process_batch(txs, bundles).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
}

async fn test_adaptive_switching(pipeline: &IntegratedPipeline) {
    println!("Testing adaptive scheduler behavior with increasing load...");

    println!("▶ Low Load (10 txs) - Expecting PrioGraph");
    let low_load_txs = (0..10)
        .map(|i| {
            Transaction::new(
                format!("low_tx_{}", i),
                vec![AccountMeta::new(Pubkey::new_unique(), true)],
                100_000,
                (5000 - i * 100) as u64,
            )
        })
        .collect();
    let _ = pipeline.process_batch(low_load_txs, vec![]).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    println!("▶ Medium Load (1000 txs) - Expecting PrioGraph");
    let medium_load_txs = (0..1000)
        .map(|i| {
            Transaction::new(
                format!("med_tx_{}", i),
                vec![AccountMeta::new(Pubkey::new_unique(), true)],
                100_000,
                (10000 - i * 5) as u64,
            )
        })
        .collect();
    let _ = pipeline.process_batch(medium_load_txs, vec![]).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    println!("▶ High Load (3000 txs) - Expecting Greedy");
    let high_load_txs = (0..3000)
        .map(|i| {
            Transaction::new(
                format!("high_tx_{}", i),
                vec![AccountMeta::new(Pubkey::new_unique(), true)],
                100_000,
                (15000 - i * 3) as u64,
            )
        })
        .collect();
    let _ = pipeline.process_batch(high_load_txs, vec![]).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

    println!("Adaptive switching test complete");
}

async fn stress_test_continuous(pipeline: &IntegratedPipeline) {
    use std::time::Instant;

    let start = Instant::now();
    let duration = tokio::time::Duration::from_secs(10);
    let mut batch_count = 0;
    let mut total_txs = 0;

    println!("Running stress test for 10 seconds...");

    while start.elapsed() < duration {
        let txs: Vec<Transaction> = (0..20)
            .map(|i| {
                Transaction::new(
                    format!("stress_tx_{}_{}", batch_count, i),
                    vec![AccountMeta::new(Pubkey::new_unique(), true)],
                    100_000,
                    (batch_count * 100 + i) as u64,
                )
            })
            .collect();

        total_txs += txs.len();

        if let Err(e) = pipeline.process_batch(txs, vec![]).await {
            eprintln!("Batch {} error: {}", batch_count, e);
        }

        batch_count += 1;
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    println!("Stress test results:");
    println!("Duration: {:?}", start.elapsed());
    println!("Batches: {}", batch_count);
    println!("Total txs: {}", total_txs);
    println!(
        "Throughput: {:.2} txs/sec",
        total_txs as f64 / start.elapsed().as_secs_f64()
    );
}
