//! This module provides a Spinlock implementation.
use core::{cell::UnsafeCell, sync::atomic::Ordering};

use critical_section::CriticalSection;
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

    pub fn with<'a, F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        while self.state.swap(1, Ordering::Acquire) != 0 {}
        let inner = unsafe { &mut *self.inner.get() };
        let res = f(inner);
        self.state.store(0, Ordering::Release);
        res
    }

    pub fn with_cs<F, R>(&self, _: CriticalSection, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        self.with(f)
    }
}
unsafe impl<T> Sync for AtomicLock<T> {}
