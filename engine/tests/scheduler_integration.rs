use blunder_core::ExecutionUnitData;
use blunder_engine::SchedulerEngine;
use once_cell::sync::OnceCell;
extern crate rustls;

static CRYPTO_INIT: OnceCell<()> = OnceCell::new();

fn init_crypto() {
    CRYPTO_INIT.get_or_init(|| {
        rustls::crypto::CryptoProvider::install_default(rustls::crypto::ring::default_provider())
            .expect("install crypto");
    });
}

#[tokio::test]
#[ignore]
async fn test_external_scheduler_integration() {
    init_crypto();
    let addr = "127.0.0.1:8080".parse().unwrap();
    let engine = SchedulerEngine::new(4, Some(addr))
        .await
        .expect("Failed to connect");
    let items = vec![
        ExecutionUnitData::Transaction {
            id: "tx1".to_string(),
            priority: 1000,
            accounts: vec![[1u8; 32]],
        },
        ExecutionUnitData::Bundle {
            id: "bundle1".to_string(),
            tip: 50000,
            accounts: vec![[3u8; 32]],
            num_transactions: 2,
        },
    ];
    let assignments = engine.schedule(items).await.expect("Schedule failed");

    assert_eq!(assignments.len(), 2);
    assert_eq!(assignments[1].is_bundle, true);
}

#[tokio::test]
async fn test_internal_scheduler_integration() {
    let engine = SchedulerEngine::new(2, None)
        .await
        .expect("Failed to create");
    let items = vec![
        ExecutionUnitData::Transaction {
            id: "tx1".to_string(),
            priority: 1000,
            accounts: vec![],
        },
        ExecutionUnitData::Transaction {
            id: "tx2".to_string(),
            priority: 2000,
            accounts: vec![],
        },
        ExecutionUnitData::Transaction {
            id: "tx3".to_string(),
            priority: 3000,
            accounts: vec![],
        },
    ];
    let assignments = engine.schedule(items).await.expect("Schedule failed");
    assert_eq!(assignments.len(), 3);
    assert_eq!(assignments[0].worker_id, 0);
    assert_eq!(assignments[1].worker_id, 1);
    assert_eq!(assignments[2].worker_id, 0);
}
