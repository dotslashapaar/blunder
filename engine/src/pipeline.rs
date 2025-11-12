use crate::async_worker::{AsyncWorkerPool, WorkItem};
use crate::{BlockEngine, Prioritizer};
use blunder_core::{Bundle, Result, Scheduler, Transaction};
use blunder_scheduler::AdaptiveScheduler;
use std::collections::HashMap;

pub struct TpuPipeline {
    scheduler: AdaptiveScheduler,
    prioritizer: Prioritizer,
    #[allow(dead_code)]
    block_engine: BlockEngine,
    worker_pool: AsyncWorkerPool,
}

impl TpuPipeline {
    pub fn new(worker_count: usize) -> Self {
        Self {
            scheduler: AdaptiveScheduler::new(worker_count),
            prioritizer: Prioritizer::new(),
            block_engine: BlockEngine::new(),
            worker_pool: AsyncWorkerPool::spawn(worker_count),
        }
    }

    pub async fn process_batch(
        &self,
        bundles: Vec<Bundle>,
        loose_txs: Vec<Transaction>,
    ) -> Result<()> {
        let units = self
            .prioritizer
            .priortize(bundles.clone(), loose_txs.clone());
        println!("Prioritized {} units", units.len());

        let mut bundle_map: HashMap<u64, Bundle> = bundles.into_iter().map(|b| (b.id, b)).collect();

        let mut tx_map: HashMap<String, Transaction> = loose_txs
            .into_iter()
            .map(|tx| (tx.signature.clone(), tx))
            .collect();

        let bundles_for_schedule: Vec<Bundle> = bundle_map.values().cloned().collect();
        let txs_for_schedule: Vec<Transaction> = tx_map.values().cloned().collect();

        let assignments = self
            .scheduler
            .schedule(bundles_for_schedule, txs_for_schedule)?;
        println!("Scheduled {} units", assignments.len());

        for assignment in assignments {
            let work = if assignment.is_bundle {
                let bundle_id: u64 = assignment.unit_id.parse().unwrap_or(0);
                if let Some(bundle) = bundle_map.remove(&bundle_id) {
                    WorkItem::Bundle(bundle)
                } else {
                    continue;
                }
            } else if let Some(tx) = tx_map.remove(&assignment.unit_id) {
                WorkItem::Transaction(tx)
            } else {
                continue;
            };
            self.worker_pool.send_work(assignment.worker_id, work)?;
        }

        Ok(())
    }

    pub async fn shutdown(self) {
        println!("Pipeline shutting down...");
        self.worker_pool.shutdown().await;
    }
}
