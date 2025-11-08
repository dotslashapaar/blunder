use serde::{Deserialize, Serialize};
use solana_program::pubkey::Pubkey;
use std::collections::HashSet;

use crate::transaction::Transaction;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Bundle {
    pub id: u64,
    pub transactions: Vec<Transaction>,
    pub tip: u64,
    pub searcher_id: String,
    pub atomic: bool,
}

impl Bundle {
    pub fn new(id: u64, transactions: Vec<Transaction>, tip: u64, searcher_id: String) -> Self {
        Self {
            id,
            transactions,
            tip,
            searcher_id,
            atomic: true,
        }
    }

    pub fn total_compute_units(&self) -> u64 {
        self.transactions
            .iter()
            .map(|tx| tx.compute_unit_limit)
            .sum()
    }

    pub fn all_accounts(&self) -> HashSet<Pubkey> {
        self.transactions
            .iter()
            .flat_map(|tx| tx.all_accounts())
            .collect()
    }

    pub fn writable_accounts(&self) -> HashSet<Pubkey> {
        self.transactions
            .iter()
            .flat_map(|tx| tx.writable_accounts())
            .collect()
    }

    pub fn priority_score(&self) -> u64 {
        self.tip
    }
}
