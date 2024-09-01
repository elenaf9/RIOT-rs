//! Synchronization primitives for RIOT-rs threads.

mod channel;
mod lock;
mod mutex;
mod spinlock;

pub use channel::Channel;
pub use lock::Lock;
pub use mutex::{Mutex, MutexGuard};
// pub use spinlock::{Spinlock, SpinlockGuard, SpinlockGuardMut};
pub use spinlock::Spinlock;
