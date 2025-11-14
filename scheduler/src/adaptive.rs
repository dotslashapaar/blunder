use crate::{GreedyScheduler, LoadMonitor, PrioGraphScheduler};
use blunder_core::{Bundle, Result, Scheduler, Transaction, WorkerAssignment};

pub struct AdaptiveScheduler {
    greedy: GreedyScheduler,
    priograph: PrioGraphScheduler,
    load_monitor: LoadMonitor,
}

impl AdaptiveScheduler {
    pub fn new(worker_count: usize) -> Self {
        Self {
            greedy: GreedyScheduler::new(worker_count),
            priograph: PrioGraphScheduler::new(worker_count),
            load_monitor: LoadMonitor::new(500, 2000),
        }
    }

    fn choose_scheduler(&self, total_items: usize) -> &dyn Scheduler {
        self.load_monitor.update_pending(total_items);

        // Uncomment exactly one to force a scheduler for testing:

        // force greedy
        // return &self.greedy as &dyn Scheduler;

        // force priograph
        // return &self.priograph as &dyn Scheduler;

        if self.load_monitor.should_use_greedy() {
            println!(
                "  [Scheduler] Using GREEDY (high load: {} items)",
                total_items
            );
            &self.greedy
        } else {
            println!(
                "  [Scheduler] Using PRIOGRAPH (load: {} items)",
                total_items
            );
            &self.priograph
        }
    }
}

impl Scheduler for AdaptiveScheduler {
    fn schedule(
        &self,
        bundles: Vec<Bundle>,
        loose_txs: Vec<Transaction>,
    ) -> Result<Vec<WorkerAssignment>> {
        let total_items = bundles.len() + loose_txs.len();
        let scheduler = self.choose_scheduler(total_items);
        scheduler.schedule(bundles, loose_txs)
    }

    fn name(&self) -> &str {
        "AdaptiveScheduler"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blunder_core::{AccountMeta, Pubkey};

    #[test]
    fn test_adaptive_low_load() {
        let scheduler = AdaptiveScheduler::new(4);
        let tx = Transaction::new(
            "tx1".to_string(),
            vec![AccountMeta::new(Pubkey::new_unique(), true)],
            100_000,
            1000,
        );
        let result = scheduler.schedule(vec![], vec![tx]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_adaptive_high_load() {
        let scheduler = AdaptiveScheduler::new(4);
        let txs: Vec<_> = (0..100)
            .map(|i| {
                Transaction::new(
                    format!("tx{}", i),
                    vec![AccountMeta::new(Pubkey::new_unique(), true)],
                    100_000,
                    1000,
                )
            })
            .collect();
        let result = scheduler.schedule(vec![], txs);
        assert!(result.is_ok());
    }

    #[test]
    fn test_adaptive_uses_priograph_for_low_load() {
        let scheduler = AdaptiveScheduler::new(4);
        let txs: Vec<_> = (0..10)
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
        assert_eq!(assignments.len(), 10);

        let unique_workers: std::collections::HashSet<_> =
            assignments.iter().map(|a| a.worker_id).collect();
        assert!(
            unique_workers.len() >= 2,
            "PrioGraph should use multiple workers"
        );
    }

    #[test]
    fn test_adaptive_uses_greedy_for_high_load() {
        let scheduler = AdaptiveScheduler::new(4);
        let txs: Vec<_> = (0..3000)
            .map(|i| {
                Transaction::new(
                    format!("tx{}", i),
                    vec![AccountMeta::new(Pubkey::new_unique(), true)],
                    100_000,
                    1000,
                )
            })
            .collect();
        let result = scheduler.schedule(vec![], txs);
        assert!(result.is_ok());

        let assignments = result.unwrap();
        assert_eq!(assignments.len(), 3000);
    }

    #[test]
    fn test_adaptive_medium_load() {
        let scheduler = AdaptiveScheduler::new(4);
        let txs: Vec<_> = (0..1000)
            .map(|i| {
                Transaction::new(
                    format!("tx{}", i),
                    vec![AccountMeta::new(Pubkey::new_unique(), true)],
                    100_000,
                    1000,
                )
            })
            .collect();
        let result = scheduler.schedule(vec![], txs);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1000);
    }

    #[test]
    fn test_adaptive_with_bundles() {
        let scheduler = AdaptiveScheduler::new(4);
        let pubkey = Pubkey::new_unique();
        let tx = Transaction::new(
            "tx1".to_string(),
            vec![AccountMeta::new(pubkey, true)],
            100_000,
            1000,
        );
        let bundle = Bundle::new(1, vec![tx], 50_000, "searcher1".to_string());
        let result = scheduler.schedule(vec![bundle], vec![]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn test_adaptive_empty() {
        let scheduler = AdaptiveScheduler::new(4);
        let result = scheduler.schedule(vec![], vec![]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn test_adaptive_threshold_boundaries() {
        let scheduler = AdaptiveScheduler::new(4);
        let txs_500: Vec<_> = (0..500)
            .map(|i| {
                Transaction::new(
                    format!("tx{}", i),
                    vec![AccountMeta::new(Pubkey::new_unique(), true)],
                    100_000,
                    1000,
                )
            })
            .collect();
        let result = scheduler.schedule(vec![], txs_500);
        assert!(result.is_ok());

        let txs_2000: Vec<_> = (0..2000)
            .map(|i| {
                Transaction::new(
                    format!("tx{}", i),
                    vec![AccountMeta::new(Pubkey::new_unique(), true)],
                    100_000,
                    1000,
                )
            })
            .collect();
        let result = scheduler.schedule(vec![], txs_2000);
        assert!(result.is_ok());
    }
}
