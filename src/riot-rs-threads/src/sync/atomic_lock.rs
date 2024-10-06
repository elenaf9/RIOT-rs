//! This module provides a Spinlock implementation.
use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    sync::atomic::Ordering,
    usize,
};

use portable_atomic::AtomicUsize;

/// A basic spinlock.
pub struct AtomicLock<T, const N: usize> {
    state: AtomicUsize,
    inner: UnsafeCell<T>,
}

impl<T, const N: usize> AtomicLock<T, N> {
    /// Creates new Spinlock.
    pub const fn new(inner: T) -> Self {
        Self {
            state: AtomicUsize::new(0),
            inner: UnsafeCell::new(inner),
        }
    }

    pub fn lock(&self) -> AtomicLockGuard<T, N> {
        while self
            .state
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |val| {
                (val < usize::MAX).then_some(val + 1)
            })
            .is_err()
        {}
        AtomicLockGuard { lock: self }
    }

    pub fn lock_mut(&self) -> AtomicLockGuardMut<T, N> {
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
pub struct AtomicLockGuard<'a, T, const N: usize> {
    lock: &'a AtomicLock<T, N>,
}

impl<'a, T, const N: usize> AtomicLockGuard<'a, T, N> {
    pub fn release(self) {
        // dropping self will automatically release the lock.
    }
}

impl<'a, T, const N: usize> Deref for AtomicLockGuard<'a, T, N> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.inner.get() }
    }
}

impl<'a, T, const N: usize> Drop for AtomicLockGuard<'a, T, N> {
    fn drop(&mut self) {
        self.lock.release();
    }
}

pub struct AtomicLockGuardMut<'a, T, const N: usize> {
    lock: &'a AtomicLock<T, N>,
}

impl<'a, T, const N: usize> AtomicLockGuardMut<'a, T, N> {
    pub fn release(self) {
        // dropping self will automatically release the lock.
    }
}

impl<'a, T, const N: usize> Deref for AtomicLockGuardMut<'a, T, N> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.inner.get() }
    }
}

impl<'a, T, const N: usize> DerefMut for AtomicLockGuardMut<'a, T, N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.inner.get() }
    }
}

impl<'a, T, const N: usize> Drop for AtomicLockGuardMut<'a, T, N> {
    fn drop(&mut self) {
        self.lock.release_mut();
    }
}

unsafe impl<T, const N: usize> Sync for AtomicLock<T, N> {}
