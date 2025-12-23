use blunder_core::{Bundle, SchedulableUnit, Transaction};

/// Prioritizer ranks schedulable units by fee-per-compute-unit efficiency.
/// This aligns with validator incentives: maximize total fees within the block's CU limit.
pub struct Prioritizer;

impl Prioritizer {
    pub fn new() -> Self {
        Self
    }

    /// Calculate priority score as fee-per-CU (reward efficiency).
    /// Higher score = more fee per compute unit = should be scheduled first.
    /// Formula: (priority_score * 1_000_000) / compute_units
    fn calculate_score(&self, unit: &SchedulableUnit) -> u64 {
        let cu = unit.compute_units();
        if cu == 0 {
            return 0;
        }
        unit.priority_score()
            .saturating_mul(1_000_000)
            .saturating_div(cu)
    }

    pub fn prioritize(
        &self,
        bundles: Vec<Bundle>,
        loose_txs: Vec<Transaction>,
    ) -> Vec<SchedulableUnit> {
        let mut units: Vec<SchedulableUnit> = bundles
            .into_iter()
            .map(SchedulableUnit::Bundle)
            .chain(loose_txs.into_iter().map(SchedulableUnit::Transaction))
            .collect();

        units.sort_by_key(|unit| std::cmp::Reverse(self.calculate_score(unit)));

        units
    }
}

impl Default for Prioritizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blunder_core::{AccountMeta, Pubkey};

    #[test]
    fn test_prioritizer_higher_fee_wins() {
        let prioritizer = Prioritizer::new();

        let pubkey = Pubkey::new_unique();
        let accounts = vec![AccountMeta::new(pubkey, true)];

        // Same CU, different fees - higher fee should win
        let high_fee_tx = Transaction::new("sig1".to_string(), accounts.clone(), 100_000, 10_000);
        let low_fee_tx = Transaction::new("sig2".to_string(), accounts.clone(), 100_000, 1_000);

        let units = prioritizer.prioritize(vec![], vec![high_fee_tx, low_fee_tx]);

        assert_eq!(units.len(), 2);

        let first_score = prioritizer.calculate_score(&units[0]);
        let second_score = prioritizer.calculate_score(&units[1]);
        assert!(first_score >= second_score);
    }

    #[test]
    fn test_prioritizer_efficient_tx_beats_wasteful() {
        // This is the KEY test: an efficient tx (low CU, high fee) should beat
        // a wasteful tx (high CU, same total fee) because fee-per-CU is higher.
        let prioritizer = Prioritizer::new();

        let pubkey = Pubkey::new_unique();
        let accounts = vec![AccountMeta::new(pubkey, true)];

        // Efficient tx: 10K CU, 1000 price -> total fee = 10K * 1000 / 1M = 10
        // fee-per-CU = 10 * 1M / 10K = 1,000,000
        let efficient_tx =
            Transaction::new("efficient".to_string(), accounts.clone(), 10_000, 1_000);

        // Wasteful tx: 1M CU, 100 price -> total fee = 1M * 100 / 1M = 100 (10x more total fee!)
        // fee-per-CU = 100 * 1M / 1M = 100
        let wasteful_tx =
            Transaction::new("wasteful".to_string(), accounts.clone(), 1_000_000, 100);

        let units = prioritizer.prioritize(vec![], vec![wasteful_tx, efficient_tx]);

        // Efficient tx should be first despite paying less total fee
        if let SchedulableUnit::Transaction(tx) = &units[0] {
            assert_eq!(
                tx.signature, "efficient",
                "Efficient tx should be prioritized first"
            );
        } else {
            panic!("Expected transaction");
        }

        let efficient_score = prioritizer.calculate_score(&units[0]);
        let wasteful_score = prioritizer.calculate_score(&units[1]);
        assert!(
            efficient_score > wasteful_score,
            "Efficient score {} should be > wasteful score {}",
            efficient_score,
            wasteful_score
        );
    }

    #[test]
    fn test_zero_cu_returns_zero_priority() {
        let prioritizer = Prioritizer::new();

        let pubkey = Pubkey::new_unique();
        let accounts = vec![AccountMeta::new(pubkey, true)];

        // Zero CU transaction should get zero priority (avoid division by zero)
        let zero_cu_tx = Transaction::new("zero".to_string(), accounts.clone(), 0, 1_000);
        let unit = SchedulableUnit::Transaction(zero_cu_tx);

        assert_eq!(prioritizer.calculate_score(&unit), 0);
    }

    #[test]
    fn test_bundle_priority() {
        let prioritizer = Prioritizer::new();

        let pubkey = Pubkey::new_unique();
        let accounts = vec![AccountMeta::new(pubkey, true)];
        let tx = Transaction::new("sig1".to_string(), accounts.clone(), 100_000, 1000);

        // Same CU (100K each), different tips - higher tip should win
        let high_tip_bundle = Bundle::new(1, vec![tx.clone()], 100_000, "searcher1".to_string());
        let low_tip_bundle = Bundle::new(2, vec![tx.clone()], 10_000, "searcher2".to_string());

        let units = prioritizer.prioritize(vec![high_tip_bundle, low_tip_bundle], vec![]);

        assert_eq!(units.len(), 2);
        assert!(matches!(units[0], SchedulableUnit::Bundle(_)));

        if let SchedulableUnit::Bundle(b) = &units[0] {
            assert_eq!(b.tip, 100_000);
        }
    }

    #[test]
    fn test_bundle_efficiency() {
        // Test that bundle efficiency is also considered
        let prioritizer = Prioritizer::new();

        let pubkey = Pubkey::new_unique();
        let accounts = vec![AccountMeta::new(pubkey, true)];

        // Efficient bundle: 1 tx with 50K CU, tip = 50K -> fee-per-CU = 50K * 1M / 50K = 1M
        let small_tx = Transaction::new("small".to_string(), accounts.clone(), 50_000, 1000);
        let efficient_bundle = Bundle::new(1, vec![small_tx], 50_000, "efficient".to_string());

        // Wasteful bundle: 1 tx with 500K CU, tip = 100K (2x tip!) -> fee-per-CU = 100K * 1M / 500K = 200K
        let large_tx = Transaction::new("large".to_string(), accounts.clone(), 500_000, 1000);
        let wasteful_bundle = Bundle::new(2, vec![large_tx], 100_000, "wasteful".to_string());

        let units = prioritizer.prioritize(vec![wasteful_bundle, efficient_bundle], vec![]);

        // Efficient bundle should win despite lower total tip
        if let SchedulableUnit::Bundle(b) = &units[0] {
            assert_eq!(
                b.searcher_id, "efficient",
                "Efficient bundle should be first"
            );
        }
    }
}
