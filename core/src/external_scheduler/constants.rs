//! Constants for external scheduler protocol

/// Maximum transactions per batch
pub const MAX_TRANSACTIONS_PER_BATCH: usize = 64;

/// Maximum accounts per item
pub const MAX_ACCOUNTS_PER_ITEM: usize = 64;

/// Bloom filter size in u64s (1024 bits for ~1.2% false positive rate at 64 accounts)
pub const BLOOM_FILTER_SIZE: usize = 16;

/// Queue sizes
pub const ENGINE_TO_SCHEDULER_QUEUE_SIZE: usize = 32768;
pub const SCHEDULER_TO_WORKER_QUEUE_SIZE: usize = 8192;
pub const WORKER_TO_SCHEDULER_QUEUE_SIZE: usize = 8192;
pub const PROGRESS_QUEUE_SIZE: usize = 1024;

/// Item types
pub const ITEM_TYPE_TRANSACTION: u8 = 0;
pub const ITEM_TYPE_BUNDLE: u8 = 1;

/// Item states
pub const STATE_ALLOCATED: u8 = 0;
pub const STATE_SENT: u8 = 1;
pub const STATE_PENDING: u8 = 2;
pub const STATE_SCHEDULED: u8 = 3;
pub const STATE_ACKNOWLEDGED: u8 = 4;
pub const STATE_EXECUTING: u8 = 5;
pub const STATE_COMPLETED: u8 = 6;
pub const STATE_FAILED: u8 = 7;

/// Execution policies
pub const POLICY_CONTINUE_ON_ERROR: u8 = 0;
pub const POLICY_STOP_ON_ERROR: u8 = 1;
pub const POLICY_ALL_OR_NOTHING: u8 = 2;

/// Bundle flags
pub const BUNDLE_FLAG_ATOMIC: u8 = 1 << 0;
pub const BUNDLE_FLAG_ALL_OR_NOTHING: u8 = 1 << 1;

/// Error codes
pub const ERROR_SUCCESS: u8 = 0;
pub const ERROR_SIMULATION_FAILED: u8 = 1;
pub const ERROR_EXECUTION_FAILED: u8 = 2;
pub const ERROR_TIMEOUT: u8 = 3;
pub const ERROR_DEADLOCK_AVERTED: u8 = 4;
pub const ERROR_NOT_EXECUTED: u8 = 5;
pub const ERROR_ACCOUNT_CONFLICT: u8 = 6;

/// Result values (for u8 bool)
pub const RESULT_SUCCESS: u8 = 1;
pub const RESULT_FAILURE: u8 = 0;
