use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
};

use riot_rs_runqueue::{RunqueueId, ThreadId};

use crate::{sync::Spinlock, thread::ThreadState, threadlist::ThreadList, THREADS};

/// A basic mutex with priority inheritance.
pub struct Mutex<T> {
    state: Spinlock<LockState>,
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
            state: Spinlock::new(LockState::Unlocked),
            inner: UnsafeCell::new(value),
        }
    }

    /// Returns the current mutex state.
    ///
    /// `true` if locked, `false` otherwise
    pub fn is_locked(&self) -> bool {
        !matches!(*self.state.lock(), LockState::Unlocked)
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
        THREADS.with(|mut threads| {
            let mut state = self.state.lock_mut();
            match *state {
                LockState::Unlocked => {
                    let (owner_id, owner_prio) = threads.current_pid_prio().unwrap();

                    *state = LockState::Locked {
                        waiters: ThreadList::new(),
                        owner_id,
                        owner_prio,
                    }
                }
                LockState::Locked {
                    ref mut waiters,
                    owner_id,
                    owner_prio,
                } => {
                    if let Some(inherit_priority) =
                        waiters.put_current(&mut threads, ThreadState::LockBlocked)
                    {
                        if inherit_priority > owner_prio {
                            threads.set_priority(owner_id, inherit_priority);
                        }
                    }
                }
            }
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
            let mut state = self.state.lock_mut();
            let (owner_id, owner_prio) = threads.current_pid_prio().unwrap();
            match *state {
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
    }

    /// Releases the mutex.
    ///
    /// If there are waiters, the first waiter will be woken up.
    fn release(&self) {
        THREADS.with(|mut threads| {
            let mut state = self.state.lock_mut();
            match *state {
                LockState::Unlocked => {}
                LockState::Locked {
                    ref mut waiters,
                    ref mut owner_id,
                    ref mut owner_prio,
                } => {
                    threads.set_priority(*owner_id, *owner_prio);
                    if let Some((pid, _)) = waiters.pop(&mut threads) {
                        let tcbs = threads.tcbs();
                        *owner_id = pid;
                        *owner_prio = tcbs.get_unchecked(pid).prio;
                    } else {
                        *state = LockState::Unlocked
                    }
                }
            }
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
