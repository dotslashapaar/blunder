use blunder_core::{Bundle, Transaction};
use blunder_engine::IntegratedPipeline;
use once_cell::sync::OnceCell;
use std::net::SocketAddr;

static CRYPTO_INIT: OnceCell<()> = OnceCell::new();

fn init_crypto() {
    CRYPTO_INIT.get_or_init(|| {
        rustls::crypto::CryptoProvider::install_default(rustls::crypto::ring::default_provider())
            .expect("install crypto");
    });
}

#[tokio::test]
#[ignore] // external scheduler requires external server
async fn test_pipeline_with_external_scheduler() {
    init_crypto();

    let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    let pipeline = IntegratedPipeline::new(4, Some(addr))
        .await
        .expect("Failed to create pipeline");

    let transactions = vec![
        Transaction::new("tx1".to_string(), vec![], 100_000, 1000),
        Transaction::new("tx2".to_string(), vec![], 100_000, 1000),
    ];

    let bundles = vec![Bundle::new(1, vec![], 50_000, "searcher1".to_string())];

    let batch_result = pipeline
        .process_batch(transactions, bundles)
        .await
        .expect("batch processing");

    assert_eq!(batch_result.total_processed, 3);
}

#[tokio::test]
async fn test_pipeline_with_internal_scheduler() {
    init_crypto();

    let pipeline = IntegratedPipeline::new(2, None)
        .await
        .expect("Failed to create pipeline");

    let transactions = vec![
        Transaction::new("tx1".to_string(), vec![], 100_000, 1000),
        Transaction::new("tx2".to_string(), vec![], 100_000, 1000),
    ];

    let batch_result = pipeline
        .process_batch(transactions, vec![])
        .await
        .expect("batch processing");

    assert_eq!(batch_result.total_processed, 2);
}
