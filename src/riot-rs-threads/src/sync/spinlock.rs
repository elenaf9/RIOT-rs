//! This module provides a Spinlock implementation.
use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
};

use crate::critical_section::multicore_lock_with;

/// A basic spinlock.
pub struct Spinlock<T> {
    state: UnsafeCell<LockState>,
    inner: UnsafeCell<T>,
}

#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
enum LockState {
    Unlocked,
    Locked(usize),
    LockedMut,
}

impl<T> Spinlock<T> {
    /// Creates new Spinlock.
    pub const fn new(inner: T) -> Self {
        Self {
            state: UnsafeCell::new(LockState::Unlocked),
            inner: UnsafeCell::new(inner),
        }
    }

    pub fn lock(&self) -> SpinlockGuard<T> {
        while !multicore_lock_with::<0, _>(|| self.try_acquire()) {}
        SpinlockGuard { lock: self }
    }

    pub fn lock_mut(&self) -> SpinlockGuardMut<T> {
        while !multicore_lock_with::<0, _>(|| self.try_acquire_mut()) {}
        SpinlockGuardMut { lock: self }
    }

    fn release(&self) {
        multicore_lock_with::<0, _>(|| self.release_cs());
    }

    fn release_mut(&self) {
        multicore_lock_with::<0, _>(|| self.release_cs());
    }

    fn try_acquire(&self) -> bool {
        let state = unsafe { &mut *self.state.get() };
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

    fn try_acquire_mut(&self) -> bool {
        let state = unsafe { &mut *self.state.get() };
        if *state == LockState::Unlocked {
            *state = LockState::LockedMut;
            true
        } else {
            false
        }
    }

    fn release_cs(&self) {
        let state = unsafe { &mut *self.state.get() };
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
pub struct SpinlockGuard<'a, T> {
    lock: &'a Spinlock<T>,
}

impl<'a, T> SpinlockGuard<'a, T> {
    pub fn release(self) {
        // dropping self will automatically release the lock.
    }
}

impl<'a, T> Deref for SpinlockGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.inner.get() }
    }
}

impl<'a, T> Drop for SpinlockGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.release();
    }
}

pub struct SpinlockGuardMut<'a, T> {
    lock: &'a Spinlock<T>,
}

impl<'a, T> SpinlockGuardMut<'a, T> {
    pub fn release(self) {
        // dropping self will automatically release the lock.
    }
}

impl<'a, T> Deref for SpinlockGuardMut<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.inner.get() }
    }
}

impl<'a, T> DerefMut for SpinlockGuardMut<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.inner.get() }
    }
}

impl<'a, T> Drop for SpinlockGuardMut<'a, T> {
    fn drop(&mut self) {
        self.lock.release_mut();
    }
}

unsafe impl<T> Sync for Spinlock<T> {}
