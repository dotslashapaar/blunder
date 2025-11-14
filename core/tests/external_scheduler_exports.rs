use blunder_core::*;

#[test]
fn test_exports_available() {
    // Constants
    let _ = ITEM_TYPE_TRANSACTION;
    let _ = ITEM_TYPE_BUNDLE;

    // Generator
    let gen = ItemIdGenerator::new();
    let _ = gen.next_tx();

    // Message types
    let _: SchedulableItem;
    let _: WorkerBatch;
    let _: ItemResult;

    // Helper functions
    let accounts = vec![Pubkey::new_unique()];
    let _ = compute_bloom_filter(&accounts);
}
