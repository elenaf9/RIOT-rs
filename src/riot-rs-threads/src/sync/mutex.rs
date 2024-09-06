use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
};

use riot_rs_runqueue::{RunqueueId, ThreadId};

use crate::{thread::ThreadState, threadlist::ThreadList, THREADS};

/// A basic mutex with priority inheritance.
pub struct Mutex<T> {
    state: UnsafeCell<LockState>,
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
    /// Creates a new **unlocked** [`Mutex`].
    pub const fn new(value: T) -> Self {
        Self {
            state: UnsafeCell::new(LockState::Unlocked),
            inner: UnsafeCell::new(value),
        }
    }

    /// Returns whether the mutex is locked.
    pub fn is_locked(&self) -> bool {
        critical_section::with(|_| {
            let state = unsafe { &*self.state.get() };
            !matches!(state, LockState::Unlocked)
        })
    }

    /// Acquires a mutex, blocking the current thread until it is able to do so.
    ///
    /// If the mutex was unlocked, it will be locked and a [`MutexGuard`] is returned.
    /// If the mutex is locked, this function will block the current thread until the mutex gets
    /// unlocked elsewhere.
    ///
    /// # Panics
    ///
    /// Panics if called outside of a thread context.
    pub fn lock(&self) -> MutexGuard<T> {
        THREADS.with_mut(|mut threads| {
            let state = unsafe { &mut *self.state.get() };
            match state {
                LockState::Unlocked => {
                    let thread = threads.current().unwrap();
                    *state = LockState::Locked {
                        waiters: ThreadList::new(),
                        owner_id: thread.pid,
                        owner_prio: thread.prio,
                    }
                }
                LockState::Locked {
                    waiters,
                    owner_id,
                    owner_prio,
                } => {
                    if let Some(inherit_priority) =
                        waiters.put_current(&mut threads, ThreadState::LockBlocked)
                    {
                        if &inherit_priority > owner_prio {
                            threads.set_priority(*owner_id, inherit_priority);
                        }
                    }
                }
            }
        });
        MutexGuard { mutex: self }
    }

    /// Attempts to acquire this lock, in a non-blocking fashion.
    ///
    /// If the mutex was unlocked, it will be locked and a [`MutexGuard`] is returned.
    /// If the mutex was locked `None` is returned.
    ///
    /// # Panics
    ///
    /// Panics if called outside of a thread context.
    pub fn try_lock(&self) -> Option<MutexGuard<T>> {
        THREADS.with_mut(|mut threads| {
            let thread = threads.current().unwrap();
            let state = unsafe { &mut *self.state.get() };
            match state {
                LockState::Unlocked => {
                    *state = LockState::Locked {
                        waiters: ThreadList::new(),
                        owner_id: thread.pid,
                        owner_prio: thread.prio,
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
        THREADS.with_mut(|mut threads| {
            let state = unsafe { &mut *self.state.get() };
            match state {
                LockState::Unlocked => {}
                LockState::Locked {
                    waiters,
                    owner_id,
                    owner_prio,
                } => {
                    threads.set_priority(*owner_id, *owner_prio);
                    if let Some((pid, _)) = waiters.pop(&mut threads) {
                        *owner_id = pid;
                        *owner_prio = threads.get_priority(pid).unwrap();
                    } else {
                        *state = LockState::Unlocked
                    }
                }
            }
        })
    }
}

/// Grants access to the [`Mutex`] inner data.
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
