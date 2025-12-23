//! RAII lock guards for account locking with read/write separation.
//!
//! These guards automatically release locks on drop, preventing lock leaks
//! on panic or early return. They also support Solana's read/write semantics
//! where multiple readers can access the same account concurrently.

use crate::{MevError, Pubkey, Result};
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Tracks read and write locks separately for maximum parallelism.
/// - Write locks are exclusive (only one writer)
/// - Read locks are shared (multiple readers allowed)
/// - Write-read conflicts (writer blocks readers and vice versa)
pub struct AccountLocks {
    /// Accounts with exclusive write locks
    write_locked: RwLock<HashSet<Pubkey>>,
    /// Accounts with shared read locks (count per account for multiple readers)
    read_locked: RwLock<HashMap<Pubkey, usize>>,
}

impl AccountLocks {
    pub fn new() -> Self {
        Self {
            write_locked: RwLock::new(HashSet::new()),
            read_locked: RwLock::new(HashMap::new()),
        }
    }

    /// Check if acquiring these locks would conflict with existing locks.
    /// Does NOT acquire the locks.
    pub fn would_conflict(
        &self,
        write_accounts: &HashSet<Pubkey>,
        read_accounts: &HashSet<Pubkey>,
    ) -> bool {
        let write_locked = self.write_locked.read();
        let read_locked = self.read_locked.read();

        // Check write conflicts: new writes conflict with existing writes or reads
        for acc in write_accounts {
            if write_locked.contains(acc) || read_locked.contains_key(acc) {
                return true;
            }
        }

        // Check read conflicts: new reads conflict with existing writes
        for acc in read_accounts {
            if write_locked.contains(acc) {
                return true;
            }
        }

        false
    }

    /// Get count of currently write-locked accounts
    pub fn write_locked_count(&self) -> usize {
        self.write_locked.read().len()
    }

    /// Get count of currently read-locked accounts
    pub fn read_locked_count(&self) -> usize {
        self.read_locked.read().len()
    }
}

impl Default for AccountLocks {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard for account locks with read/write separation.
/// Automatically releases all held locks when dropped.
pub struct AccountLockGuard<'a> {
    locks: &'a AccountLocks,
    write_accounts: HashSet<Pubkey>,
    read_accounts: HashSet<Pubkey>,
}

impl<'a> AccountLockGuard<'a> {
    /// Acquire locks: write accounts get exclusive, read accounts get shared.
    /// Returns Err if any lock conflicts with existing locks.
    pub fn acquire(
        locks: &'a AccountLocks,
        write_accounts: HashSet<Pubkey>,
        read_accounts: HashSet<Pubkey>,
    ) -> Result<Self> {
        let mut write_locked = locks.write_locked.write();
        let mut read_locked = locks.read_locked.write();

        // Check write conflicts: no one else can have write OR read lock on accounts we want to write
        for acc in &write_accounts {
            if write_locked.contains(acc) || read_locked.contains_key(acc) {
                return Err(MevError::LockAcquisitionFailed);
            }
        }

        // Check read conflicts: no one else can have write lock on accounts we want to read
        for acc in &read_accounts {
            if write_locked.contains(acc) {
                return Err(MevError::LockAcquisitionFailed);
            }
        }

        // Acquire write locks (exclusive)
        write_locked.extend(write_accounts.iter().copied());

        // Acquire read locks (increment refcount for shared access)
        for acc in &read_accounts {
            *read_locked.entry(*acc).or_insert(0) += 1;
        }

        Ok(Self {
            locks,
            write_accounts,
            read_accounts,
        })
    }

    /// Acquire locks for ALL accounts as write locks (legacy behavior).
    /// Use this when you don't have read/write separation.
    pub fn acquire_all_as_write(
        locks: &'a AccountLocks,
        accounts: HashSet<Pubkey>,
    ) -> Result<Self> {
        Self::acquire(locks, accounts, HashSet::new())
    }

    /// Get the write accounts held by this guard
    pub fn write_accounts(&self) -> &HashSet<Pubkey> {
        &self.write_accounts
    }

    /// Get the read accounts held by this guard
    pub fn read_accounts(&self) -> &HashSet<Pubkey> {
        &self.read_accounts
    }
}

impl Drop for AccountLockGuard<'_> {
    fn drop(&mut self) {
        let mut write_locked = self.locks.write_locked.write();
        let mut read_locked = self.locks.read_locked.write();

        // Release write locks
        for acc in &self.write_accounts {
            write_locked.remove(acc);
        }

        // Release read locks (decrement refcount)
        for acc in &self.read_accounts {
            if let Some(count) = read_locked.get_mut(acc) {
                *count -= 1;
                if *count == 0 {
                    read_locked.remove(acc);
                }
            }
        }
    }
}

/// Async version of AccountLocks using tokio's RwLock
pub struct AsyncAccountLocks {
    write_locked: tokio::sync::RwLock<HashSet<Pubkey>>,
    read_locked: tokio::sync::RwLock<HashMap<Pubkey, usize>>,
}

impl AsyncAccountLocks {
    pub fn new() -> Self {
        Self {
            write_locked: tokio::sync::RwLock::new(HashSet::new()),
            read_locked: tokio::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Check if acquiring these locks would conflict (async version)
    pub async fn would_conflict(
        &self,
        write_accounts: &HashSet<Pubkey>,
        read_accounts: &HashSet<Pubkey>,
    ) -> bool {
        let write_locked = self.write_locked.read().await;
        let read_locked = self.read_locked.read().await;

        for acc in write_accounts {
            if write_locked.contains(acc) || read_locked.contains_key(acc) {
                return true;
            }
        }

        for acc in read_accounts {
            if write_locked.contains(acc) {
                return true;
            }
        }

        false
    }
}

impl Default for AsyncAccountLocks {
    fn default() -> Self {
        Self::new()
    }
}

/// Async RAII guard for account locks.
/// Supports explicit async release or fallback sync release on drop.
pub struct AsyncAccountLockGuard {
    locks: Arc<AsyncAccountLocks>,
    write_accounts: HashSet<Pubkey>,
    read_accounts: HashSet<Pubkey>,
    released: bool,
}

impl AsyncAccountLockGuard {
    /// Acquire locks asynchronously
    pub async fn acquire(
        locks: Arc<AsyncAccountLocks>,
        write_accounts: HashSet<Pubkey>,
        read_accounts: HashSet<Pubkey>,
    ) -> Result<Self> {
        {
            let mut write_locked = locks.write_locked.write().await;
            let mut read_locked = locks.read_locked.write().await;

            // Check write conflicts
            for acc in &write_accounts {
                if write_locked.contains(acc) || read_locked.contains_key(acc) {
                    return Err(MevError::LockAcquisitionFailed);
                }
            }

            // Check read conflicts
            for acc in &read_accounts {
                if write_locked.contains(acc) {
                    return Err(MevError::LockAcquisitionFailed);
                }
            }

            // Acquire write locks
            write_locked.extend(write_accounts.iter().copied());

            // Acquire read locks
            for acc in &read_accounts {
                *read_locked.entry(*acc).or_insert(0) += 1;
            }
        } // Guards dropped here before we move locks

        Ok(Self {
            locks,
            write_accounts,
            read_accounts,
            released: false,
        })
    }

    /// Acquire all accounts as write locks (legacy behavior)
    pub async fn acquire_all_as_write(
        locks: Arc<AsyncAccountLocks>,
        accounts: HashSet<Pubkey>,
    ) -> Result<Self> {
        Self::acquire(locks, accounts, HashSet::new()).await
    }

    /// Explicit async release - preferred method for async contexts.
    /// This is more efficient than relying on Drop in async code.
    pub async fn release(mut self) {
        let mut write_locked = self.locks.write_locked.write().await;
        let mut read_locked = self.locks.read_locked.write().await;

        for acc in &self.write_accounts {
            write_locked.remove(acc);
        }

        for acc in &self.read_accounts {
            if let Some(count) = read_locked.get_mut(acc) {
                *count -= 1;
                if *count == 0 {
                    read_locked.remove(acc);
                }
            }
        }

        self.released = true;
    }

    /// Get the write accounts held by this guard
    pub fn write_accounts(&self) -> &HashSet<Pubkey> {
        &self.write_accounts
    }

    /// Get the read accounts held by this guard
    pub fn read_accounts(&self) -> &HashSet<Pubkey> {
        &self.read_accounts
    }
}

impl Drop for AsyncAccountLockGuard {
    fn drop(&mut self) {
        if self.released {
            return;
        }

        // Fallback: try to release synchronously using try_write
        // This handles the case where guard is dropped without explicit release
        if let Ok(mut write_locked) = self.locks.write_locked.try_write() {
            for acc in &self.write_accounts {
                write_locked.remove(acc);
            }
        } else {
            eprintln!(
                "WARNING: AsyncAccountLockGuard dropped without explicit release, \
                 write locks may not be released immediately"
            );
        }

        if let Ok(mut read_locked) = self.locks.read_locked.try_write() {
            for acc in &self.read_accounts {
                if let Some(count) = read_locked.get_mut(acc) {
                    *count -= 1;
                    if *count == 0 {
                        read_locked.remove(acc);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_write_conflict() {
        let locks = AccountLocks::new();
        let acc1 = Pubkey::new_unique();

        let accounts: HashSet<_> = [acc1].into_iter().collect();

        // First guard acquires write lock
        let _guard1 = AccountLockGuard::acquire_all_as_write(&locks, accounts.clone()).unwrap();

        // Second guard should fail (write-write conflict)
        let result = AccountLockGuard::acquire_all_as_write(&locks, accounts);
        assert!(result.is_err());
    }

    #[test]
    fn test_read_read_no_conflict() {
        let locks = AccountLocks::new();
        let acc1 = Pubkey::new_unique();

        let read_accounts: HashSet<_> = [acc1].into_iter().collect();
        let empty_write: HashSet<Pubkey> = HashSet::new();

        // First guard acquires read lock
        let _guard1 =
            AccountLockGuard::acquire(&locks, empty_write.clone(), read_accounts.clone()).unwrap();

        // Second guard should succeed (read-read no conflict)
        let _guard2 = AccountLockGuard::acquire(&locks, empty_write, read_accounts).unwrap();
    }

    #[test]
    fn test_write_read_conflict() {
        let locks = AccountLocks::new();
        let acc1 = Pubkey::new_unique();

        let write_accounts: HashSet<_> = [acc1].into_iter().collect();
        let read_accounts: HashSet<_> = [acc1].into_iter().collect();
        let empty: HashSet<Pubkey> = HashSet::new();

        // First guard acquires write lock
        let _guard1 = AccountLockGuard::acquire(&locks, write_accounts, empty.clone()).unwrap();

        // Second guard trying to read should fail
        let result = AccountLockGuard::acquire(&locks, empty, read_accounts);
        assert!(result.is_err());
    }

    #[test]
    fn test_read_write_conflict() {
        let locks = AccountLocks::new();
        let acc1 = Pubkey::new_unique();

        let write_accounts: HashSet<_> = [acc1].into_iter().collect();
        let read_accounts: HashSet<_> = [acc1].into_iter().collect();
        let empty: HashSet<Pubkey> = HashSet::new();

        // First guard acquires read lock
        let _guard1 = AccountLockGuard::acquire(&locks, empty.clone(), read_accounts).unwrap();

        // Second guard trying to write should fail
        let result = AccountLockGuard::acquire(&locks, write_accounts, empty);
        assert!(result.is_err());
    }

    #[test]
    fn test_lock_release_on_drop() {
        let locks = AccountLocks::new();
        let acc1 = Pubkey::new_unique();

        let accounts: HashSet<_> = [acc1].into_iter().collect();

        {
            let _guard = AccountLockGuard::acquire_all_as_write(&locks, accounts.clone()).unwrap();
            assert_eq!(locks.write_locked_count(), 1);
        }
        // Guard dropped, lock should be released
        assert_eq!(locks.write_locked_count(), 0);

        // Should be able to acquire again
        let _guard2 = AccountLockGuard::acquire_all_as_write(&locks, accounts).unwrap();
    }

    #[test]
    fn test_disjoint_accounts_no_conflict() {
        let locks = AccountLocks::new();
        let acc1 = Pubkey::new_unique();
        let acc2 = Pubkey::new_unique();

        let accounts1: HashSet<_> = [acc1].into_iter().collect();
        let accounts2: HashSet<_> = [acc2].into_iter().collect();

        // Both should succeed since they touch different accounts
        let _guard1 = AccountLockGuard::acquire_all_as_write(&locks, accounts1).unwrap();
        let _guard2 = AccountLockGuard::acquire_all_as_write(&locks, accounts2).unwrap();
    }

    #[tokio::test]
    async fn test_async_lock_guard() {
        let locks = Arc::new(AsyncAccountLocks::new());
        let acc1 = Pubkey::new_unique();

        let accounts: HashSet<_> = [acc1].into_iter().collect();

        // Acquire and release
        let guard = AsyncAccountLockGuard::acquire_all_as_write(locks.clone(), accounts.clone())
            .await
            .unwrap();

        // Should conflict
        let result =
            AsyncAccountLockGuard::acquire_all_as_write(locks.clone(), accounts.clone()).await;
        assert!(result.is_err());

        // Explicit release
        guard.release().await;

        // Should succeed now
        let _guard2 = AsyncAccountLockGuard::acquire_all_as_write(locks, accounts)
            .await
            .unwrap();
    }
}
