//! Item ID generation for external scheduler

use super::constants::*;

/// Unique item identifier (128-bit)
pub type ItemId = u128;

/// Thread-safe item ID generator
pub struct ItemIdGenerator {
    counter: std::sync::atomic::AtomicU64,
}

impl ItemIdGenerator {
    pub fn new() -> Self {
        Self {
            counter: std::sync::atomic::AtomicU64::new(1),
        }
    }

    pub fn next_tx(&self) -> ItemId {
        let seq = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        ((ITEM_TYPE_TRANSACTION as u128) << 64) | (seq as u128)
    }

    pub fn next_bundle(&self) -> ItemId {
        let seq = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        ((ITEM_TYPE_BUNDLE as u128) << 64) | (seq as u128)
    }

    pub fn item_type(id: ItemId) -> u8 {
        ((id >> 64) & 0xFF) as u8
    }

    pub fn sequence(id: ItemId) -> u64 {
        (id & 0xFFFFFFFFFFFFFFFF) as u64
    }
}

impl Default for ItemIdGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uniqueness() {
        let gen = ItemIdGenerator::new();

        let tx1 = gen.next_tx();
        let tx2 = gen.next_tx();
        let bundle1 = gen.next_bundle();
        let bundle2 = gen.next_bundle();

        assert_ne!(tx1, tx2);
        assert_ne!(bundle1, bundle2);
        assert_ne!(tx1, bundle1);
    }

    #[test]
    fn test_type_encoding() {
        let gen = ItemIdGenerator::new();

        let tx_id = gen.next_tx();
        let bundle_id = gen.next_bundle();

        assert_eq!(ItemIdGenerator::item_type(tx_id), ITEM_TYPE_TRANSACTION);
        assert_eq!(ItemIdGenerator::item_type(bundle_id), ITEM_TYPE_BUNDLE);
    }

    #[test]
    fn test_sequential() {
        let gen = ItemIdGenerator::new();

        let id1 = gen.next_tx();
        let id2 = gen.next_tx();

        let seq1 = ItemIdGenerator::sequence(id1);
        let seq2 = ItemIdGenerator::sequence(id2);

        assert_eq!(seq2, seq1 + 1);
    }
}
