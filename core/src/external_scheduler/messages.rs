//! Message types for external scheduler communication

use super::constants::*;
use super::id_generator::ItemId;
use crate::Pubkey;

pub type BloomFilter = [u64; BLOOM_FILTER_SIZE];

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SchedulableItem {
    pub item_id: ItemId,
    pub priority: u64,
    pub accounts_offset: usize,
    pub data_offset: usize,
    pub account_bloom: BloomFilter,
    pub num_accounts: u32,
    pub data_length: u32,
    pub item_type: u8,
    pub num_transactions: u8,
    pub bundle_flags: u8,
    pub _reserved: [u8; 13],
}

const _: () = assert!(std::mem::size_of::<SchedulableItem>() == 96);

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct WorkerBatch {
    pub batch_id: u64,
    pub worker_id: u8,
    pub batch_size: u8,
    pub execution_policy: u8,
    pub flags: u8,
    pub items_offset: usize,
    pub max_compute_units: u64,
    pub deadline_us: u64,
    pub _reserved: [u8; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BatchItem {
    pub item_id: ItemId,
    pub item_type: u8,
    pub data_offset: usize,
    pub data_length: u32,
    pub _reserved: [u8; 3],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct WorkerBatchAck {
    pub batch_id: u64,
    pub worker_id: u8,
    pub queue_depth: u32,
    pub timestamp_us: u64,
    pub _reserved: [u8; 3],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct WorkerResult {
    pub batch_id: u64,
    pub worker_id: u8,
    pub results_offset: usize,
    pub num_results: u8,
    pub total_compute_units: u64,
    pub total_time_us: u64,
    pub _reserved: [u8; 6],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ItemResult {
    pub item_id: ItemId,
    pub success: u8,
    pub error_code: u8,
    pub compute_units: u64,
    pub execution_time_us: u64,
    pub _reserved: [u8; 6],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ProgressUpdate {
    pub block_space_used: u64,
    pub block_space_total: u64,
    pub active_workers: u8,
    pub idle_workers: u8,
    pub worker_queue_depths: [u32; 4],
    pub items_per_second: u32,
    pub timestamp_us: u64,
    pub _reserved: [u8; 6],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SchedulerHeartbeat {
    pub timestamp_us: u64,
    pub items_pending: u32,
    pub items_scheduled: u32,
    pub items_completed: u32,
    pub _reserved: [u8; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ItemCompletionAck {
    pub item_id: ItemId,
    pub can_free: u8,
    pub _reserved: [u8; 7],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ShutdownMessage {
    pub reason: u8,
    pub _reserved: [u8; 7],
}

impl SchedulableItem {
    pub fn from_transaction(
        item_id: ItemId,
        priority: u64,
        accounts: &[Pubkey],
        data_offset: usize,
        data_length: u32,
    ) -> Self {
        Self {
            item_id,
            item_type: ITEM_TYPE_TRANSACTION,
            priority,
            account_bloom: compute_bloom_filter(accounts),
            accounts_offset: 0,
            num_accounts: accounts.len() as u32,
            data_offset,
            data_length,
            num_transactions: 0,
            bundle_flags: 0,
            _reserved: [0; 13],
        }
    }

    pub fn from_bundle(
        item_id: ItemId,
        tip: u64,
        accounts: &[Pubkey],
        data_offset: usize,
        data_length: u32,
        num_txs: u8,
        atomic: bool,
    ) -> Self {
        let mut flags = 0;
        if atomic {
            flags |= BUNDLE_FLAG_ATOMIC | BUNDLE_FLAG_ALL_OR_NOTHING;
        }

        Self {
            item_id,
            item_type: ITEM_TYPE_BUNDLE,
            priority: tip,
            account_bloom: compute_bloom_filter(accounts),
            accounts_offset: 0,
            num_accounts: accounts.len() as u32,
            data_offset,
            data_length,
            num_transactions: num_txs,
            bundle_flags: flags,
            _reserved: [0; 13],
        }
    }

    pub fn is_bundle(&self) -> bool {
        self.item_type == ITEM_TYPE_BUNDLE
    }

    pub fn is_atomic(&self) -> bool {
        (self.bundle_flags & BUNDLE_FLAG_ATOMIC) != 0
    }
}

impl ItemResult {
    pub fn is_success(&self) -> bool {
        self.success == RESULT_SUCCESS
    }
}

impl ItemCompletionAck {
    pub fn can_free_memory(&self) -> bool {
        self.can_free == RESULT_SUCCESS
    }
}

pub fn compute_bloom_filter(accounts: &[Pubkey]) -> BloomFilter {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut bloom = [0u64; BLOOM_FILTER_SIZE];

    for account in accounts {
        for seed in 0..3 {
            let mut hasher = DefaultHasher::new();
            seed.hash(&mut hasher);
            account.hash(&mut hasher);
            let hash = hasher.finish();

            let word_index = ((hash as u32) % (BLOOM_FILTER_SIZE as u32)) as usize;
            let bit_index = ((hash >> 32) % 64) as usize;

            bloom[word_index] |= 1u64 << bit_index;
        }
    }

    bloom
}

pub fn blooms_might_conflict(a: &BloomFilter, b: &BloomFilter) -> bool {
    for i in 0..BLOOM_FILTER_SIZE {
        if (a[i] & b[i]) != 0 {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schedulable_item_size() {
        assert_eq!(std::mem::size_of::<SchedulableItem>(), 96);
    }

    #[test]
    fn test_create_transaction_item() {
        let accounts = vec![Pubkey::new_unique(), Pubkey::new_unique()];
        let item = SchedulableItem::from_transaction(12345, 5000, &accounts, 0x1000, 256);

        assert_eq!(item.item_id, 12345);
        assert_eq!(item.item_type, ITEM_TYPE_TRANSACTION);
        assert!(!item.is_bundle());
    }

    #[test]
    fn test_create_bundle_item() {
        let accounts = vec![Pubkey::new_unique(); 3];
        let item = SchedulableItem::from_bundle(67890, 100_000, &accounts, 0x2000, 512, 3, true);

        assert_eq!(item.item_type, ITEM_TYPE_BUNDLE);
        assert!(item.is_bundle());
        assert!(item.is_atomic());
    }
}
