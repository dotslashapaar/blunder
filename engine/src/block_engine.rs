use blunder_core::{Bundle, BundleEngine, BundleSubmission, Pubkey, Result};
use dashmap::DashMap;
use std::collections::HashSet;
use std::sync::Arc;

pub struct BlockEngine {
    submissions: DashMap<u64, BundleSubmission>,
    next_id: Arc<parking_lot::Mutex<u64>>,
}

impl BlockEngine {
    pub fn new() -> Self {
        Self {
            submissions: DashMap::new(),
            next_id: Arc::new(parking_lot::Mutex::new(0)),
        }
    }

    pub fn submit_bundle(&self, mut bundle: Bundle, score: u64) -> u64 {
        let mut id = self.next_id.lock();
        let bundle_id = *id;
        *id += 1;

        bundle.id = bundle_id;

        self.submissions
            .insert(bundle_id, BundleSubmission { bundle, score });

        bundle_id
    }

    pub fn clear_submission(&self) {
        self.submissions.clear();
    }

    fn has_conflicts(bundle: &Bundle, locked_accounts: &HashSet<Pubkey>) -> bool {
        !bundle.all_accounts().is_disjoint(locked_accounts)
    }
}

impl BundleEngine for BlockEngine {
    fn select_winners(&self, mut submissions: Vec<BundleSubmission>) -> Result<Vec<Bundle>> {
        submissions.sort_by_key(|s| std::cmp::Reverse(s.score));

        let mut winners = Vec::new();
        let mut locked_accounts = HashSet::new();

        for submission in submissions {
            if !Self::has_conflicts(&submission.bundle, &locked_accounts) {
                locked_accounts.extend(submission.bundle.all_accounts());
                winners.push(submission.bundle);
            }
        }

        Ok(winners)
    }
}

impl Default for BlockEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blunder_core::{AccountMeta, Pubkey, Transaction};

    #[test]
    fn test_block_engine_submission() {
        let engine = BlockEngine::new();

        let bundle = Bundle::new(0, vec![], 50_000, "searcher1".to_string());
        let id = engine.submit_bundle(bundle, 50_000);

        assert_eq!(id, 0);
        assert!(engine.submissions.contains_key(&id));
    }

    #[test]
    fn test_winner_selection() {
        let engine = BlockEngine::new();

        let pubkey1 = Pubkey::new_unique();
        let pubkey2 = Pubkey::new_unique();

        let accounts1 = vec![AccountMeta::new(pubkey1, true)];
        let tx1 = Transaction::new("sig1".to_string(), accounts1, 100_000, 1000);
        let bundle1 = Bundle::new(1, vec![tx1], 100_000, "searcher1".to_string());

        let accounts2 = vec![AccountMeta::new(pubkey2, true)];
        let tx2 = Transaction::new("sig2".to_string(), accounts2, 100_000, 1000);
        let bundle2 = Bundle::new(2, vec![tx2], 50_000, "searcher2".to_string());

        let submissions = vec![
            BundleSubmission::new(bundle1, 100_000),
            BundleSubmission::new(bundle2, 50_000),
        ];

        let winners = engine.select_winners(submissions).unwrap();
        assert_eq!(winners.len(), 2);
        assert_eq!(winners[0].id, 1);
    }

    #[test]
    fn test_conflict_detection() {
        let engine = BlockEngine::new();

        let pubkey = Pubkey::new_unique();
        let accounts = vec![AccountMeta::new(pubkey, true)];

        let tx1 = Transaction::new("sig1".to_string(), accounts.clone(), 100_000, 1000);
        let bundle1 = Bundle::new(1, vec![tx1], 100_000, "searcher1".to_string());

        let tx2 = Transaction::new("sig2".to_string(), accounts, 100_000, 1000);
        let bundle2 = Bundle::new(2, vec![tx2], 50_000, "searcher2".to_string());

        let submissions = vec![
            BundleSubmission::new(bundle1, 100_000),
            BundleSubmission::new(bundle2, 50_000),
        ];

        let winners = engine.select_winners(submissions).unwrap();
        assert_eq!(winners.len(), 1);
        assert_eq!(winners[0].id, 1);
    }
}
