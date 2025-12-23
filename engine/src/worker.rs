use blunder_core::{Bundle, MevError, Pubkey, Result, Transaction};
use parking_lot::RwLock;
use std::collections::HashSet;
use std::sync::Arc;

pub struct Worker {
    id: usize,
    locked_accounts: Arc<RwLock<HashSet<Pubkey>>>,
}

impl Worker {
    pub fn new_with_shared_locks(id: usize, locked_accounts: Arc<RwLock<HashSet<Pubkey>>>) -> Self {
        Self {
            id,
            locked_accounts,
        }
    }

    pub fn id(&self) -> usize {
        self.id
    }

    fn acquire_locks(&self, accounts: &HashSet<Pubkey>) -> Result<()> {
        let mut locked = self.locked_accounts.write();

        if accounts.iter().any(|acc| locked.contains(acc)) {
            return Err(MevError::LockAcquisitionFailed);
        }

        locked.extend(accounts.iter().copied());
        Ok(())
    }

    fn release_locks(&self, accounts: &HashSet<Pubkey>) {
        let mut locked = self.locked_accounts.write();
        for account in accounts {
            locked.remove(account);
        }
    }

    fn execute_transaction_inner(&self, _tx: &Transaction) -> Result<()> {
        Ok(())
    }

    pub fn execute_transaction(&self, tx: &Transaction) -> Result<()> {
        let accounts = tx.all_accounts();
        self.acquire_locks(&accounts)?;

        let result = self.execute_transaction_inner(tx);

        self.release_locks(&accounts);
        result
    }

    pub fn execute_bundle(&self, bundle: &Bundle) -> Result<()> {
        let accounts = bundle.all_accounts();
        self.acquire_locks(&accounts)?;

        for tx in &bundle.transactions {
            if let Err(e) = self.execute_transaction_inner(tx) {
                self.release_locks(&accounts);
                return Err(MevError::BundleExecutionFailed(e.to_string()));
            }
        }
        self.release_locks(&accounts);
        Ok(())
    }
}

pub struct WorkerPool {
    workers: Vec<Worker>,
    global_locks: Arc<RwLock<HashSet<Pubkey>>>,
}

impl WorkerPool {
    pub fn new(worker_count: usize) -> Self {
        let global_locks = Arc::new(RwLock::new(HashSet::new()));

        let workers = (0..worker_count)
            .map(|id| Worker::new_with_shared_locks(id, Arc::clone(&global_locks)))
            .collect();

        Self {
            workers,
            global_locks,
        }
    }

    pub fn get_worker(&self, id: usize) -> Option<&Worker> {
        self.workers.get(id)
    }

    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }

    pub fn locked_account_count(&self) -> usize {
        self.global_locks.read().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blunder_core::{AccountMeta, Pubkey};

    #[test]
    fn test_worker_execution() {
        let pool = WorkerPool::new(1);
        let worker = pool.get_worker(0).unwrap();

        let pubkey = Pubkey::new_unique();
        let accounts = vec![AccountMeta::new(pubkey, true)];
        let tx = Transaction::new("sig1".to_string(), accounts, 10_000, 1000);

        let result = worker.execute_transaction(&tx);
        assert!(result.is_ok());
    }

    #[test]
    fn test_bundle_execution() {
        let pool = WorkerPool::new(1);
        let worker = pool.get_worker(0).unwrap();

        let pubkey1 = Pubkey::new_unique();
        let pubkey2 = Pubkey::new_unique();

        let accounts1 = vec![AccountMeta::new(pubkey1, true)];
        let accounts2 = vec![AccountMeta::new(pubkey2, true)];

        let tx1 = Transaction::new("sig1".to_string(), accounts1, 10_000, 1000);
        let tx2 = Transaction::new("sig2".to_string(), accounts2, 20_000, 2000);

        let bundle = Bundle::new(1, vec![tx1, tx2], 5_000, "searcher1".to_string());

        let result = worker.execute_bundle(&bundle);
        assert!(result.is_ok());
    }

    #[test]
    fn test_worker_pool() {
        let pool = WorkerPool::new(4);
        assert_eq!(pool.worker_count(), 4);

        for id in 0..4 {
            let worker = pool.get_worker(id);
            assert!(worker.is_some());
            assert_eq!(worker.unwrap().id(), id);
        }
    }

    #[test]
    fn test_global_account_locking() {
        let pool = WorkerPool::new(2);
        let worker1 = pool.get_worker(0).unwrap();
        let worker2 = pool.get_worker(1).unwrap();

        let pubkey = Pubkey::new_unique();
        let accounts = vec![AccountMeta::new(pubkey, true)];

        let tx1 = Transaction::new("sig1".to_string(), accounts.clone(), 100_000, 1000);
        let tx2 = Transaction::new("sig2".to_string(), accounts, 100_000, 1000);

        let accounts1 = tx1.all_accounts();
        assert!(worker1.acquire_locks(&accounts1).is_ok());

        let accounts2 = tx2.all_accounts();
        assert!(worker2.acquire_locks(&accounts2).is_err());

        worker1.release_locks(&accounts1);

        assert!(worker2.acquire_locks(&accounts2).is_ok());
    }
}
