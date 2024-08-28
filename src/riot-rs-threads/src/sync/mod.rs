//! Synchronization primitives for RIOT-rs threads.

mod channel;
mod semaphore;
mod spinlock;

pub use channel::Channel;
pub use semaphore::Semaphore;
pub use spinlock::{Spinlock, SpinlockGuard, SpinlockGuardMut};
