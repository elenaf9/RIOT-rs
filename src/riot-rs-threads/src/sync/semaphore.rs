//! This module provides a Semaphore implementation.
use core::cell::UnsafeCell;

use crate::{threadlist::ThreadList, ThreadState};

/// A basic binary semaphore.
///
/// A `Semaphore` behaves like a Mutex, but carries no data.
/// This is supposed to be used to implement other locking primitives.
pub struct Semaphore<const N: usize> {
    state: UnsafeCell<LockState>,
}

unsafe impl<const N: usize> Sync for Semaphore<N> {}

enum LockState {
    Unlocked(usize),
    Locked(ThreadList),
}

impl<const N: usize> Semaphore<N> {
    /// Creates new **unlocked** Semaphore
    pub const fn new() -> Self {
        Self {
            state: UnsafeCell::new(LockState::Unlocked(N)),
        }
    }

    /// Creates new **locked** Semaphore
    pub const fn new_locked() -> Self {
        Self {
            state: UnsafeCell::new(LockState::Locked(ThreadList::new())),
        }
    }

    /// Returns the current lock state
    ///
    /// true if locked, false otherwise
    pub fn is_locked(&self) -> bool {
        crate::cs_with(|_| {
            let state = unsafe { &*self.state.get() };
            !matches!(state, LockState::Unlocked(_))
        })
    }

    /// Get this semaphore (blocking)
    ///
    /// If the semaphore was unlocked, it will be locked and the function returns.
    /// If the semaphore was locked, this function will unschedule the current thread until the
    /// semaphore gets unlocked elsewhere.
    ///
    /// **NOTE**: must not be called outside thread context!
    pub fn acquire(&self) {
        crate::cs_with(|cs| {
            let state = unsafe { &mut *self.state.get() };
            match state {
                LockState::Unlocked(1) => *state = LockState::Locked(ThreadList::new()),
                LockState::Unlocked(counter) => *counter -= 1,
                LockState::Locked(waiters) => {
                    waiters.put_current(cs, ThreadState::LockBlocked);
                }
            }
        })
    }

    /// Get the semaphore (non-blocking)
    ///
    /// If the semaphore was unlocked, it will be locked and the function returns true.
    /// If the semaphore was locked, the function returns false
    pub fn try_acquire(&self) -> bool {
        crate::cs_with(|_| {
            let state = unsafe { &mut *self.state.get() };
            match state {
                LockState::Unlocked(1) => {
                    *state = LockState::Locked(ThreadList::new());
                    true
                }
                LockState::Unlocked(counter) => {
                    *counter -= 1;
                    true
                }
                LockState::Locked(_) => false,
            }
        })
    }

    /// Releases the semaphore.
    ///
    /// If the semaphore was locked, and there were waiters, the first waiter will be
    /// woken up.
    /// If the semaphore was locked and there were no waiters, the lock will be unlocked.
    /// If the semaphore was not locked, the function just returns.
    pub fn release(&self) {
        crate::cs_with(|cs| {
            let state = unsafe { &mut *self.state.get() };
            match state {
                LockState::Unlocked(counter) => *counter += 1,
                LockState::Locked(waiters) => {
                    if waiters.pop(cs).is_none() {
                        *state = LockState::Unlocked(1)
                    }
                }
            }
        })
    }
}

impl<const N: usize> Default for Semaphore<N> {
    fn default() -> Self {
        Self::new()
    }
}
