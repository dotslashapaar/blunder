pub mod bundle;
pub mod errors;
pub mod metadata;
pub mod traits;
pub mod transaction;

pub use bundle::*;
pub use errors::*;
pub use metadata::*;
pub use traits::*;
pub use transaction::*;

pub use solana_program::instruction::AccountMeta;
pub use solana_program::pubkey::Pubkey;

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_transaction_creation() {
        let pubkey = Pubkey::new_unique();
        let accounts = vec![AccountMeta::new(pubkey, true)];
        let tx = Transaction::new("sig123".to_string(), accounts, 100_000, 1000);
        assert_eq!(tx.compute_unit_limit, 100_000);
        assert_eq!(tx.compute_unit_price, 1000);
    }

    #[test]
    fn test_bundle_creation() {
        let bundle = Bundle::new(1, vec![], 50_000, "searcher1".to_string());
        assert_eq!(bundle.id, 1);
        assert_eq!(bundle.tip, 50_000);
        assert!(bundle.atomic);
    }

    #[test]
    fn test_schedulable_unit_bundle() {
        let bundle = Bundle::new(1, vec![], 50_000, "searcher1".to_string());
        let unit = SchedulableUnit::Bundle(bundle);
        assert!(unit.is_atomic());
        assert_eq!(unit.bundle_id(), Some(1));
    }

    #[test]
    fn test_priority_score() {
        let pubkey = Pubkey::new_unique();
        let accounts = vec![AccountMeta::new(pubkey, true)];
        let tx = Transaction::new("sig123".to_string(), accounts, 100_000, 1000);
        assert_eq!(tx.priority_score(), 100);
    }

    #[test]
    fn test_bundle_accounts() {
        let pubkey1 = Pubkey::new_unique();
        let pubkey2 = Pubkey::new_unique();

        let accounts1 = vec![AccountMeta::new(pubkey1, true)];
        let tx1 = Transaction::new("sig1".to_string(), accounts1, 100_000, 1000);

        let accounts2 = vec![AccountMeta::new(pubkey2, false)];
        let tx2 = Transaction::new("sig2".to_string(), accounts2, 50_000, 500);

        let bundle = Bundle::new(1, vec![tx1, tx2], 10_000, "searcher1".to_string());

        assert_eq!(bundle.all_accounts().len(), 2);
        assert_eq!(bundle.total_compute_units(), 150_000);
    }
}
