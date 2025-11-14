//! Round-robin scheduling algorithm

use blunder_core::{ExecutionUnitData, WorkerAssignment};

pub struct RoundRobinScheduler {
    #[allow(dead_code)]
    next_worker: std::sync::atomic::AtomicUsize,
}

impl RoundRobinScheduler {
    pub fn new() -> Self {
        Self {
            next_worker: std::sync::atomic::AtomicUsize::new(0),
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

        items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let worker_id = i % worker_count;

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
    fn test_round_robin_basic() {
        let scheduler = RoundRobinScheduler::new();

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
        assert_eq!(assignments[0].worker_id, 0);
        assert_eq!(assignments[1].worker_id, 1);
        assert_eq!(assignments[2].worker_id, 0);
    }

    #[test]
    fn test_round_robin_bundle() {
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
        assert_eq!(assignments[0].is_bundle, true);
    }

    #[test]
    fn test_round_robin_zero_workers() {
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
