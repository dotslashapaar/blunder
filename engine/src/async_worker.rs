use blunder_core::{Bundle, MevError, Pubkey, Result, Transaction};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

#[derive(Debug)]
pub enum WorkItem {
    Transaction(Transaction),
    Bundle(Bundle),
    Shutdown,
}

pub struct AsyncWorker{
    id: usize,
    locked_accounts: Arc<RwLock<HashSet<Pubkey>>>,
    work_rx: mpsc::UnboundedReceiver<WorkItem>,
}

impl AsyncWorker {
    pub fn new(id: usize, locked_accounts: Arc<RwLock<HashSet<Pubkey>>>, work_rx: mpsc::UnboundedReceiver<WorkItem>) -> Self {
        Self {
            id,
            locked_accounts,
            work_rx,
        }
    }

    pub async fn run(mut self){
        println!("Worker {} starting", self.id);

        loop {
            match self.work_rx.recv().await {
                Some(WorkItem::Transaction(tx)) => {
                    if let Err(e) = self.execute_transaction(&tx).await {
                        eprintln!("Worker {} tx {} failed: {}", self.id, tx.signature, e);
                    } else {
                        println!("Worker {} executed tx {}", self.id, tx.signature);
                    }
                }
                Some(WorkItem::Bundle(bundle)) => {
                    if let Err(e) = self.execute_bundle(&bundle).await {
                        eprintln!("Worker {} bundle {} failed: {}", self.id, bundle.id, e);
                    } else {
                        println!("Worker {} executed bundle {}", self.id, bundle.id);
                    }
                }
                Some(WorkItem::Shutdown) | None => {
                    println!("Worker {} shutting down", self.id);
                    break;
                }
            }
        }
    }

    async fn accquire_locks(&self, accounts: &HashSet<Pubkey>) -> Result<()> {
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
        self.accquire_locks(&accounts).await?;

        tokio::time::sleep(tokio::time::Duration::from_micros(100)).await;

        self.release_locks(&accounts).await;
        Ok(())
    }

    async fn execute_bundle(&self, bundle: &Bundle)->Result<()>{
        let accounts = bundle.all_accounts();
        self.accquire_locks(&accounts).await?;

        for _tx in &bundle.transactions{
            tokio::time::sleep(tokio::time::Duration::from_micros(100)).await;
        }

        self.release_locks(&accounts).await;
        Ok(())
    }
}

pub struct AsyncWorkerPool{
    worker_count: usize,
    senders: Vec<mpsc::UnboundedSender<WorkItem>>,
    global_locks: Arc<RwLock<HashSet<Pubkey>>>,
}

impl AsyncWorkerPool {
    pub fn spawn(worker_count: usize)->Self{
        let global_locks = Arc::new(RwLock::new(HashSet::new()));
        let mut senders = Vec::new();

        for id in 0..worker_count{
            let (tx, rx) = mpsc::unbounded_channel();
            let worker = AsyncWorker::new(id, Arc::clone(&global_locks), rx);

            tokio::spawn(async move {
                worker.run().await;
            });
            senders.push(tx);
        }

        Self { worker_count, senders, global_locks }
    }

    pub fn send_work(&self, worker_id: usize, work: WorkItem)->Result<()>{
        self.senders.get(worker_id).ok_or_else(|| MevError::SchedulerError("Invalid worker ID".to_string()))?.send(work).map_err(|_| MevError::SchedulerError("Worker channel closed".to_string()))
    }

    pub fn worker_count(&self)->usize{
        self.worker_count
    }

    pub async fn locked_account_count(&self)->usize{
        self.global_locks.read().await.len()
    }

    pub async fn shutdown(self){
        println!("Sending shutdown signal to {} workers", self.worker_count);
        
        for sender in &self.senders {
            let _ = sender.send(WorkItem::Shutdown);
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        println!("All workers shut down");
    }
}