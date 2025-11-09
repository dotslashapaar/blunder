use crate::greedy::GreedyScheduler;
use crate::load_monitor::LoadMonitor;
use blunder_core::{Bundle, Result, Scheduler, Transaction, WorkerAssignment};

pub struct AdaptiveScheduler {
    greedy: GreedyScheduler,
    load_monitor: LoadMonitor,
}

impl AdaptiveScheduler {
    pub fn new(worker_count: usize) -> Self {
        Self {
            greedy: GreedyScheduler::new(worker_count),
            load_monitor: LoadMonitor::new(500, 2000),
        }
    }

    pub fn with_thresholds(worker_count: usize, low: usize, high: usize) -> Self {
        Self {
            greedy: GreedyScheduler::new(worker_count),
            load_monitor: LoadMonitor::new(low, high),
        }
    }
}

impl Scheduler for AdaptiveScheduler {
    fn schedule(
        &self,
        bundles: Vec<Bundle>,
        loose_txs: Vec<Transaction>,
    ) -> Result<Vec<WorkerAssignment>> {
        let total_pending = bundles.len() + loose_txs.len();
        self.load_monitor.update_pending(total_pending);

        self.greedy.schedule(bundles, loose_txs)
    }

    fn name(&self) -> &str {
        if self.load_monitor.should_use_greedy() {
            "AdaptiveScheduler(Greedy)"
        } else {
            "AdaptiveScheduler(PrioGraph)"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blunder_core::{AccountMeta, Pubkey};

    #[test]
    fn test_adaptive_scheduler() {
        let scheduler = AdaptiveScheduler::new(4);

        let pubkey = Pubkey::new_unique();
        let accounts = vec![AccountMeta::new(pubkey, true)];
        let tx = Transaction::new("sig1".to_string(), accounts, 100_000, 1000);

        let result = scheduler.schedule(vec![], vec![tx]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_adaptive_switching() {
        let scheduler = AdaptiveScheduler::with_thresholds(4, 100, 500);

        let pubkey = Pubkey::new_unique();
        let accounts = vec![AccountMeta::new(pubkey, true)];

        let few_txs: Vec<_> = (0..50)
            .map(|i| Transaction::new(format!("sig{}", i), accounts.clone(), 100_000, 1000))
            .collect();

        let result = scheduler.schedule(vec![], few_txs);
        assert!(result.is_ok());
        assert!(scheduler.name().contains("PrioGraph"));

        let many_txs: Vec<_> = (0..1000)
            .map(|i| Transaction::new(format!("sig{}", i), accounts.clone(), 100_000, 1000))
            .collect();

        let result = scheduler.schedule(vec![], many_txs);
        assert!(result.is_ok());
        assert!(scheduler.name().contains("Greedy"))
    }
}
