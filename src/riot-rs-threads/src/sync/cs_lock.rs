//! This module provides a CsLock implementation.
use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
};

use crate::smp::{Chip, Multicore};

/// A basic binary spinlock.
///
/// A `CsLock` behaves like a Mutex, but carries no data.
/// This is supposed to be used to implement other locking primitives.
pub struct CsLock<T, const N: usize> {
    inner: UnsafeCell<T>,
}

impl<T, const N: usize> CsLock<T, N> {
    pub const fn new(inner: T) -> Self {
        Self {
            inner: UnsafeCell::new(inner),
        }
    }

    pub fn lock(&self) -> CsLockGuard<T, N> {
        let token = Chip::multicore_lock_acquire::<N>();
        CsLockGuard { lock: self, token }
    }

    pub fn lock_mut(&self) -> CsLockGuardMut<T, N> {
        let token = Chip::multicore_lock_acquire::<N>();
        CsLockGuardMut { lock: self, token }
    }

    fn release(&self, token: <Chip as Multicore>::LockRestoreState) {
        Chip::multicore_lock_release::<N>(token);
    }
}

/// Grants access to a [`Mutex`] inner data.
///
/// Dropping the [`MutexGuard`] will unlock the [`Mutex`];
pub struct CsLockGuard<'a, T, const N: usize> {
    lock: &'a CsLock<T, N>,
    token: <Chip as Multicore>::LockRestoreState,
}

impl<'a, T, const N: usize> CsLockGuard<'a, T, N> {
    pub fn release(self) {
        // dropping self will automatically release the lock.
    }
}

impl<'a, T, const N: usize> Deref for CsLockGuard<'a, T, N> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.inner.get() }
    }
}

impl<'a, T, const N: usize> Drop for CsLockGuard<'a, T, N> {
    fn drop(&mut self) {
        self.lock.release(self.token);
    }
}

pub struct CsLockGuardMut<'a, T, const N: usize> {
    lock: &'a CsLock<T, N>,
    token: <Chip as Multicore>::LockRestoreState,
}

impl<'a, T, const N: usize> CsLockGuardMut<'a, T, N> {
    pub fn release(self) {
        // dropping self will automatically release the lock.
    }
}

impl<'a, T, const N: usize> Deref for CsLockGuardMut<'a, T, N> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.inner.get() }
    }
}

impl<'a, T, const N: usize> DerefMut for CsLockGuardMut<'a, T, N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.inner.get() }
    }
}

impl<'a, T, const N: usize> Drop for CsLockGuardMut<'a, T, N> {
    fn drop(&mut self) {
        self.lock.release(self.token);
    }
}

unsafe impl<T, const N: usize> Sync for CsLock<T, N> {}
