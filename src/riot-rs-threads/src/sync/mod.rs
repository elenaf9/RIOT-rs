//! Synchronization primitives for RIOT-rs threads.

mod channel;
mod lock;
mod spinlock;

pub use channel::Channel;
pub use lock::Lock;
pub use spinlock::{Spinlock, SpinlockGuard, SpinlockGuardMut};
