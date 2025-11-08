use crate::bundle::Bundle;
use crate::transaction::Transaction;
use serde::{Deserialize, Serialize};
use solana_program::pubkey::Pubkey;
use std::collections::HashSet;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TxWithMetadata {
    pub tx: Transaction,
    pub bundle_id: Option<u64>,
    pub atomic: bool,
    pub priority_score: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SchedulableUnit {
    Bundle(Bundle),
    Transaction(Transaction),
}

impl SchedulableUnit {
    pub fn is_atomic(&self) -> bool {
        matches!(self, SchedulableUnit::Bundle(_))
    }

    pub fn bundle_id(&self) -> Option<u64> {
        match self {
            SchedulableUnit::Bundle(b) => Some(b.id),
            SchedulableUnit::Transaction(_) => None,
        }
    }

    pub fn compute_units(&self) -> u64 {
        match self {
            SchedulableUnit::Bundle(b) => b.total_compute_units(),
            SchedulableUnit::Transaction(tx) => tx.compute_unit_limit,
        }
    }

    pub fn all_accounts(&self) -> HashSet<Pubkey> {
        match self {
            SchedulableUnit::Bundle(b) => b.all_accounts(),
            SchedulableUnit::Transaction(tx) => tx.all_accounts(),
        }
    }

    pub fn writable_accounts(&self) -> HashSet<Pubkey> {
        match self {
            SchedulableUnit::Bundle(b) => b.writable_accounts(),
            SchedulableUnit::Transaction(tx) => tx.writable_accounts(),
        }
    }

    pub fn priority_score(&self) -> u64 {
        match self {
            SchedulableUnit::Bundle(b) => b.priority_score(),
            SchedulableUnit::Transaction(tx) => tx.priority_score(),
        }
    }
}
