use crate::{AsyncWorkerPool, SchedulerEngine, WorkItem};
use blunder_core::{
    AccountMeta, Bundle, ExecutionUnitData, Pubkey, Result, Transaction, WorkerAssignment,
};
use std::collections::HashMap;
use std::sync::Arc;

pub struct IntegratedPipeline {
    scheduler_engine: SchedulerEngine,
    worker_pool: Arc<AsyncWorkerPool>,
}

impl IntegratedPipeline {
    pub async fn new(
        worker_count: usize,
        external_scheduler_addr: Option<std::net::SocketAddr>,
    ) -> Result<Self> {
        let scheduler_engine = SchedulerEngine::new(worker_count, external_scheduler_addr).await?;
        let worker_pool = Arc::new(AsyncWorkerPool::spawn(worker_count));

        Ok(Self {
            scheduler_engine,
            worker_pool,
        })
    }

    pub async fn process_batch(
        &self,
        transactions: Vec<Transaction>,
        bundles: Vec<Bundle>,
    ) -> Result<BatchResult> {
        let mut items: Vec<ExecutionUnitData> = Vec::new();

        // Convert Transactions to ExecutionUnitData
        for tx in &transactions {
            let accounts: Vec<[u8; 32]> =
                tx.accounts.iter().map(|am| am.pubkey.to_bytes()).collect();
            items.push(ExecutionUnitData::Transaction {
                id: tx.signature.clone(),
                priority: tx.compute_unit_price,
                accounts,
            });
        }

        // Convert Bundles to ExecutionUnitData
        for bundle in &bundles {
            let accounts: Vec<[u8; 32]> =
                bundle.all_accounts().iter().map(|p| p.to_bytes()).collect();
            items.push(ExecutionUnitData::Bundle {
                id: bundle.id.to_string(),
                tip: bundle.tip,
                accounts,
                num_transactions: bundle.transactions.len(),
            });
        }

        let assignments = self.scheduler_engine.schedule(items.clone()).await?;

        let dispatch_results = self.dispatch_to_workers(items, assignments)?;

        // Wait for all workers to complete their assigned work
        let completions = self.worker_pool.wait_for_completions().await;

        // Merge dispatch results with actual completion results
        let results: Vec<ExecutionResult> = dispatch_results
            .into_iter()
            .map(|mut r| {
                // Update result with actual completion status if available
                if let Some(completion) = completions.iter().find(|c| c.unit_id == r.unit_id) {
                    r.success = completion.success;
                    r.error = completion.error.clone();
                }
                r
            })
            .collect();

        Ok(BatchResult {
            total_processed: results.len(),
            successful: results.iter().filter(|r| r.success).count(),
            failed: results.iter().filter(|r| !r.success).count(),
            results,
        })
    }

    fn dispatch_to_workers(
        &self,
        items: Vec<ExecutionUnitData>,
        assignments: Vec<WorkerAssignment>,
    ) -> Result<Vec<ExecutionResult>> {
        let mut unit_map: HashMap<String, ExecutionUnitData> = items
            .into_iter()
            .map(|item| (item.id().to_string(), item))
            .collect();

        let mut results = Vec::new();

        for assignment in assignments {
            if let Some(unit) = unit_map.remove(&assignment.unit_id) {
                let work_item = convert_to_work_item(&unit)?;

                let res = self.worker_pool.send_work(assignment.worker_id, work_item);

                results.push(ExecutionResult {
                    unit_id: assignment.unit_id,
                    worker_id: assignment.worker_id,
                    success: res.is_ok(),
                    error: res.err().map(|e| e.to_string()),
                });
            } else {
                results.push(ExecutionResult {
                    unit_id: assignment.unit_id,
                    worker_id: assignment.worker_id,
                    success: false,
                    error: Some("Work unit not found".to_string()),
                });
            }
        }

        Ok(results)
    }
}

fn convert_to_work_item(unit: &ExecutionUnitData) -> Result<WorkItem> {
    match unit {
        ExecutionUnitData::Transaction {
            id: _,
            priority: _,
            accounts: _,
        } => Ok(WorkItem::Transaction(
            convert_execution_unit_to_transaction(unit)?,
        )),
        ExecutionUnitData::Bundle {
            id: _,
            tip: _,
            accounts: _,
            num_transactions: _,
        } => Ok(WorkItem::Bundle(convert_execution_unit_to_bundle(unit)?)),
    }
}

fn convert_execution_unit_to_transaction(unit: &ExecutionUnitData) -> Result<Transaction> {
    match unit {
        ExecutionUnitData::Transaction {
            id,
            priority: _,
            accounts,
        } => {
            let account_metas: Vec<AccountMeta> = accounts
                .iter()
                .map(|bytes| {
                    let pubkey = Pubkey::new_from_array(*bytes);
                    AccountMeta::new(pubkey, true)
                })
                .collect();

            Ok(Transaction::new(
                id.clone(),
                account_metas,
                100_000, // default compute_unit_limit
                1000,    // default compute_unit_price
            ))
        }
        _ => Err(blunder_core::MevError::SchedulerError(
            "Expected Transaction variant".to_string(),
        )),
    }
}

fn convert_execution_unit_to_bundle(unit: &ExecutionUnitData) -> Result<Bundle> {
    match unit {
        ExecutionUnitData::Bundle {
            id,
            tip,
            accounts: _,
            num_transactions: _,
        } => {
            let bundle_id = id.parse::<u64>().map_err(|_| {
                blunder_core::MevError::InvalidTransaction(format!("Invalid bundle ID: {}", id))
            })?;
            Ok(Bundle::new(
                bundle_id,
                vec![],
                *tip,
                "default_searcher".to_string(),
            ))
        }
        _ => Err(blunder_core::MevError::SchedulerError(
            "Expected Bundle variant".to_string(),
        )),
    }
}

#[derive(Debug)]
pub struct BatchResult {
    pub total_processed: usize,
    pub successful: usize,
    pub failed: usize,
    pub results: Vec<ExecutionResult>,
}

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub unit_id: String,
    pub worker_id: usize,
    pub success: bool,
    pub error: Option<String>,
}
