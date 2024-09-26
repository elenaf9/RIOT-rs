use core::cell::UnsafeCell;

use crate::{thread::ThreadState, threadlist::ThreadList, THREADS};

use super::MutexGuard;

/// Blocks a thread while its waiting for an event to occur.
pub struct Condvar {
    waiters: UnsafeCell<ThreadList>,
}

impl Condvar {
    /// Creates a new condition variable.
    pub const fn new() -> Self {
        Condvar {
            waiters: UnsafeCell::new(ThreadList::new()),
        }
    }

    /// Blocks the current thread until this condition variable receives a notification.
    ///
    /// The function unlocks the mutex and puts the current thread to sleep until a signal on
    /// the condition variable is received.
    pub fn wait<'a, T>(&self, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
        let mutex = critical_section::with(move |cs| {
            let waiters = unsafe { &mut *self.waiters.get() };
            THREADS.with_mut_cs(cs, |mut threads| {
                waiters.put_current(&mut threads, ThreadState::CondVarBlocked)
            });
            guard.guard_lock()
            // Guard is dropped and the mutex therefore unlocked.
        });

        // Thread was put to sleep and only continues running here after a signal on
        // this `Condvar` was received.

        mutex.lock()
    }

    /// Wakes up one blocked thread on this condvar.
    ///
    /// If there is a blocked thread on this condition variable, then it will be woken up
    /// from its call to wait or wait_timeout. Calls to `notify_one`` are not buffered in any way.
    ///
    /// If there are multiple threads waiting, the thread with the highest priority will be picked.
    /// Within a priority, threads are notified in FIFO order.
    pub fn notify_one(&self) {
        critical_section::with(|cs| {
            let waiters = unsafe { &mut *self.waiters.get() };
            THREADS.with_mut_cs(cs, |mut threads| waiters.pop(&mut threads))
        });
    }

    /// Wakes up all blocked threads on this condvar.
    ///
    /// This method will ensure that any current waiters on the condition variable are awoken.
    /// Calls to `notify_all` are not buffered in any way.
    pub fn notify_all(&self) {
        critical_section::with(|cs| {
            let waiters = unsafe { &mut *self.waiters.get() };
            THREADS.with_mut_cs(cs, |mut threads| waiters.drain(&mut threads))
        })
    }
}

unsafe impl Sync for Condvar {}
