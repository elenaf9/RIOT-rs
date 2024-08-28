//! This module provides a Mutex implementation.
use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
};

use super::Lock;

/// A basic Mutex implementation for shared mutable access to an object.
///
/// Builds on [`Lock`](super::Lock).
pub struct Mutex<T> {
    lock: Lock,
    inner: UnsafeCell<T>,
}

unsafe impl<T> Sync for Mutex<T> {}

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
        self.mutex.lock.release()
    }
}

impl<T> Mutex<T> {
    /// Creates new **unlocked** Mutex
    pub const fn new(inner: T) -> Self {
        Self {
            lock: Lock::new(),
            inner: UnsafeCell::new(inner),
        }
    }

    /// Creates new **locked** Mutex
    pub const fn new_locked(inner: T) -> Self {
        Self {
            lock: Lock::new(),
            inner: UnsafeCell::new(inner),
        }
    }

    /// Returns the current lock state.
    pub fn is_locked(&self) -> bool {
        self.lock.is_locked()
    }

    /// Get this mutex (blocking).
    ///
    /// If another thread currently holds the mutex, the current thread
    /// is put to sleep until the mutex is available.
    pub fn lock(&self) -> MutexGuard<T> {
        self.lock.acquire();
        MutexGuard { mutex: self }
    }

    /// Get the mutex (non-blocking)
    ///
    /// If the mutex was unlocked, it will be locked and the function returns
    /// an [`MutexGuard`], else [`None`] is returned.
    pub fn try_lock(&self) -> Option<MutexGuard<T>> {
        self.lock.try_acquire().then(|| MutexGuard { mutex: self })
    }
}
