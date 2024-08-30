//! This module provides a Spinlock-based wrapper that ensures
//! at runtime that some reference is used only once.

use crate::smp::no_preemption_with;
use crate::sync::Spinlock;

pub(crate) struct EnsureOnce<T> {
    inner: Spinlock<T>,
}

impl<T> EnsureOnce<T> {
    pub const fn new(inner: T) -> Self {
        Self {
            inner: Spinlock::new(inner),
        }
    }

    pub fn with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        no_preemption_with(|| {
            let inner = self.inner.acquire();
            f(&inner)
        })
    }

    pub fn with_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        no_preemption_with(|| {
            let mut inner = self.inner.acquire_mut();
            f(&mut inner)
        })
    }
}
