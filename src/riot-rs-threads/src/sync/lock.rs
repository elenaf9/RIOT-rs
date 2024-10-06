//! This module provides a Lock implementation.
use crate::{sync::ILock, threadlist::ThreadList, ThreadState, THREADS};

/// A basic locking object.
///
/// A `Lock` behaves like a Mutex, but carries no data.
/// This is supposed to be used to implement other locking primitives.
pub struct Lock {
    state: ILock<LockState>,
}

unsafe impl Sync for Lock {}

enum LockState {
    Unlocked,
    Locked(ThreadList),
}

impl Lock {
    /// Creates new **unlocked** Lock.
    pub const fn new() -> Self {
        Self {
            state: ILock::new(LockState::Unlocked),
        }
    }

    /// Creates new **locked** Lock.
    pub const fn new_locked() -> Self {
        Self {
            state: ILock::new(LockState::Locked(ThreadList::new())),
        }
    }

    /// Returns the current lock state.
    ///
    /// true if locked, false otherwise
    pub fn is_locked(&self) -> bool {
        let state = self.state.lock();
        !matches!(*state, LockState::Unlocked)
    }

    /// Get this lock (blocking).
    ///
    /// If the lock was unlocked, it will be locked and the function returns.
    /// If the lock was locked, this function will block the current thread until the lock gets
    /// unlocked elsewhere.
    ///
    /// # Panics
    ///
    /// Panics if this is called outside of a thread context.
    pub fn acquire(&self) {
        THREADS.with(|mut threads| {
            let mut state = self.state.lock_mut();
            match *state {
                LockState::Unlocked => *state = LockState::Locked(ThreadList::new()),
                LockState::Locked(ref mut waiters) => {
                    waiters.put_current(&mut threads, ThreadState::LockBlocked);
                }
            }
        })
    }

    /// Get the lock (non-blocking).
    ///
    /// If the lock was unlocked, it will be locked and the function returns true.
    /// If the lock was locked, the function returns false
    pub fn try_acquire(&self) -> bool {
        let mut state = self.state.lock_mut();
        match *state {
            LockState::Unlocked => {
                *state = LockState::Locked(ThreadList::new());
                true
            }
            LockState::Locked(_) => false,
        }
    }

    /// Releases the lock.
    ///
    /// If the lock was locked, and there were waiters, the first waiter will be
    /// woken up.
    /// If the lock was locked and there were no waiters, the lock will be unlocked.
    /// If the lock was not locked, the function just returns.
    pub fn release(&self) {
        THREADS.with(|mut threads| {
            let mut state = self.state.lock_mut();
            match *state {
                LockState::Unlocked => {}
                LockState::Locked(ref mut waiters) => {
                    if waiters.pop(&mut threads).is_none() {
                        *state = LockState::Unlocked
                    }
                }
            }
        })
    }
}

impl Default for Lock {
    fn default() -> Self {
        Self::new()
    }
}
