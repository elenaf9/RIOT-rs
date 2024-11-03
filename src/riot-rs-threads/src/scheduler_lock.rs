//! This module provides a Mutex-protected RefCell --- basically a way to ensure
//! at runtime that some reference is used only once.
use core::cell::UnsafeCell;

use critical_section::CriticalSection;

use crate::critical_section;

pub(crate) struct SchedulerLock<T> {
    inner: UnsafeCell<T>,
}

impl<T> SchedulerLock<T> {
    pub const fn new(inner: T) -> Self {
        Self {
            inner: UnsafeCell::new(inner),
        }
    }

    pub fn with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        critical_section::no_preemption_with(|| {
            let inner = unsafe { &mut *self.inner.get() };
            f(inner)
        })
    }

    pub fn with_cs<F, R>(&self, _cs: CriticalSection, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        let inner = unsafe { &mut *self.inner.get() };
        f(inner)
    }
}

unsafe impl<T> Sync for SchedulerLock<T> {}
