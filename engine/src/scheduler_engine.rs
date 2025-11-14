use blunder_core::{
    AccountMeta, Bundle, ExecutionUnitData, ExternalSchedulerClient, Pubkey, Result, Scheduler,
    Transaction, WorkerAssignment,
};
use blunder_scheduler::AdaptiveScheduler;
use std::net::SocketAddr;

pub struct SchedulerEngine {
    scheduler_client: Option<ExternalSchedulerClient>,
    adaptive_scheduler: AdaptiveScheduler,
    worker_count: usize,
}

impl SchedulerEngine {
    pub async fn new(
        worker_count: usize,
        external_scheduler_addr: Option<SocketAddr>,
    ) -> Result<Self> {
        let scheduler_client = match external_scheduler_addr {
            Some(addr) => Some(ExternalSchedulerClient::connect(addr).await?),
            None => None,
        };

        let adaptive_scheduler = AdaptiveScheduler::new(worker_count);

        Ok(Self {
            scheduler_client,
            adaptive_scheduler,
            worker_count,
        })
    }

    pub async fn schedule(&self, items: Vec<ExecutionUnitData>) -> Result<Vec<WorkerAssignment>> {
        if let Some(client) = &self.scheduler_client {
            client.schedule(items, self.worker_count).await
        } else {
            let (bundles, transactions) = self.convert_execution_units(items)?;
            self.adaptive_scheduler.schedule(bundles, transactions)
        }
    }

    fn convert_execution_units(
        &self,
        items: Vec<ExecutionUnitData>,
    ) -> Result<(Vec<Bundle>, Vec<Transaction>)> {
        let mut bundles = Vec::new();
        let mut transactions = Vec::new();

        for item in items {
            match item {
                ExecutionUnitData::Transaction {
                    id,
                    priority: _,
                    accounts,
                } => {
                    let account_metas = accounts
                        .iter()
                        .map(|bytes| AccountMeta::new(Pubkey::new_from_array(*bytes), true))
                        .collect();

                    transactions.push(Transaction::new(id, account_metas, 100_000, 1000));
                }
                ExecutionUnitData::Bundle { id, tip, .. } => {
                    bundles.push(Bundle::new(
                        id.parse::<u64>().unwrap_or(0),
                        vec![],
                        tip,
                        "default_searcher".to_string(),
                    ));
                }
            }
        }

        Ok((bundles, transactions))
    }

    pub async fn report_completion(
        &self,
        worker_id: usize,
        item_id: String,
        success: bool,
        compute_units: u64,
    ) -> Result<()> {
        if let Some(client) = &self.scheduler_client {
            client
                .report_completion(worker_id, item_id, success, compute_units)
                .await
        } else {
            Ok(())
        }
    }
}
