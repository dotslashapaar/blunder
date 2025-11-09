use blunder_core::{Bundle, MevError, Pubkey, Result, Scheduler, Transaction, WorkerAssignment};
use parking_lot::Mutex;
use std::collections::HashSet;
use std::sync::Arc;

pub struct GreedyScheduler {
    worker_states: Arc<Mutex<Vec<WorkerState>>>,
}

struct WorkerState {
    locked_accounts: HashSet<Pubkey>,
}

impl GreedyScheduler {
    pub fn new(worker_count: usize) -> Self {
        let worker_states = (0..worker_count)
            .map(|_| WorkerState {
                locked_accounts: HashSet::new(),
            })
            .collect();

        Self {
            worker_states: Arc::new(Mutex::new(worker_states)),
        }
    }

    fn find_available_worker(
        &self,
        accounts: &HashSet<Pubkey>,
        states: &[WorkerState],
    ) -> Option<usize> {
        for (idx, state) in states.iter().enumerate() {
            if accounts.is_disjoint(&state.locked_accounts) {
                return Some(idx);
            }
        }
        None
    }

    fn schedule_bundle(&self, bundle: &Bundle) -> Result<WorkerAssignment> {
        let mut states = self.worker_states.lock();
        let accounts = bundle.all_accounts();

        let worker_id = self
            .find_available_worker(&accounts, &states)
            .ok_or_else(|| MevError::SchedulerError("No availble worker".to_string()))?;

        states[worker_id].locked_accounts.extend(accounts);

        Ok(WorkerAssignment {
            unit_id: bundle.id.to_string(),
            worker_id,
            is_bundle: true,
        })
    }

    fn schedule_transaction(&self, tx: &Transaction) -> Result<WorkerAssignment> {
        let mut states = self.worker_states.lock();
        let accounts = tx.all_accounts();

        let worker_id = self
            .find_available_worker(&accounts, &states)
            .ok_or_else(|| MevError::SchedulerError("No availble worker".to_string()))?;

        states[worker_id].locked_accounts.extend(accounts);

        Ok(WorkerAssignment {
            unit_id: tx.signature.clone(),
            worker_id,
            is_bundle: false,
        })
    }
}

impl Scheduler for GreedyScheduler {
    fn schedule(
        &self,
        bundles: Vec<Bundle>,
        loose_txs: Vec<Transaction>,
    ) -> Result<Vec<WorkerAssignment>> {
        let mut assignments = Vec::new();

        for bundle in bundles {
            match self.schedule_bundle(&bundle) {
                Ok(assignment) => assignments.push(assignment),
                Err(_) => continue,
            }
        }

        for tx in loose_txs {
            match self.schedule_transaction(&tx) {
                Ok(assignment) => assignments.push(assignment),
                Err(_) => continue,
            }
        }

        Ok(assignments)
    }

    fn name(&self) -> &str {
        "GreedyScheduler"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blunder_core::{AccountMeta, Pubkey};

    #[test]
    fn test_greedy_scheduler() {
        let scheduler = GreedyScheduler::new(4);

        let pubkey = Pubkey::new_unique();
        let accounts = vec![AccountMeta::new(pubkey, true)];
        let tx = Transaction::new("sig1".to_string(), accounts, 100_000, 1000);

        let result = scheduler.schedule(vec![], vec![tx]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn test_conflict_detection() {
        let scheduler = GreedyScheduler::new(2);

        let pubkey = Pubkey::new_unique();
        let accounts = vec![AccountMeta::new(pubkey, true)];

        let tx1 = Transaction::new("sig1".to_string(), accounts.clone(), 100_000, 1000);
        let tx2 = Transaction::new("sig1".to_string(), accounts, 100_000, 1000);

        let result = scheduler.schedule(vec![], vec![tx1, tx2]);
        assert!(result.is_ok());

        let assignments = result.unwrap();
        assert_eq!(assignments.len(), 2);
        assert_ne!(assignments[0].worker_id, assignments[1].worker_id);
    }
}
