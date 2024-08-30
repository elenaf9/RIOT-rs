//! This module provides a Spinlock-based wrapper that ensures
//! at runtime that some reference is used only once.

use crate::smp::no_preemption_with;

pub(crate) struct EnsureOnce<T> {
    inner: T,
}

impl<T> EnsureOnce<T> {
    pub const fn new(inner: T) -> Self {
        Self { inner }
    }

    pub fn with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        no_preemption_with(|| f(&self.inner))
    }

    pub fn with_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        no_preemption_with(|| f(&self.inner))
    }
}
