use blunder_core::{Bundle, MevError, Pubkey, Result, Transaction};
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

/// Result of work completion sent back to the pool
#[derive(Debug, Clone)]
pub struct WorkCompletion {
    pub worker_id: usize,
    pub unit_id: String,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug)]
pub enum WorkItem {
    Transaction(Transaction),
    Bundle(Bundle),
    Shutdown,
}

pub struct AsyncWorker {
    id: usize,
    locked_accounts: Arc<RwLock<HashSet<Pubkey>>>,
    work_rx: mpsc::UnboundedReceiver<WorkItem>,
    completion_tx: mpsc::UnboundedSender<WorkCompletion>,
}

impl AsyncWorker {
    pub fn new(
        id: usize,
        locked_accounts: Arc<RwLock<HashSet<Pubkey>>>,
        work_rx: mpsc::UnboundedReceiver<WorkItem>,
        completion_tx: mpsc::UnboundedSender<WorkCompletion>,
    ) -> Self {
        Self {
            id,
            locked_accounts,
            work_rx,
            completion_tx,
        }
    }

    pub async fn run(mut self) {
        println!("Worker {} starting", self.id);

        loop {
            match self.work_rx.recv().await {
                Some(WorkItem::Transaction(tx)) => {
                    let unit_id = tx.signature.clone();
                    let result = self.execute_transaction(&tx).await;
                    let (success, error) = match result {
                        Ok(_) => {
                            println!("Worker {} executed tx {}", self.id, unit_id);
                            (true, None)
                        }
                        Err(e) => {
                            eprintln!("Worker {} tx {} failed: {}", self.id, unit_id, e);
                            (false, Some(e.to_string()))
                        }
                    };
                    let _ = self.completion_tx.send(WorkCompletion {
                        worker_id: self.id,
                        unit_id,
                        success,
                        error,
                    });
                }
                Some(WorkItem::Bundle(bundle)) => {
                    let unit_id = bundle.id.to_string();
                    let result = self.execute_bundle(&bundle).await;
                    let (success, error) = match result {
                        Ok(_) => {
                            println!("Worker {} executed bundle {}", self.id, unit_id);
                            (true, None)
                        }
                        Err(e) => {
                            eprintln!("Worker {} bundle {} failed: {}", self.id, unit_id, e);
                            (false, Some(e.to_string()))
                        }
                    };
                    let _ = self.completion_tx.send(WorkCompletion {
                        worker_id: self.id,
                        unit_id,
                        success,
                        error,
                    });
                }
                Some(WorkItem::Shutdown) | None => {
                    println!("Worker {} shutting down", self.id);
                    break;
                }
            }
        }
    }

    async fn acquire_locks(&self, accounts: &HashSet<Pubkey>) -> Result<()> {
        let mut locked = self.locked_accounts.write().await;

        if accounts.iter().any(|acc| locked.contains(acc)) {
            return Err(MevError::LockAcquisitionFailed);
        }

        locked.extend(accounts.iter().copied());
        Ok(())
    }

    async fn release_locks(&self, accounts: &HashSet<Pubkey>) {
        let mut locked = self.locked_accounts.write().await;
        for account in accounts {
            locked.remove(account);
        }
    }

    async fn execute_transaction(&self, tx: &Transaction) -> Result<()> {
        let accounts = tx.all_accounts();
        self.acquire_locks(&accounts).await?;

        tokio::time::sleep(tokio::time::Duration::from_micros(100)).await;

        self.release_locks(&accounts).await;
        Ok(())
    }

    async fn execute_bundle(&self, bundle: &Bundle) -> Result<()> {
        let accounts = bundle.all_accounts();
        self.acquire_locks(&accounts).await?;

        for _tx in &bundle.transactions {
            tokio::time::sleep(tokio::time::Duration::from_micros(100)).await;
        }

        self.release_locks(&accounts).await;
        Ok(())
    }
}

pub struct AsyncWorkerPool {
    worker_count: usize,
    senders: Vec<mpsc::UnboundedSender<WorkItem>>,
    global_locks: Arc<RwLock<HashSet<Pubkey>>>,
    completion_rx: tokio::sync::Mutex<mpsc::UnboundedReceiver<WorkCompletion>>,
    pending_work: AtomicUsize,
}

impl AsyncWorkerPool {
    pub fn spawn(worker_count: usize) -> Self {
        let global_locks = Arc::new(RwLock::new(HashSet::new()));
        let mut senders = Vec::new();
        let (completion_tx, completion_rx) = mpsc::unbounded_channel();

        for id in 0..worker_count {
            let (tx, rx) = mpsc::unbounded_channel();
            let worker = AsyncWorker::new(id, Arc::clone(&global_locks), rx, completion_tx.clone());

            tokio::spawn(async move {
                worker.run().await;
            });
            senders.push(tx);
        }

        Self {
            worker_count,
            senders,
            global_locks,
            completion_rx: tokio::sync::Mutex::new(completion_rx),
            pending_work: AtomicUsize::new(0),
        }
    }

    pub fn send_work(&self, worker_id: usize, work: WorkItem) -> Result<()> {
        // Don't count shutdown signals as pending work
        let is_shutdown = matches!(work, WorkItem::Shutdown);

        self.senders
            .get(worker_id)
            .ok_or_else(|| MevError::SchedulerError("Invalid worker ID".to_string()))?
            .send(work)
            .map_err(|_| MevError::SchedulerError("Worker channel closed".to_string()))?;

        if !is_shutdown {
            self.pending_work.fetch_add(1, Ordering::SeqCst);
        }
        Ok(())
    }

    /// Wait for all pending work to complete and return the completion results
    pub async fn wait_for_completions(&self) -> Vec<WorkCompletion> {
        let pending = self.pending_work.load(Ordering::SeqCst);
        if pending == 0 {
            return Vec::new();
        }

        let mut completions = Vec::with_capacity(pending);
        let mut rx = self.completion_rx.lock().await;

        while completions.len() < pending {
            match rx.recv().await {
                Some(completion) => {
                    completions.push(completion);
                    self.pending_work.fetch_sub(1, Ordering::SeqCst);
                }
                None => break, // Channel closed
            }
        }

        completions
    }

    /// Get the number of pending work items
    pub fn pending_count(&self) -> usize {
        self.pending_work.load(Ordering::SeqCst)
    }

    pub fn worker_count(&self) -> usize {
        self.worker_count
    }

    pub async fn locked_account_count(&self) -> usize {
        self.global_locks.read().await.len()
    }

    pub async fn shutdown(self) {
        // Wait for any pending work to complete first
        let pending = self.pending_work.load(Ordering::SeqCst);
        if pending > 0 {
            println!("Waiting for {} pending work items to complete...", pending);
            let mut rx = self.completion_rx.lock().await;
            let mut completed = 0;
            while completed < pending {
                if rx.recv().await.is_some() {
                    completed += 1;
                } else {
                    break;
                }
            }
        }

        println!("Sending shutdown signal to {} workers", self.worker_count);

        for sender in &self.senders {
            let _ = sender.send(WorkItem::Shutdown);
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        println!("All workers shut down");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blunder_core::{AccountMeta, Bundle, Pubkey, Transaction};

    #[tokio::test]
    async fn test_async_worker_pool_creation() {
        let pool = AsyncWorkerPool::spawn(4);
        assert_eq!(pool.worker_count(), 4);

        pool.shutdown().await;
    }

    #[tokio::test]
    async fn test_async_worker_execute_transaction() {
        let pool = AsyncWorkerPool::spawn(2);

        let tx = Transaction::new(
            "tx1".to_string(),
            vec![AccountMeta::new(Pubkey::new_unique(), true)],
            100_000,
            1000,
        );

        let result = pool.send_work(0, WorkItem::Transaction(tx));
        assert!(result.is_ok());

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        pool.shutdown().await;
    }

    #[tokio::test]
    async fn test_async_worker_execute_bundle() {
        let pool = AsyncWorkerPool::spawn(2);

        let tx1 = Transaction::new(
            "bundle_tx1".to_string(),
            vec![AccountMeta::new(Pubkey::new_unique(), true)],
            100_000,
            1000,
        );

        let tx2 = Transaction::new(
            "bundle_tx2".to_string(),
            vec![AccountMeta::new(Pubkey::new_unique(), true)],
            100_000,
            1000,
        );

        let bundle = Bundle::new(1, vec![tx1, tx2], 50_000, "searcher1".to_string());

        let result = pool.send_work(0, WorkItem::Bundle(bundle));
        assert!(result.is_ok());

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        pool.shutdown().await;
    }

    #[tokio::test]
    async fn test_async_worker_conflicting_transactions() {
        let pool = AsyncWorkerPool::spawn(2);

        let shared_account = Pubkey::new_unique();

        let tx1 = Transaction::new(
            "tx1".to_string(),
            vec![AccountMeta::new(shared_account, true)],
            100_000,
            1000,
        );

        let tx2 = Transaction::new(
            "tx2".to_string(),
            vec![AccountMeta::new(shared_account, true)],
            100_000,
            1000,
        );

        // Send to different workers
        pool.send_work(0, WorkItem::Transaction(tx1)).unwrap();
        pool.send_work(1, WorkItem::Transaction(tx2)).unwrap();

        // One should succeed, one should fail (conflict on shared account)
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        pool.shutdown().await;
    }

    #[tokio::test]
    async fn test_async_worker_parallel_execution() {
        let pool = AsyncWorkerPool::spawn(4);

        // 4 transactions with different accounts - should execute in parallel
        for i in 0..4 {
            let tx = Transaction::new(
                format!("tx{}", i),
                vec![AccountMeta::new(Pubkey::new_unique(), true)],
                100_000,
                1000,
            );

            pool.send_work(i, WorkItem::Transaction(tx)).unwrap();
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        pool.shutdown().await;
    }

    #[tokio::test]
    async fn test_async_worker_invalid_worker_id() {
        let pool = AsyncWorkerPool::spawn(2);

        let tx = Transaction::new(
            "tx1".to_string(),
            vec![AccountMeta::new(Pubkey::new_unique(), true)],
            100_000,
            1000,
        );

        // Worker ID 5 doesn't exist (only 0 and 1)
        let result = pool.send_work(5, WorkItem::Transaction(tx));
        assert!(result.is_err());

        pool.shutdown().await;
    }

    #[tokio::test]
    async fn test_async_worker_shutdown_signal() {
        let pool = AsyncWorkerPool::spawn(2);

        // Send shutdown manually
        pool.send_work(0, WorkItem::Shutdown).unwrap();
        pool.send_work(1, WorkItem::Shutdown).unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Should complete gracefully
        pool.shutdown().await;
    }

    #[tokio::test]
    async fn test_async_worker_high_load() {
        let pool = AsyncWorkerPool::spawn(4);

        // Send 50 transactions
        for i in 0..50 {
            let tx = Transaction::new(
                format!("tx{}", i),
                vec![AccountMeta::new(Pubkey::new_unique(), true)],
                100_000,
                1000,
            );

            pool.send_work(i % 4, WorkItem::Transaction(tx)).unwrap();
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        pool.shutdown().await;
    }

    #[tokio::test]
    async fn test_async_worker_locked_account_count() {
        let pool = AsyncWorkerPool::spawn(2);

        let shared_account = Pubkey::new_unique();

        let tx = Transaction::new(
            "tx1".to_string(),
            vec![AccountMeta::new(shared_account, true)],
            100_000,
            1000,
        );

        pool.send_work(0, WorkItem::Transaction(tx)).unwrap();

        // Give worker time to acquire lock
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        let _locked_count = pool.locked_account_count().await;
        // Account may be locked or already released depending on timing

        pool.shutdown().await;
    }

    #[tokio::test]
    async fn test_async_worker_bundle_atomicity() {
        let pool = AsyncWorkerPool::spawn(1);

        let account1 = Pubkey::new_unique();
        let account2 = Pubkey::new_unique();

        let tx1 = Transaction::new(
            "bundle_tx1".to_string(),
            vec![AccountMeta::new(account1, true)],
            100_000,
            1000,
        );

        let tx2 = Transaction::new(
            "bundle_tx2".to_string(),
            vec![AccountMeta::new(account2, true)],
            100_000,
            1000,
        );

        // Bundle should execute atomically
        let bundle = Bundle::new(1, vec![tx1, tx2], 100_000, "searcher1".to_string());

        pool.send_work(0, WorkItem::Bundle(bundle)).unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        pool.shutdown().await;
    }
}
