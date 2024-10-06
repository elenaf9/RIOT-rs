//! Synchronization primitives.
mod atomic_lock;
mod channel;
mod cs_lock;
mod lock;
mod mutex;
mod spinlock;

pub use atomic_lock::{AtomicLock, AtomicLockGuard, AtomicLockGuardMut};
pub use channel::Channel;
pub use cs_lock::{CsLock, CsLockGuard, CsLockGuardMut};
pub use lock::Lock;
pub use mutex::{Mutex, MutexGuard};
pub use spinlock::{Spinlock, SpinlockGuard, SpinlockGuardMut};

pub type ILock<T> = AtomicLock<T>;
pub type ILockGuard<'a, T> = AtomicLockGuard<'a, T>;
pub type ILockGuardMut<'a, T> = AtomicLockGuardMut<'a, T>;
