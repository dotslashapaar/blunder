use blunder_core::{Bundle, SchedulableUnit, Transaction};

pub struct Prioritizer {
    fee_weight: f64,
    cu_weight: f64,
}

impl Prioritizer {
    pub fn new() -> Self {
        Self {
            fee_weight: 0.7,
            cu_weight: 0.3,
        }
    }

    pub fn with_weights(fee_weight: f64, cu_weight: f64) -> Self {
        Self {
            fee_weight,
            cu_weight,
        }
    }

    fn calculate_score(&self, unit: &SchedulableUnit) -> u64 {
        let fee_component = (unit.priority_score() as f64 * self.fee_weight) as u64;
        let cu_component = (unit.compute_units() as f64 * self.cu_weight) as u64;

        fee_component.saturating_add(cu_component)
    }

    pub fn priortize(
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
    fn test_prioritizer() {
        let prioritizer = Prioritizer::new();

        let pubkey = Pubkey::new_unique();
        let accounts = vec![AccountMeta::new(pubkey, true)];

        let high_fee_tx = Transaction::new("sig1".to_string(), accounts.clone(), 100_000, 10_000);

        let low_fee_tx = Transaction::new("sig2".to_string(), accounts.clone(), 100_000, 1_000);

        let units = prioritizer.priortize(vec![], vec![high_fee_tx, low_fee_tx]);

        assert_eq!(units.len(), 2);

        let first_score = prioritizer.calculate_score(&units[0]);
        let second_score = prioritizer.calculate_score(&units[1]);
        assert!(first_score >= second_score);
    }

    #[test]
    fn test_bundle_priority() {
        let prioritizer = Prioritizer::new();

        let pubkey = Pubkey::new_unique();
        let accounts = vec![AccountMeta::new(pubkey, true)];
        let tx = Transaction::new("sig1".to_string(), accounts.clone(), 100_000, 1000);

        let high_tip_bundle = Bundle::new(1, vec![tx.clone()], 100_000, "searcher1".to_string());
        let low_tip_bundle = Bundle::new(2, vec![tx.clone()], 10_000, "searcher2".to_string());

        let units = prioritizer.priortize(vec![high_tip_bundle, low_tip_bundle], vec![]);

        assert_eq!(units.len(), 2);
        assert!(matches!(units[0], SchedulableUnit::Bundle(_)));

        if let SchedulableUnit::Bundle(b) = &units[0] {
            assert_eq!(b.tip, 100_000);
        }
    }
}
