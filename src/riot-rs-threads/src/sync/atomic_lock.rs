//! This module provides a Spinlock implementation.
use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    sync::atomic::Ordering,
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
        while self.state.swap(1, Ordering::Acquire) > 0 {}
        AtomicLockGuard { lock: self }
    }

    fn release(&self) {
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

impl<'a, T> DerefMut for AtomicLockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.inner.get() }
    }
}

impl<'a, T> Drop for AtomicLockGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.release();
    }
}
unsafe impl<T> Sync for AtomicLock<T> {}
