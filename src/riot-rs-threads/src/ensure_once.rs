//! This module provides a Mutex-protected RefCell --- basically a way to ensure
//! at runtime that some reference is used only once.
use critical_section::CriticalSection;

use crate::smp::{Chip, Multicore};

pub(crate) struct EnsureOnce<T> {
    inner: crate::sync::Spinlock<T>,
}

impl<T> EnsureOnce<T> {
    pub const fn new(inner: T) -> Self {
        Self {
            inner: crate::sync::Spinlock::new(inner),
        }
    }

    pub fn with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        Chip::no_preemption_with(|| self.inner.with(|t| f(t)))
    }

    pub fn with_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        Chip::no_preemption_with(|| self.inner.with(f))
    }

    #[allow(dead_code)]
    pub fn with_cs<F, R>(&self, cs: CriticalSection, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        self.inner.with_cs(cs, |t| f(t))
    }

    #[allow(dead_code)]
    pub fn with_mut_cs<F, R>(&self, cs: CriticalSection, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        self.inner.with_cs(cs, f)
    }
}
