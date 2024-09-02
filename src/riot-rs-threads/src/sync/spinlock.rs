//! This module provides a Spinlock implementation.
use core::{
    cell::{RefCell, UnsafeCell},
    ops::{Deref, DerefMut},
};

use critical_section::Mutex;

/// A basic binary spinlock.
///
/// A `Spinlock` behaves like a Mutex, but carries no data.
/// This is supposed to be used to implement other locking primitives.
pub struct Spinlock<T> {
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

pub struct SpinlockGuard<'a, T> {
    lock: &'a Spinlock<T>,
}

impl<'a, T> Deref for SpinlockGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.inner.get() }
    }
}
impl<'a, T> Drop for SpinlockGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.release()
    }
}

pub struct SpinlockGuardMut<'a, T> {
    lock: &'a Spinlock<T>,
}

impl<'a, T> Deref for SpinlockGuardMut<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.inner.get() }
    }
}

impl<'a, T> DerefMut for SpinlockGuardMut<'a, T> {
    fn deref_mut(&mut self) -> &mut <Self as Deref>::Target {
        unsafe { &mut *self.lock.inner.get() }
    }
}
impl<'a, T> Drop for SpinlockGuardMut<'a, T> {
    fn drop(&mut self) {
        self.lock.release()
    }
}

impl<T> Spinlock<T> {
    /// Creates new **unlocked** Spinlock
    pub const fn new(inner: T) -> Self {
        Self {
            state: Mutex::new(RefCell::new(LockState::Unlocked)),
            inner: UnsafeCell::new(inner),
        }
    }

    /// Creates new **locked** Spinlock
    pub const fn new_locked(inner: T) -> Self {
        Self {
            state: Mutex::new(RefCell::new(LockState::Locked(1))),
            inner: UnsafeCell::new(inner),
        }
    }

    /// Returns the current lock state
    ///
    /// true if locked, false otherwise
    pub fn is_locked(&self) -> bool {
        crate::global_cs_with(|cs| {
            let state = self.state.borrow(cs).borrow();
            matches!(*state, LockState::Locked { .. })
        })
    }

    /// Get this spinlock (blocking)
    ///
    /// If the spinlock was unlocked, it will be locked and the function returns.
    /// If the spinlock was locked, this function will unschedule the current thread until the
    /// spinlock gets unlocked elsewhere.
    ///
    /// **NOTE**: must not be called outside thread context!
    pub fn acquire(&self) -> SpinlockGuard<T> {
        while !crate::global_cs_with(|cs| {
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
        }) {}
        SpinlockGuard { lock: &self }
    }

    pub fn acquire_mut(&self) -> SpinlockGuardMut<T> {
        while !crate::global_cs_with(|cs| {
            let mut state = self.state.borrow(cs).borrow_mut();
            if *state == LockState::Unlocked {
                *state = LockState::LockedMut;
                true
            } else {
                false
            }
        }) {}
        SpinlockGuardMut { lock: &self }
    }

    /// Get this spinlock (blocking)
    ///
    /// If the spinlock was unlocked, it will be locked and the function returns.
    /// If the spinlock was locked, this function will unschedule the current thread until the
    /// spinlock gets unlocked elsewhere.
    ///
    /// **NOTE**: must not be called outside thread context!
    pub fn try_acquire(&self) -> Option<SpinlockGuard<T>> {
        crate::global_cs_with(|cs| {
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
        })
        .then(|| SpinlockGuard { lock: &self })
    }

    pub fn try_acquire_mut(&self) -> Option<SpinlockGuardMut<T>> {
        crate::global_cs_with(|cs| {
            let mut state = self.state.borrow(cs).borrow_mut();
            if *state == LockState::Unlocked {
                *state = LockState::LockedMut;
                true
            } else {
                false
            }
        })
        .then(|| SpinlockGuardMut { lock: &self })
    }

    /// Releases the spinlock.
    ///
    /// If the spinlock was locked, and there were waiters, the first waiter will be
    /// woken up.
    /// If the spinlock was locked and there were no waiters, the lock will be unlocked.
    /// If the spinlock was not locked, the function just returns.
    fn release(&self) {
        crate::global_cs_with(|cs| {
            let mut state = self.state.borrow(cs).borrow_mut();
            match *state {
                LockState::Locked(1) | LockState::LockedMut => *state = LockState::Unlocked,
                LockState::Locked(ref mut count) => *count -= 1,
                LockState::Unlocked => {}
            }
        });
    }
}

impl Default for Spinlock<()> {
    fn default() -> Self {
        Self::new(())
    }
}

unsafe impl<T> Sync for Spinlock<T> {}
