use blunder_core::{AccountMeta, Bundle, Pubkey, Transaction};
use blunder_engine::TpuPipeline;

#[tokio::main]
async fn main() {
    println!("=== Blunder TPU - MEV Optimizer ===");
    println!("Starting with 4 workers...\n");

    let pipeline = TpuPipeline::new(4);

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    println!("=== TEST 1: Basic Transactions (No Conflicts) ===");
    test_no_conflicts(&pipeline).await;

    println!("\n=== TEST 2: Conflicting Transactions ===");
    test_conflicts(&pipeline).await;

    println!("\n=== TEST 3: Bundles ===");
    test_bundles(&pipeline).await;

    println!("\n=== TEST 4: High Load (50 transactions) ===");
    test_high_load(&pipeline).await;

    println!("\n=== TEST 5: Mixed Bundles and Transactions ===");
    test_mixed(&pipeline).await;

    println!("\n=== TEST 6: Adaptive Scheduler Switching ===");
    test_adaptive_switching(&pipeline).await;

    // ============================================================================
    // STRESS TEST: Uncomment below to run continuous load simulation
    // ============================================================================
    
    // println!("\n=== STRESS TEST: Continuous Random Load ===");
    // println!("Running for 10 seconds...\n");
    // stress_test_continuous(&pipeline).await;

    println!("\n=== Shutting down... ===");
    pipeline.shutdown().await;
    
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    println!("Done!");
}

async fn test_no_conflicts(pipeline: &TpuPipeline) {
    let accounts: Vec<_> = (0..4)
        .map(|_| Pubkey::new_unique())
        .collect();

    let txs: Vec<_> = accounts
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
    if let Err(e) = pipeline.process_batch(vec![], txs).await {
        eprintln!("Error: {}", e);
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
}

async fn test_conflicts(pipeline: &TpuPipeline) {
    let shared_account = Pubkey::new_unique();

    let txs: Vec<_> = (0..4)
        .map(|i| {
            Transaction::new(
                format!("conflict_tx_{}", i),
                vec![AccountMeta::new(shared_account, true)],
                100_000,
                5000,
            )
        })
        .collect();

    println!("Submitting 4 transactions with same account (conflicts expected)");
    if let Err(e) = pipeline.process_batch(vec![], txs).await {
        eprintln!("Error: {}", e);
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
}

async fn test_bundles(pipeline: &TpuPipeline) {
    let accounts: Vec<_> = (0..3)
        .map(|_| Pubkey::new_unique())
        .collect();

    let bundle1_txs = vec![
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
    ];

    let bundle1 = Bundle::new(
        1,
        bundle1_txs,
        100_000,
        "searcher1".to_string(),
    );

    let bundle2_txs = vec![
        Transaction::new(
            "bundle2_tx1".to_string(),
            vec![AccountMeta::new(accounts[2], true)],
            100_000,
            1000,
        ),
        Transaction::new(
            "bundle2_tx2".to_string(),
            vec![AccountMeta::new(accounts[2], true)],
            100_000,
            1000,
        ),
        Transaction::new(
            "bundle2_tx3".to_string(),
            vec![AccountMeta::new(accounts[2], true)],
            100_000,
            1000,
        ),
    ];

    let bundle2 = Bundle::new(
        2,
        bundle2_txs,
        80_000,
        "searcher2".to_string(),
    );

    println!("Submitting 2 bundles (2 txs + 3 txs)");
    if let Err(e) = pipeline.process_batch(vec![bundle1, bundle2], vec![]).await {
        eprintln!("Error: {}", e);
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
}

async fn test_high_load(pipeline: &TpuPipeline) {
    let txs: Vec<_> = (0..50)
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
    if let Err(e) = pipeline.process_batch(vec![], txs).await {
        eprintln!("Error: {}", e);
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
}

async fn test_mixed(pipeline: &TpuPipeline) {
    let accounts: Vec<_> = (0..10)
        .map(|_| Pubkey::new_unique())
        .collect();

    let bundles: Vec<_> = (0..3)
        .map(|i| {
            let txs = vec![
                Transaction::new(
                    format!("mixed_bundle{}_tx1", i),
                    vec![AccountMeta::new(accounts[i], true)],
                    100_000,
                    1000,
                ),
                Transaction::new(
                    format!("mixed_bundle{}_tx2", i),
                    vec![AccountMeta::new(accounts[i + 1], true)],
                    100_000,
                    1000,
                ),
            ];
            Bundle::new(
                i as u64,
                txs,
                (100_000 - i * 10000) as u64,
                format!("searcher{}", i),
            )
        })
        .collect();

    let txs: Vec<_> = (3..13)
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
    if let Err(e) = pipeline.process_batch(bundles, txs).await {
        eprintln!("Error: {}", e);
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
}

async fn test_adaptive_switching(pipeline: &TpuPipeline) {
    println!("Testing adaptive scheduler behavior with increasing load...\n");

    // --- Low Load: 10 txs (< 500) → Should use PrioGraph ---
    println!("▶ Low Load (10 txs) - Expecting PrioGraph (distributed workers)");
    let low_load_txs: Vec<_> = (0..10)
        .map(|i| {
            Transaction::new(
                format!("low_tx_{}", i),
                vec![AccountMeta::new(Pubkey::new_unique(), true)],
                100_000,
                (5000 - i * 100) as u64,
            )
        })
        .collect();

    if let Err(e) = pipeline.process_batch(vec![], low_load_txs).await {
        eprintln!("Error: {}", e);
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    println!();

    // --- Medium Load: 1000 txs (500-2000) → Should use PrioGraph ---
    println!("▶ Medium Load (1000 txs) - Expecting PrioGraph (distributed workers)");
    let medium_load_txs: Vec<_> = (0..1000)
        .map(|i| {
            Transaction::new(
                format!("med_tx_{}", i),
                vec![AccountMeta::new(Pubkey::new_unique(), true)],
                100_000,
                (10000 - i * 5) as u64,
            )
        })
        .collect();

    if let Err(e) = pipeline.process_batch(vec![], medium_load_txs).await {
        eprintln!("Error: {}", e);
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    println!();

    // --- High Load: 3000 txs (> 2000) → Should use Greedy ---
    println!("▶ High Load (3000 txs) - Expecting Greedy (mostly Worker 0)");
    let high_load_txs: Vec<_> = (0..3000)
        .map(|i| {
            Transaction::new(
                format!("high_tx_{}", i),
                vec![AccountMeta::new(Pubkey::new_unique(), true)],
                100_000,
                (15000 - i * 3) as u64,
            )
        })
        .collect();

    if let Err(e) = pipeline.process_batch(vec![], high_load_txs).await {
        eprintln!("Error: {}", e);
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    println!();

    println!("✓ Adaptive switching test complete");
}

// ============================================================================
// STRESS TEST: Continuous Random Load Generation
// ============================================================================

#[allow(dead_code)]
async fn stress_test_continuous(pipeline: &TpuPipeline) {
    use std::time::Instant;
    
    let start = Instant::now();
    let duration = tokio::time::Duration::from_secs(10);
    let mut batch_count = 0;
    let mut total_txs = 0;
    let mut total_bundles = 0;
    
    let account_pool: Vec<Pubkey> = (0..20)
        .map(|_| Pubkey::new_unique())
        .collect();
    
    while start.elapsed() < duration {
        let (bundles, txs) = generate_random_batch(&account_pool, batch_count);
        
        total_bundles += bundles.len();
        total_txs += txs.len();
        
        if let Err(e) = pipeline.process_batch(bundles, txs).await {
            eprintln!("Batch {} error: {}", batch_count, e);
        }
        
        batch_count += 1;
        
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
    
    println!("\nStress Test Results:");
    println!("  Duration: {:?}", start.elapsed());
    println!("  Batches processed: {}", batch_count);
    println!("  Total bundles: {}", total_bundles);
    println!("  Total transactions: {}", total_txs);
    println!("  Total items: {}", total_bundles + total_txs);
    println!("  Throughput: {:.2} items/sec", 
             (total_bundles + total_txs) as f64 / start.elapsed().as_secs_f64());
}

#[allow(dead_code)]
fn generate_random_batch(
    account_pool: &[Pubkey],
    batch_id: usize,
) -> (Vec<Bundle>, Vec<Transaction>) {
    use std::time::{SystemTime, UNIX_EPOCH};
    
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as usize;
    
    let num_bundles = seed % 4;
    let num_txs = 5 + (seed % 11);
    
    let bundles: Vec<Bundle> = (0..num_bundles)
        .map(|i| {
            let bundle_size = 2 + ((seed + i) % 3);
            
            let txs: Vec<Transaction> = (0..bundle_size)
                .map(|j| {
                    let account = if (seed + i + j) % 10 < 3 {
                        account_pool[(seed + i) % account_pool.len()]
                    } else {
                        Pubkey::new_unique()
                    };
                    
                    Transaction::new(
                        format!("batch{}_bundle{}_tx{}", batch_id, i, j),
                        vec![AccountMeta::new(account, true)],
                        100_000,
                        ((seed + i * 1000 + j) % 10000) as u64,
                    )
                })
                .collect();
            
            Bundle::new(
                (batch_id * 100 + i) as u64,
                txs,
                ((seed + i * 5000) % 200000) as u64,
                format!("searcher_{}", (seed + i) % 5),
            )
        })
        .collect();
    
    let txs: Vec<Transaction> = (0..num_txs)
        .map(|i| {
            let account = if (seed + i) % 10 < 2 {
                account_pool[(seed + i) % account_pool.len()]
            } else {
                Pubkey::new_unique()
            };
            
            Transaction::new(
                format!("batch{}_tx{}", batch_id, i),
                vec![AccountMeta::new(account, true)],
                100_000,
                ((seed + i * 100) % 15000) as u64,
            )
        })
        .collect();
    
    (bundles, txs)
}
