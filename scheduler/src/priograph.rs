use blunder_core::{Bundle, Pubkey, Result, Scheduler, Transaction, WorkerAssignment};
use std::collections::{BinaryHeap, HashSet};

pub struct PrioGraphScheduler {
    worker_count: usize,
}

#[derive(Debug, Clone)]
struct Node {
    id: String,
    priority: u64,
    accounts: HashSet<Pubkey>,
    is_bundle: bool,
}

#[derive(Eq, PartialEq)]
struct PriorityNode {
    priority: u64,
    index: usize,
}

impl Ord for PriorityNode {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.priority.cmp(&other.priority)
    }
}

impl PartialOrd for PriorityNode {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PrioGraphScheduler {
    pub fn new(worker_count: usize) -> Self {
        Self { worker_count }
    }

    fn build_priority_graph(
        &self,
        bundles: &[Bundle],
        loose_txs: &[Transaction],
    ) -> (Vec<Node>, Vec<Vec<usize>>) {
        let mut nodes = Vec::new();

        for bundle in bundles {
            let cu = bundle.total_compute_units().max(1);
            nodes.push(Node {
                id: bundle.id.to_string(),
                priority: bundle.tip.saturating_mul(1_000_000).saturating_div(cu),
                accounts: bundle.all_accounts(),
                is_bundle: true,
            });
        }

        for tx in loose_txs {
            let cu = tx.compute_unit_limit.max(1);
            let fee = tx
                .compute_unit_price
                .saturating_mul(tx.compute_unit_limit)
                .saturating_div(1_000_000);
            nodes.push(Node {
                id: tx.signature.clone(),
                priority: fee.saturating_mul(1_000_000).saturating_div(cu),
                accounts: tx.all_accounts(),
                is_bundle: false,
            });
        }

        let n = nodes.len();
        let mut adj_list: Vec<Vec<usize>> = vec![Vec::new(); n];

        for i in 0..n {
            for j in (i + 1)..n {
                if !nodes[i].accounts.is_disjoint(&nodes[j].accounts) {
                    if nodes[i].priority > nodes[j].priority {
                        adj_list[i].push(j);
                    } else if nodes[j].priority > nodes[i].priority {
                        adj_list[j].push(i);
                    } else {
                        adj_list[i].push(j);
                    }
                }
            }
        }
        (nodes, adj_list)
    }

    fn topological_sort(&self, nodes: &[Node], adj_list: &[Vec<usize>]) -> Vec<usize> {
        let n = nodes.len();
        let mut in_degree = vec![0; n];

        for neighbors in adj_list {
            for &neighbor in neighbors {
                in_degree[neighbor] += 1;
            }
        }

        let mut heap: BinaryHeap<PriorityNode> = BinaryHeap::new();
        let mut sorted = Vec::new();

        for (i, &degree) in in_degree.iter().enumerate() {
            if degree == 0 {
                heap.push(PriorityNode {
                    priority: nodes[i].priority,
                    index: i,
                });
            }
        }

        while let Some(PriorityNode { index, .. }) = heap.pop() {
            sorted.push(index);

            for &neighbor in &adj_list[index] {
                in_degree[neighbor] -= 1;
                if in_degree[neighbor] == 0 {
                    heap.push(PriorityNode {
                        priority: nodes[neighbor].priority,
                        index: neighbor,
                    });
                }
            }
        }

        sorted
    }

    fn assign_workers(&self, nodes: &[Node], sorted: &[usize]) -> Vec<WorkerAssignment> {
        let mut assignments = Vec::new();
        let mut worker_locked: Vec<HashSet<Pubkey>> = vec![HashSet::new(); self.worker_count];
        let mut worker_loads = vec![0u64; self.worker_count];

        for &node_idx in sorted {
            let node = &nodes[node_idx];

            let available: Vec<usize> = (0..self.worker_count)
                .filter(|&w| node.accounts.is_disjoint(&worker_locked[w]))
                .collect();

            let worker_id = if !available.is_empty() {
                *available
                    .iter()
                    .min_by_key(|&&w| worker_loads[w])
                    .expect("available is non-empty")
            } else {
                worker_loads
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, &load)| load)
                    .map(|(id, _)| id)
                    .unwrap_or(0)
            };

            worker_locked[worker_id].extend(node.accounts.iter().copied());
            worker_loads[worker_id] += node.priority;

            assignments.push(WorkerAssignment {
                unit_id: node.id.clone(),
                worker_id,
                is_bundle: node.is_bundle,
            });
        }

        assignments
    }
}

impl Scheduler for PrioGraphScheduler {
    fn schedule(
        &self,
        bundles: Vec<Bundle>,
        loose_txs: Vec<Transaction>,
    ) -> Result<Vec<WorkerAssignment>> {
        if bundles.is_empty() && loose_txs.is_empty() {
            return Ok(Vec::new());
        }

        let (nodes, adj_list) = self.build_priority_graph(&bundles, &loose_txs);
        let sorted = self.topological_sort(&nodes, &adj_list);
        let assignments = self.assign_workers(&nodes, &sorted);

        Ok(assignments)
    }

    fn name(&self) -> &str {
        "PrioGraphScheduler"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blunder_core::AccountMeta;

    #[test]
    fn test_priograph_no_conflicts() {
        let scheduler = PrioGraphScheduler::new(4);

        let tx1 = Transaction::new(
            "tx1".to_string(),
            vec![AccountMeta::new(Pubkey::new_unique(), true)],
            100_000,
            5000,
        );

        let tx2 = Transaction::new(
            "tx2".to_string(),
            vec![AccountMeta::new(Pubkey::new_unique(), true)],
            100_000,
            3000,
        );

        let result = scheduler.schedule(vec![], vec![tx1, tx2]);
        assert!(result.is_ok());

        let assignments = result.unwrap();
        assert_eq!(assignments.len(), 2);
    }

    #[test]
    fn test_priograph_priority_ordering() {
        let scheduler = PrioGraphScheduler::new(4);

        let shared_account = Pubkey::new_unique();

        let high_priority = Transaction::new(
            "high".to_string(),
            vec![AccountMeta::new(shared_account, true)],
            100_000,
            10_000,
        );

        let low_priority = Transaction::new(
            "low".to_string(),
            vec![AccountMeta::new(shared_account, true)],
            100_000,
            1_000,
        );

        let result = scheduler.schedule(vec![], vec![low_priority, high_priority]);
        assert!(result.is_ok());

        let assignments = result.unwrap();
        assert_eq!(assignments.len(), 2);

        assert_eq!(assignments[0].unit_id, "high");
        assert_eq!(assignments[1].unit_id, "low");
    }

    #[test]
    fn test_priograph_load_balancing() {
        let scheduler = PrioGraphScheduler::new(4);

        let txs: Vec<_> = (0..4)
            .map(|i| {
                Transaction::new(
                    format!("tx{}", i),
                    vec![AccountMeta::new(Pubkey::new_unique(), true)],
                    100_000,
                    (5000 - i * 100) as u64,
                )
            })
            .collect();

        let result = scheduler.schedule(vec![], txs);
        assert!(result.is_ok());

        let assignments = result.unwrap();
        assert_eq!(assignments.len(), 4);

        let unique_workers: HashSet<_> = assignments.iter().map(|a| a.worker_id).collect();
        assert!(unique_workers.len() >= 2, "Should use multiple workers");
    }

    #[test]
    fn test_priograph_with_bundles() {
        let scheduler = PrioGraphScheduler::new(4);

        let pubkey1 = Pubkey::new_unique();
        let pubkey2 = Pubkey::new_unique();

        let tx1 = Transaction::new(
            "tx1".to_string(),
            vec![AccountMeta::new(pubkey1, true)],
            100_000,
            1000,
        );

        let tx2 = Transaction::new(
            "tx2".to_string(),
            vec![AccountMeta::new(pubkey2, true)],
            100_000,
            1000,
        );

        let bundle = Bundle::new(1, vec![tx1, tx2], 50_000, "searcher1".to_string());

        let result = scheduler.schedule(vec![bundle], vec![]);
        assert!(result.is_ok());

        let assignments = result.unwrap();
        assert_eq!(assignments.len(), 1);
        assert!(assignments[0].is_bundle);
    }

    #[test]
    fn test_priograph_complex_conflicts() {
        let scheduler = PrioGraphScheduler::new(4);

        let shared = Pubkey::new_unique();
        let unique1 = Pubkey::new_unique();
        let unique2 = Pubkey::new_unique();

        // Diamond pattern: A -> B,C -> D
        let tx_a = Transaction::new(
            "a".to_string(),
            vec![AccountMeta::new(shared, true)],
            100_000,
            10_000,
        );

        let tx_b = Transaction::new(
            "b".to_string(),
            vec![AccountMeta::new(unique1, true)],
            100_000,
            8_000,
        );

        let tx_c = Transaction::new(
            "c".to_string(),
            vec![AccountMeta::new(unique2, true)],
            100_000,
            8_000,
        );

        let tx_d = Transaction::new(
            "d".to_string(),
            vec![AccountMeta::new(shared, true)],
            100_000,
            5_000,
        );

        let result = scheduler.schedule(vec![], vec![tx_a, tx_b, tx_c, tx_d]);
        assert!(result.is_ok());

        let assignments = result.unwrap();
        assert_eq!(assignments.len(), 4);

        // A should be first (highest priority with shared account)
        assert_eq!(assignments[0].unit_id, "a");

        // D should be last (lowest priority with shared account)
        assert_eq!(assignments[3].unit_id, "d");
    }

    #[test]
    fn test_priograph_all_conflicting() {
        let scheduler = PrioGraphScheduler::new(4);

        let shared = Pubkey::new_unique();

        let txs: Vec<_> = (0..5)
            .map(|i| {
                Transaction::new(
                    format!("tx{}", i),
                    vec![AccountMeta::new(shared, true)],
                    100_000,
                    (10_000 - i * 1000) as u64,
                )
            })
            .collect();

        let result = scheduler.schedule(vec![], txs);
        assert!(result.is_ok());

        let assignments = result.unwrap();
        assert_eq!(assignments.len(), 5);

        // Should be ordered by priority (descending)
        for i in 0..4 {
            let curr_id: usize = assignments[i].unit_id[2..].parse().unwrap();
            let next_id: usize = assignments[i + 1].unit_id[2..].parse().unwrap();
            assert!(curr_id < next_id, "Should be ordered by priority");
        }
    }

    #[test]
    fn test_priograph_empty_input() {
        let scheduler = PrioGraphScheduler::new(4);
        let result = scheduler.schedule(vec![], vec![]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn test_priograph_mixed_bundles_transactions() {
        let scheduler = PrioGraphScheduler::new(4);

        let pubkey1 = Pubkey::new_unique();
        let pubkey2 = Pubkey::new_unique();

        let tx1 = Transaction::new(
            "tx1".to_string(),
            vec![AccountMeta::new(pubkey1, true)],
            100_000,
            8000,
        );

        let bundle_tx = Transaction::new(
            "bundle_tx".to_string(),
            vec![AccountMeta::new(pubkey2, true)],
            100_000,
            1000,
        );

        let bundle = Bundle::new(1, vec![bundle_tx], 100_000, "searcher1".to_string());

        let tx2 = Transaction::new(
            "tx2".to_string(),
            vec![AccountMeta::new(pubkey1, true)],
            100_000,
            2000,
        );

        let result = scheduler.schedule(vec![bundle], vec![tx1, tx2]);
        assert!(result.is_ok());

        let assignments = result.unwrap();
        assert_eq!(assignments.len(), 3);

        // Bundle (100k tip) should be first
        assert_eq!(assignments[0].unit_id, "1");
        assert!(assignments[0].is_bundle);
    }

    #[test]
    fn test_priograph_high_load() {
        let scheduler = PrioGraphScheduler::new(4);

        let txs: Vec<_> = (0..100)
            .map(|i| {
                Transaction::new(
                    format!("tx{}", i),
                    vec![AccountMeta::new(Pubkey::new_unique(), true)],
                    100_000,
                    (10_000 - i * 10) as u64,
                )
            })
            .collect();

        let result = scheduler.schedule(vec![], txs);
        assert!(result.is_ok());

        let assignments = result.unwrap();
        assert_eq!(assignments.len(), 100);

        // Should distribute across workers
        let unique_workers: HashSet<_> = assignments.iter().map(|a| a.worker_id).collect();
        assert!(
            unique_workers.len() >= 3,
            "Should use most workers for high load"
        );
    }
}
