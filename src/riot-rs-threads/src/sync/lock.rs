//! This module provides a Lock implementation.
use core::cell::UnsafeCell;

use crate::{threadlist::ThreadList, ThreadId, ThreadState};

/// A basic locking object.
///
/// A `Lock` behaves like a Mutex, but carries no data.
/// This is supposed to be used to implement other locking primitives.
pub struct Lock {
    state: UnsafeCell<LockState>,
}

unsafe impl Sync for Lock {}

enum LockState {
    Unlocked,
    Locked {
        waiters: ThreadList,
        owner: ThreadId,
    },
}

impl Lock {
    /// Creates new **unlocked** Lock
    pub const fn new() -> Self {
        Self {
            state: UnsafeCell::new(LockState::Unlocked),
        }
    }

    // /// Creates new **locked** Lock
    // pub const fn new_locked() -> Self {
    //     Self {
    //         state: UnsafeCell::new(LockState::Locked(ThreadList::new())),
    //     }
    // }

    /// Returns the current lock state
    ///
    /// true if locked, false otherwise
    pub fn is_locked(&self) -> bool {
        crate::cs_with(|_| {
            let state = unsafe { &*self.state.get() };
            !matches!(state, LockState::Unlocked)
        })
    }

    /// Get this lock (blocking)
    ///
    /// If the lock was unlocked, it will be locked and the function returns.
    /// If the lock was locked, this function will block the current thread until the lock gets
    /// unlocked elsewhere.
    ///
    /// **NOTE**: must not be called outside thread context!
    pub fn acquire(&self) {
        crate::cs_with(|cs| {
            let state = unsafe { &mut *self.state.get() };
            match state {
                LockState::Unlocked => {
                    let pid = crate::current_pid().unwrap();
                    *state = LockState::Locked {
                        waiters: ThreadList::new(),
                        owner: pid,
                    }
                }
                LockState::Locked { waiters, .. } => {
                    waiters.put_current(cs, ThreadState::LockBlocked);
                }
            }
        })
    }

    /// Get the lock (non-blocking)
    ///
    /// If the lock was unlocked, it will be locked and the function returns true.
    /// If the lock was locked by another thread, the function returns false.
    /// If the lock is already locked by the current thread, the function returns true.
    pub fn try_acquire(&self) -> bool {
        crate::cs_with(|_| {
            let pid = crate::current_pid().unwrap();
            let state = unsafe { &mut *self.state.get() };
            match state {
                LockState::Unlocked => {
                    *state = LockState::Locked {
                        waiters: ThreadList::new(),
                        owner: pid,
                    };
                    true
                }
                LockState::Locked { owner, .. } => *owner == pid,
            }
        })
    }

    /// Releases the lock.
    ///
    /// If the lock was locked, and there were waiters, the first waiter will be
    /// woken up.
    /// If the lock was locked and there were no waiters, the lock will be unlocked.
    /// If the lock was not locked, the function just returns.
    pub fn release(&self) {
        crate::cs_with(|cs| {
            let state = unsafe { &mut *self.state.get() };
            match state {
                LockState::Unlocked => {}
                LockState::Locked { waiters, owner } => {
                    if let Some((pid, _)) = waiters.pop(cs) {
                        *owner = pid
                    } else {
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
