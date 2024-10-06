//! This module provides a Spinlock implementation.
use core::{
    cell::{RefCell, UnsafeCell},
    ops::{Deref, DerefMut},
};

use critical_section::{CriticalSection, Mutex};

use crate::smp::multicore_lock_with;

/// A basic spinlock.
pub struct Spinlock<T, const N: usize> {
    state: Mutex<RefCell<LockState>>,
    inner: UnsafeCell<T>,
}

#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
enum LockState {
    Unlocked,
    Locked(usize),
    LockedMut,
}

impl<T, const N: usize> Spinlock<T, N> {
    /// Creates new Spinlock.
    pub const fn new(inner: T) -> Self {
        Self {
            state: Mutex::new(RefCell::new(LockState::Unlocked)),
            inner: UnsafeCell::new(inner),
        }
    }

    pub fn lock(&self) -> SpinlockGuard<T, N> {
        while !multicore_lock_with::<0, _>(|cs| self.try_acquire(cs)) {}
        SpinlockGuard { lock: self }
    }

    pub fn lock_mut(&self) -> SpinlockGuardMut<T, N> {
        while !multicore_lock_with::<0, _>(|cs| self.try_acquire_mut(cs)) {}
        SpinlockGuardMut { lock: self }
    }

    fn release(&self) {
        multicore_lock_with::<0, _>(|cs| self.release_cs(cs));
    }

    fn release_mut(&self) {
        multicore_lock_with::<0, _>(|cs| self.release_cs(cs));
    }

    fn try_acquire(&self, cs: CriticalSection) -> bool {
        let mut state = self.state.borrow(cs).borrow_mut();
        match *state {
            LockState::Unlocked => {
                *state = LockState::Locked(1);
                true
            }
            LockState::Locked(ref mut count) => {
                *count += 1;
                true
            }
            _ => false,
        }
    }

    fn try_acquire_mut(&self, cs: CriticalSection) -> bool {
        let mut state = self.state.borrow(cs).borrow_mut();
        if *state == LockState::Unlocked {
            *state = LockState::LockedMut;
            true
        } else {
            false
        }
    }

    fn release_cs(&self, cs: CriticalSection) {
        let mut state = self.state.borrow(cs).borrow_mut();
        match *state {
            LockState::Locked(1) | LockState::LockedMut => *state = LockState::Unlocked,
            LockState::Locked(ref mut count) => *count -= 1,
            LockState::Unlocked => {}
        }
    }
}

/// Grants access to a [`Mutex`] inner data.
///
/// Dropping the [`MutexGuard`] will unlock the [`Mutex`];
pub struct SpinlockGuard<'a, T, const N: usize> {
    lock: &'a Spinlock<T, N>,
}

impl<'a, T, const N: usize> SpinlockGuard<'a, T, N> {
    pub fn release(self) {
        // dropping self will automatically release the lock.
    }
}

impl<'a, T, const N: usize> Deref for SpinlockGuard<'a, T, N> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.inner.get() }
    }
}

impl<'a, T, const N: usize> Drop for SpinlockGuard<'a, T, N> {
    fn drop(&mut self) {
        self.lock.release();
    }
}

pub struct SpinlockGuardMut<'a, T, const N: usize> {
    lock: &'a Spinlock<T, N>,
}

impl<'a, T, const N: usize> SpinlockGuardMut<'a, T, N> {
    pub fn release(self) {
        // dropping self will automatically release the lock.
    }
}

impl<'a, T, const N: usize> Deref for SpinlockGuardMut<'a, T, N> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.inner.get() }
    }
}

impl<'a, T, const N: usize> DerefMut for SpinlockGuardMut<'a, T, N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.inner.get() }
    }
}

impl<'a, T, const N: usize> Drop for SpinlockGuardMut<'a, T, N> {
    fn drop(&mut self) {
        self.lock.release_mut();
    }
}

unsafe impl<T, const N: usize> Sync for Spinlock<T, N> {}
