//! This module provides a Lock implementation.
use core::cell::UnsafeCell;

use crate::{threadlist::ThreadList, RunqueueId, ThreadId, ThreadState, THREADS};

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
        // The current owner of the lock and their normal priority.
        owner: (ThreadId, RunqueueId),
        waiters: ThreadList,
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
    /// If the lock was locked by another thread, this function will block the current thread
    /// until the lock gets unlocked elsewhere.
    /// If the lock was already locked by the current thread, nothing happens.
    ///
    /// **NOTE**: must not be called outside thread context!
    pub fn acquire(&self) {
        THREADS.with_mut(|threads| {
            let thread = threads.current().unwrap();
            let (pid, prio) = (thread.pid, thread.prio);
            drop(thread);
            let state = unsafe { &mut *self.state.get() };
            match state {
                LockState::Unlocked => {
                    *state = LockState::Locked {
                        waiters: ThreadList::new(),
                        owner: (pid, prio),
                    }
                }
                LockState::Locked {
                    waiters,
                    owner: (owner_id, owner_prio),
                } => {
                    if *owner_id == pid {
                        return;
                    }
                    if let Some(inherit_priority) =
                        waiters.put_current(&threads, ThreadState::LockBlocked)
                    {
                        if &inherit_priority > owner_prio {
                            threads.set_priority(*owner_id, inherit_priority);
                        }
                    }
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
        THREADS.with_mut(|threads| {
            let thread = threads.current().unwrap();
            let (pid, prio) = (thread.pid, thread.prio);
            drop(thread);
            let state = unsafe { &mut *self.state.get() };
            match state {
                LockState::Unlocked => {
                    *state = LockState::Locked {
                        waiters: ThreadList::new(),
                        owner: (pid, prio),
                    };
                    true
                }
                LockState::Locked {
                    owner: (owner_pid, _),
                    ..
                } => *owner_pid == pid,
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
        THREADS.with_mut(|threads| {
            let state = unsafe { &mut *self.state.get() };
            match state {
                LockState::Unlocked => {}
                LockState::Locked {
                    waiters,
                    owner: (owner_pid, owner_prio),
                } => {
                    if threads.current_pid().unwrap() != *owner_pid {
                        return;
                    }
                    threads.set_priority(*owner_pid, *owner_prio);
                    if let Some((pid, _)) = waiters.pop(&threads) {
                        *owner_pid = pid;
                        *owner_prio = threads.get_priority(pid).unwrap();
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
