//! Round-robin scheduling algorithm with conflict detection

use blunder_core::{ExecutionUnitData, WorkerAssignment};
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Raw account key type used in ExecutionUnitData
type AccountKey = [u8; 32];

pub struct RoundRobinScheduler {
    next_worker: AtomicUsize,
}

impl RoundRobinScheduler {
    pub fn new() -> Self {
        Self {
            next_worker: AtomicUsize::new(0),
        }
    }

    /// Get accounts from an ExecutionUnitData item
    fn get_accounts(item: &ExecutionUnitData) -> HashSet<AccountKey> {
        match item {
            ExecutionUnitData::Transaction { accounts, .. } => accounts.iter().copied().collect(),
            ExecutionUnitData::Bundle { accounts, .. } => accounts.iter().copied().collect(),
        }
    }

    pub fn schedule(
        &self,
        items: &[ExecutionUnitData],
        worker_count: usize,
    ) -> Vec<WorkerAssignment> {
        if worker_count == 0 {
            return Vec::new();
        }

        // Track locked accounts per worker for conflict detection
        let mut worker_locked: Vec<HashSet<AccountKey>> = vec![HashSet::new(); worker_count];

        items
            .iter()
            .map(|item| {
                let item_accounts = Self::get_accounts(item);

                // Find first non-conflicting worker, always starting from worker 0
                // This ensures conflicting transactions pile up on the same worker
                let worker_id = (0..worker_count)
                    .find(|&wid| worker_locked[wid].is_disjoint(&item_accounts))
                    .unwrap_or_else(|| {
                        // All workers have conflicts - use round-robin fallback
                        self.next_worker.fetch_add(1, Ordering::Relaxed) % worker_count
                    });

                // Mark accounts as locked for this worker
                worker_locked[worker_id].extend(&item_accounts);

                WorkerAssignment {
                    unit_id: item.id().to_string(),
                    worker_id,
                    is_bundle: matches!(item, ExecutionUnitData::Bundle { .. }),
                }
            })
            .collect()
    }
}

impl Default for RoundRobinScheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_non_conflicting_same_worker() {
        let scheduler = RoundRobinScheduler::new();

        // Empty accounts = no conflicts, all go to first available (worker 0)
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

        let assignments = scheduler.schedule(&items, 2);

        assert_eq!(assignments.len(), 3);
        // All non-conflicting txs go to worker 0 (first available)
        assert_eq!(assignments[0].worker_id, 0);
        assert_eq!(assignments[1].worker_id, 0);
        assert_eq!(assignments[2].worker_id, 0);
    }

    #[test]
    fn test_conflicting_different_workers() {
        let scheduler = RoundRobinScheduler::new();

        let shared_account = [1u8; 32];

        // All txs touch the same account - should go to different workers
        let items = vec![
            ExecutionUnitData::Transaction {
                id: "tx1".to_string(),
                priority: 1000,
                accounts: vec![shared_account],
            },
            ExecutionUnitData::Transaction {
                id: "tx2".to_string(),
                priority: 2000,
                accounts: vec![shared_account],
            },
            ExecutionUnitData::Transaction {
                id: "tx3".to_string(),
                priority: 3000,
                accounts: vec![shared_account],
            },
        ];

        let assignments = scheduler.schedule(&items, 4);

        assert_eq!(assignments.len(), 3);
        // Conflicting txs should go to different workers
        assert_eq!(assignments[0].worker_id, 0);
        assert_eq!(assignments[1].worker_id, 1);
        assert_eq!(assignments[2].worker_id, 2);
    }

    #[test]
    fn test_bundle() {
        let scheduler = RoundRobinScheduler::new();

        let items = vec![ExecutionUnitData::Bundle {
            id: "bundle1".to_string(),
            tip: 100_000,
            accounts: vec![],
            num_transactions: 3,
        }];

        let assignments = scheduler.schedule(&items, 4);

        assert_eq!(assignments.len(), 1);
        assert_eq!(assignments[0].worker_id, 0);
        assert!(assignments[0].is_bundle);
    }

    #[test]
    fn test_zero_workers() {
        let scheduler = RoundRobinScheduler::new();

        let items = vec![ExecutionUnitData::Transaction {
            id: "tx1".to_string(),
            priority: 1000,
            accounts: vec![],
        }];

        let assignments = scheduler.schedule(&items, 0);

        assert_eq!(assignments.len(), 0);
    }
}
