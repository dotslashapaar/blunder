use serde::{Deserialize, Serialize};
use solana_program::instruction::AccountMeta;
use solana_program::pubkey::Pubkey;
use std::collections::HashSet;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Instruction {
    pub program_id: Pubkey,
    pub account_indices: Vec<usize>,
    pub data: Vec<u8>,
}

impl Instruction {
    pub fn new(program_id: Pubkey, account_indices: Vec<usize>, data: Vec<u8>) -> Self {
        Self {
            program_id,
            account_indices,
            data,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Transaction {
    pub signature: String,
    pub accounts: Vec<AccountMeta>,
    pub instructions: Vec<Instruction>,
    pub compute_unit_limit: u64,
    pub compute_unit_price: u64,
    pub blockhash: String,
    pub sequence: u64,
}

impl Transaction {
    pub fn new(
        signature: String,
        accounts: Vec<AccountMeta>,
        compute_unit_limit: u64,
        compute_unit_price: u64,
    ) -> Self {
        Self {
            signature,
            accounts,
            instructions: Vec::new(),
            compute_unit_limit,
            compute_unit_price,
            blockhash: String::new(),
            sequence: 0,
        }
    }

    pub fn all_accounts(&self) -> HashSet<Pubkey> {
        self.accounts.iter().map(|a| a.pubkey).collect()
    }

    pub fn writable_accounts(&self) -> HashSet<Pubkey> {
        self.accounts
            .iter()
            .filter(|a| a.is_writable)
            .map(|a| a.pubkey)
            .collect()
    }

    pub fn priority_score(&self) -> u64 {
        self.compute_unit_price
            .saturating_mul(self.compute_unit_limit)
            .saturating_div(1_000_000)
    }
}
