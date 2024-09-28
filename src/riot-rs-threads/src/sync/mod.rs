//! Synchronization primitives.
mod channel;
mod cs_lock;
mod lock;
mod spinlock;

pub use channel::Channel;
pub use cs_lock::CsLock;
pub use lock::Lock;
pub use spinlock::Spinlock;
