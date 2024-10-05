//! Synchronization primitives.
mod atomic_lock;
mod channel;
mod lock;
mod spinlock;

#[cfg(feature = "multi-core")]
mod cs_lock;

pub use atomic_lock::{AtomicLock, AtomicLockGuard};
pub use channel::Channel;
pub use lock::Lock;
pub use spinlock::{Spinlock, SpinlockGuard};

#[cfg(feature = "multi-core")]
pub use cs_lock::{CsLock, CsLockGuard};

pub type ILock<T> = Spinlock<T>;
pub type ILockGuard<'a, T> = SpinlockGuard<'a, T>;
