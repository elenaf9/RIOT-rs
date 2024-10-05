//! This module provides a Mutex-protected RefCell --- basically a way to ensure
//! at runtime that some reference is used only once.
use critical_section::CriticalSection;

use crate::critical_section;

pub(crate) struct SchedulerLock<T> {
    inner: T,
}

impl<T> SchedulerLock<T> {
    pub const fn new(inner: T) -> Self {
        Self { inner }
    }

    pub fn with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        critical_section::no_preemption_with(|| f(&self.inner))
    }

    pub fn with_cs<F, R>(&self, _cs: CriticalSection, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        f(&self.inner)
    }
}
