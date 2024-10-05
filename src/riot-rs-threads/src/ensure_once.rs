//! This module provides a Mutex-protected RefCell --- basically a way to ensure
//! at runtime that some reference is used only once.
use crate::smp::{Chip, Multicore};

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
        Chip::no_preemption_with(|| f(&self.inner))
    }
}
