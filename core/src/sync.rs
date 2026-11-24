//! Locking that survives a panic on another thread.
//!
//! `Mutex::lock` fails only when another thread panicked while holding the lock ("poisoning"). Everything
//! this app keeps behind a mutex (the player, caches, lists of devices, flags, the licence state) is
//! updated with plain assignments and inserts, so a panic part-way cannot leave it half-written in a way
//! that matters; carrying on with the data is better than taking the whole app down with a second panic.
//! The panic itself is still printed by the panicking thread.
//!
//! The audio callback never uses this: it calls `try_lock` and plays silence when the lock is busy
//! (see `engine::fill`).

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Locks `mutex`, recovering the data if another thread panicked while holding it.
pub fn lock<T: ?Sized>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn locks_an_unpoisoned_mutex() {
        let mutex = Mutex::new(1);
        *lock(&mutex) += 1;
        assert_eq!(*lock(&mutex), 2);
    }

    #[test]
    fn recovers_the_data_after_a_panic_while_locked() {
        let mutex = Arc::new(Mutex::new(vec![1, 2]));
        let poisoner = mutex.clone();
        let result = std::thread::spawn(move || {
            let mut guard = poisoner.lock().unwrap();
            guard.push(3);
            panic!("deliberate panic while holding the lock");
        })
        .join();
        assert!(result.is_err());
        assert!(mutex.is_poisoned());
        assert_eq!(*lock(&mutex), vec![1, 2, 3]);
    }
}
