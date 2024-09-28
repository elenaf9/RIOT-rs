//! This module provides a Spinlock implementation.
use core::cell::{RefCell, UnsafeCell};

use critical_section::{CriticalSection, Mutex};

use crate::smp::{Chip, Multicore};

/// A basic spinlock.
pub struct Spinlock<T> {
    state: Mutex<RefCell<LockState>>,
    inner: UnsafeCell<T>,
}

#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
enum LockState {
    Unlocked,
    Locked,
}

impl<T> Spinlock<T> {
    /// Creates new Spinlock.
    pub const fn new(inner: T) -> Self {
        Self {
            state: Mutex::new(RefCell::new(LockState::Unlocked)),
            inner: UnsafeCell::new(inner),
        }
    }

    pub fn with<'a, F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        while !Chip::multicore_lock_with(|cs| self.try_acquire(cs)) {}
        let inner = unsafe { &mut *self.inner.get() };
        let res = f(inner);
        Chip::multicore_lock_with(|cs| self.release(cs));
        res
    }

    pub fn with_cs<F, R>(&self, cs: CriticalSection, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        while !self.try_acquire(cs) {}
        let inner = unsafe { &mut *self.inner.get() };
        let res = f(inner);
        self.release(cs);
        res
    }

    fn try_acquire(&self, cs: CriticalSection) -> bool {
        let mut state = self.state.borrow(cs).borrow_mut();
        if *state == LockState::Unlocked {
            *state = LockState::Locked;
            true
        } else {
            false
        }
    }

    fn release(&self, cs: CriticalSection) {
        let mut state = self.state.borrow(cs).borrow_mut();
        *state = LockState::Unlocked;
    }
}
unsafe impl<T> Sync for Spinlock<T> {}
