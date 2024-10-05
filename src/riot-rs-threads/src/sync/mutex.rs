use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
};

use riot_rs_runqueue::{RunqueueId, ThreadId};

use crate::{thread::ThreadState, threadlist::ThreadList, THREADS};

/// A basic mutex with priority inheritance.
pub struct Mutex<T> {
    state: crate::Lock<LockState>,
    inner: UnsafeCell<T>,
}

unsafe impl<T> Sync for Mutex<T> {}

enum LockState {
    Unlocked,
    Locked {
        // The current owner of the lock and their normal priority.
        owner_id: ThreadId,
        owner_prio: RunqueueId,
        waiters: ThreadList,
    },
}

impl<T> Mutex<T> {
    /// Creates new **unlocked** [`Mutex`].
    pub const fn new(value: T) -> Self {
        Self {
            state: crate::Lock::new(LockState::Unlocked),
            inner: UnsafeCell::new(value),
        }
    }

    /// Returns the current mutex state.
    ///
    /// `true` if locked, `false` otherwise
    pub fn is_locked(&self) -> bool {
        self.state
            .with(|state| !matches!(state, LockState::Unlocked))
    }

    /// Get this mutex (blocking).
    ///
    /// If the mutex was unlocked, it will be locked and a [`MutexGuard`] is returned.
    /// If the mutex is locked, this function will block the current thread until the mutex gets
    /// unlocked elsewhere.
    ///
    /// # Panics
    ///
    /// Panics if called outside of a thread context.
    pub fn lock(&self) -> MutexGuard<T> {
        THREADS.with(|threads| {
            self.state.with_mut(|state| match state {
                LockState::Unlocked => {
                    let (owner_id, owner_prio) = threads
                        .current_with(|t| t.map(|t| (t.pid, t.prio)))
                        .unwrap();
                    *state = LockState::Locked {
                        waiters: ThreadList::new(),
                        owner_id,
                        owner_prio,
                    }
                }
                LockState::Locked {
                    waiters,
                    owner_id,
                    owner_prio,
                } => {
                    if let Some(inherit_priority) =
                        waiters.put_current(&threads, ThreadState::LockBlocked)
                    {
                        if &inherit_priority > owner_prio {
                            threads.set_priority(*owner_id, inherit_priority);
                        }
                    }
                }
            })
        });
        MutexGuard { mutex: self }
    }

    /// Get the mutex (non-blocking).
    ///
    /// If the mutex was unlocked, it will be locked and a [`MutexGuard`] is returned.
    /// If the mutex was locked `None` is returned.
    ///
    /// # Panics
    ///
    /// Panics if called outside of a thread context.
    pub fn try_lock(&self) -> Option<MutexGuard<T>> {
        THREADS.with(|threads| {
            self.state.with_mut(|state| {
                let (owner_id, owner_prio) = threads
                    .current_with(|t| t.map(|t| (t.pid, t.prio)))
                    .unwrap();
                match state {
                    LockState::Unlocked => {
                        *state = LockState::Locked {
                            waiters: ThreadList::new(),
                            owner_id,
                            owner_prio,
                        };
                        Some(MutexGuard { mutex: self })
                    }
                    _ => None,
                }
            })
        })
    }

    /// Releases the mutex.
    ///
    /// If there are waiters, the first waiter will be woken up.
    fn release(&self) {
        THREADS.with(|threads| {
            self.state.with_mut(|state| match state {
                LockState::Unlocked => {}
                LockState::Locked {
                    waiters,
                    owner_id,
                    owner_prio,
                } => {
                    threads.set_priority(*owner_id, *owner_prio);
                    if let Some((pid, _)) = waiters.pop(&threads) {
                        *owner_id = pid;
                        *owner_prio = threads.get_priority(pid);
                    } else {
                        *state = LockState::Unlocked
                    }
                }
            })
        })
    }
}

/// Grants access to a [`Mutex`] inner data.
///
/// Dropping the [`MutexGuard`] will unlock the [`Mutex`];
pub struct MutexGuard<'a, T> {
    mutex: &'a Mutex<T>,
}

impl<'a, T> Deref for MutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.mutex.inner.get() }
    }
}

impl<'a, T> DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.mutex.inner.get() }
    }
}

impl<'a, T> Drop for MutexGuard<'a, T> {
    fn drop(&mut self) {
        // Unlock the mutex when the guard is dropped.
        self.mutex.release()
    }
}
