//! This module provides a Spinlock implementation.
use core::{cell::UnsafeCell, sync::atomic::Ordering, usize};

use critical_section::CriticalSection;
use portable_atomic::AtomicUsize;

use crate::smp::NoPreemptionToken;

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

    pub fn with<'a, F, R>(&self, _: &mut NoPreemptionToken, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        while self
            .state
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |val| {
                (val < usize::MAX).then_some(val + 1)
            })
            .is_err()
        {}
        let inner = unsafe { &*self.inner.get() };
        let res = f(inner);
        self.state.sub(1, Ordering::AcqRel);
        res
    }

    pub fn with_cs<F, R>(&self, _: CriticalSection, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        while self
            .state
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |val| {
                (val < usize::MAX).then_some(val + 1)
            })
            .is_err()
        {}
        let inner = unsafe { &*self.inner.get() };
        let res = f(inner);
        self.state.sub(1, Ordering::AcqRel);
        res
    }

    pub fn with_mut<'a, F, R>(&self, _: &mut NoPreemptionToken, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        while self
            .state
            .compare_exchange(0, usize::MAX, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {}
        let inner = unsafe { &mut *self.inner.get() };
        let res = f(inner);
        self.state.store(0, Ordering::Release);
        res
    }

    pub fn with_mut_cs<F, R>(&self, _: CriticalSection, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        while self
            .state
            .compare_exchange(0, usize::MAX, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {}
        let inner = unsafe { &mut *self.inner.get() };
        let res = f(inner);
        self.state.store(0, Ordering::Release);
        res
    }
}
unsafe impl<T> Sync for AtomicLock<T> {}
