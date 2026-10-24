//! Locking a `Mutex` that a worker thread or a web view shares with the reader.
//!
//! A panic in one thread poisons every mutex that thread held. What these mutexes guard — a map of
//! chapters, a slice of verses, the download queue — is whole values written under the lock, not a
//! structure a panic can leave half updated, so keeping the numbers and carrying on shows the
//! reader something better than taking the window down with the thread that failed.

use std::sync::{Mutex, MutexGuard, PoisonError};

pub trait LockExt<T> {
    /// The guard, whether or not a thread panicked while holding the lock.
    fn lock_or_recover(&self) -> MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    fn lock_or_recover(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::LockExt;
    use std::sync::{Arc, Mutex};

    #[test]
    fn a_poisoned_lock_still_yields_its_value_and_takes_writes() {
        let shared = Arc::new(Mutex::new(vec![1, 2, 3]));
        let worker = shared.clone();
        let died = std::thread::spawn(move || {
            let mut held = worker.lock().unwrap();
            held.push(4);
            panic!("a worker dies holding the lock");
        })
        .join();
        assert!(died.is_err(), "the worker should have panicked");
        assert!(shared.lock().is_err(), "the lock should now be poisoned");
        // The value the dead thread left is kept, and the lock is usable again.
        assert_eq!(*shared.lock_or_recover(), vec![1, 2, 3, 4]);
        shared.lock_or_recover().push(5);
        assert_eq!(*shared.lock_or_recover(), vec![1, 2, 3, 4, 5]);
    }
}
