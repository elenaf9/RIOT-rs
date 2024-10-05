//! Synchronization primitives.
mod atomic_lock;
mod channel;
mod lock;
mod spinlock;

#[cfg(feature = "multi-core")]
mod cs_lock;

pub use atomic_lock::{AtomicLock, AtomicLockGuard, AtomicLockGuardMut};
pub use channel::Channel;
pub use lock::Lock;
pub use spinlock::{Spinlock, SpinlockGuard, SpinlockGuardMut};

#[cfg(feature = "multi-core")]
pub use cs_lock::{CsLock, CsLockGuard, CsLockGuardMut};

pub type ILock<T> = AtomicLock<T>;
pub type ILockGuard<'a, T> = AtomicLockGuard<'a, T>;
pub type ILockGuardMut<'a, T> = AtomicLockGuardMut<'a, T>;
