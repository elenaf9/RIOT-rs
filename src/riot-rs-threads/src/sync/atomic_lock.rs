//! This module provides a Spinlock implementation.
use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    sync::atomic::Ordering,
    usize,
};

use portable_atomic::AtomicUsize;

/// A basic spinlock.
pub struct AtomicLock<T> {
    state: AtomicUsize,
    inner: UnsafeCell<T>,
}

impl<T> AtomicLock<T> {
    /// Creates new Spinlock.
    pub const fn new(inner: T) -> Self {
        Self {
            state: AtomicUsize::new(0),
            inner: UnsafeCell::new(inner),
        }
    }

    pub fn lock(&self) -> AtomicLockGuard<T> {
        while self
            .state
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |val| {
                (val < usize::MAX).then_some(val + 1)
            })
            .is_err()
        {}
        AtomicLockGuard { lock: self }
    }

    pub fn lock_mut(&self) -> AtomicLockGuardMut<T> {
        while self
            .state
            .compare_exchange(0, usize::MAX, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {}
        AtomicLockGuardMut { lock: self }
    }

    fn release(&self) {
        self.state.sub(1, Ordering::AcqRel);
    }

    fn release_mut(&self) {
        self.state.store(0, Ordering::Release);
    }
}

/// Grants access to a [`Mutex`] inner data.
///
/// Dropping the [`MutexGuard`] will unlock the [`Mutex`];
pub struct AtomicLockGuard<'a, T> {
    lock: &'a AtomicLock<T>,
}

impl<'a, T> AtomicLockGuard<'a, T> {
    pub fn release(self) {
        // dropping self will automatically release the lock.
    }
}

impl<'a, T> Deref for AtomicLockGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.inner.get() }
    }
}

impl<'a, T> Drop for AtomicLockGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.release();
    }
}

pub struct AtomicLockGuardMut<'a, T> {
    lock: &'a AtomicLock<T>,
}

impl<'a, T> AtomicLockGuardMut<'a, T> {
    pub fn release(self) {
        // dropping self will automatically release the lock.
    }
}

impl<'a, T> Deref for AtomicLockGuardMut<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.inner.get() }
    }
}

impl<'a, T> DerefMut for AtomicLockGuardMut<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.inner.get() }
    }
}

impl<'a, T> Drop for AtomicLockGuardMut<'a, T> {
    fn drop(&mut self) {
        self.lock.release_mut();
    }
}

unsafe impl<T> Sync for AtomicLock<T> {}
