//! This module provides a Wrapper that ensures that the accessing thread
//! is not preempted while access the inner object.

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
}
