use crate::bundle::Bundle;
use crate::errors::Result;
use crate::transaction::Transaction;

pub trait Scheduler: Send + Sync {
    fn schedule(
        &self,
        bundles: Vec<Bundle>,
        loose_txs: Vec<Transaction>,
    ) -> Result<Vec<WorkerAssignment>>;

    fn name(&self) -> &str;
}

pub trait BundleEngine: Send + Sync {
    fn select_winners(&self, submissions: Vec<BundleSubmission>) -> Result<Vec<Bundle>>;
}

#[derive(Clone, Debug)]
pub struct WorkerAssignment {
    pub unit_id: String,
    pub worker_id: usize,
    pub is_bundle: bool,
}

#[derive(Clone, Debug)]
pub struct BundleSubmission {
    pub bundle: Bundle,
    pub score: u64,
}

impl BundleSubmission {
    pub fn new(bundle: Bundle, score: u64) -> Self {
        Self { bundle, score }
    }
}
