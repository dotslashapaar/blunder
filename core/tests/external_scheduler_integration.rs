#[cfg(test)]
mod integration_tests {
    use blunder_core::{ExecutionUnitData, ExternalSchedulerClient};
    use once_cell::sync::OnceCell;
    use std::net::SocketAddr;

    // OnceCell static to ensure single initialization
    static CRYPTO_INIT: OnceCell<()> = OnceCell::new();

    fn init_crypto() {
        CRYPTO_INIT.get_or_init(|| {
            rustls::crypto::CryptoProvider::install_default(
                rustls::crypto::ring::default_provider(),
            )
            .expect("install rustls default CryptoProvider");
        });
    }

    #[tokio::test]
    #[ignore]
    async fn test_client_server_communication() {
        init_crypto();

        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();

        println!("Connecting to scheduler at {}...", addr);

        let client = ExternalSchedulerClient::connect(addr)
            .await
            .expect("Failed to connect - is scheduler running?");

        println!("Connected! Sending schedule request...");

        let items = vec![
            ExecutionUnitData::Transaction {
                id: "tx1".to_string(),
                priority: 1000,
                accounts: vec![[1u8; 32]],
            },
            ExecutionUnitData::Transaction {
                id: "tx2".to_string(),
                priority: 2000,
                accounts: vec![[2u8; 32]],
            },
            ExecutionUnitData::Transaction {
                id: "tx3".to_string(),
                priority: 3000,
                accounts: vec![[3u8; 32]],
            },
            ExecutionUnitData::Bundle {
                id: "bundle1".to_string(),
                tip: 100_000,
                accounts: vec![[4u8; 32], [5u8; 32]],
                num_transactions: 2,
            },
        ];

        let worker_count = 2;

        let assignments = client
            .schedule(items.clone(), worker_count)
            .await
            .expect("Failed to get schedule");

        println!("Received {} assignments:", assignments.len());
        for (i, assignment) in assignments.iter().enumerate() {
            println!(
                "  {}. {} → worker {} (is_bundle: {})",
                i + 1,
                assignment.unit_id,
                assignment.worker_id,
                assignment.is_bundle
            );
        }

        assert_eq!(assignments.len(), 4, "Should have 4 assignments");
        assert_eq!(assignments[0].worker_id, 0, "tx1 should go to worker 0");
        assert_eq!(assignments[1].worker_id, 1, "tx2 should go to worker 1");
        assert_eq!(assignments[2].worker_id, 0, "tx3 should go to worker 0");
        assert_eq!(assignments[3].worker_id, 1, "bundle1 should go to worker 1");
        assert_eq!(assignments[0].unit_id, "tx1");
        assert_eq!(assignments[1].unit_id, "tx2");
        assert_eq!(assignments[2].unit_id, "tx3");
        assert_eq!(assignments[3].unit_id, "bundle1");
        assert_eq!(
            assignments[3].is_bundle, true,
            "bundle1 should be marked as bundle"
        );

        println!("\nAll assertions passed!");
    }

    #[tokio::test]
    #[ignore]
    async fn test_multiple_requests() {
        init_crypto();

        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();

        let client = ExternalSchedulerClient::connect(addr)
            .await
            .expect("Failed to connect");

        println!("Testing multiple requests...");

        for request_num in 1..=3 {
            let items = vec![ExecutionUnitData::Transaction {
                id: format!("tx{}", request_num),
                priority: 1000,
                accounts: vec![],
            }];

            let assignments = client.schedule(items, 4).await.expect("Schedule failed");

            assert_eq!(assignments.len(), 1);
            println!("Request {} completed", request_num);
        }

        println!("Multiple requests test passed!");
    }
}
