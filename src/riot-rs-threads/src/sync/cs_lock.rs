//! This module provides a Spinlock implementation.
use core::cell::UnsafeCell;

use critical_section::CriticalSection;

use crate::smp::{Chip, Multicore};

/// A basic binary spinlock.
///
/// A `Spinlock` behaves like a Mutex, but carries no data.
/// This is supposed to be used to implement other locking primitives.
pub struct CsLock<T> {
    inner: UnsafeCell<T>,
}

impl<T> CsLock<T> {
    pub const fn new(inner: T) -> Self {
        Self {
            inner: UnsafeCell::new(inner),
        }
    }

    pub fn with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        Chip::multicore_lock_with(|cs| self.with_cs(cs, f))
    }

    pub fn with_cs<F, R>(&self, _cs: CriticalSection, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        let inner = unsafe { &*self.inner.get() };
        f(inner)
    }

    pub fn with_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        Chip::multicore_lock_with(|cs| self.with_mut_cs(cs, f))
    }

    pub fn with_mut_cs<F, R>(&self, _cs: CriticalSection, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        let inner = unsafe { &mut *self.inner.get() };
        f(inner)
    }
}

unsafe impl<T> Sync for CsLock<T> {}
