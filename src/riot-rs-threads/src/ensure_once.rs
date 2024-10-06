//! This module provides a Mutex-protected RefCell --- basically a way to ensure
//! at runtime that some reference is used only once.
use core::cell::UnsafeCell;

use crate::smp::{Chip, Multicore};

pub(crate) struct EnsureOnce<T> {
    inner: UnsafeCell<T>,
}

impl<T> EnsureOnce<T> {
    pub const fn new(inner: T) -> Self {
        Self {
            inner: UnsafeCell::new(inner),
        }
    }

    pub fn with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        Chip::no_preemption_with(|| {
            let inner = unsafe { &mut *self.inner.get() };
            f(inner)
        })
    }
}

unsafe impl<T> Sync for EnsureOnce<T> {}
