use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Clone)]
pub struct LoadMonitor {
    pending_count: Arc<AtomicUsize>,
    low_threshold: usize,
    high_threshold: usize,
}

impl LoadMonitor {
    pub fn new(low_threshold: usize, high_threshold: usize) -> Self {
        Self {
            pending_count: Arc::new(AtomicUsize::new(0)),
            low_threshold,
            high_threshold,
        }
    }

    pub fn update_pending(&self, count: usize) {
        self.pending_count.store(count, Ordering::Relaxed)
    }

    pub fn get_pending(&self) -> usize {
        self.pending_count.load(Ordering::Relaxed)
    }

    pub fn should_use_greedy(&self) -> bool {
        self.get_pending() > self.high_threshold
    }

    pub fn should_use_prio_graph(&self) -> bool {
        self.get_pending() < self.low_threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_thresholds() {
        let monitor = LoadMonitor::new(500, 2000);

        monitor.update_pending(100);
        assert!(monitor.should_use_prio_graph());
        assert!(!monitor.should_use_greedy());

        monitor.update_pending(3000);
        assert!(!monitor.should_use_prio_graph());
        assert!(monitor.should_use_greedy());
    }
}
