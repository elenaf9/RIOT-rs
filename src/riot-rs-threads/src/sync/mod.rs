//! Synchronization primitives.
mod atomic_lock;
mod channel;
mod cs_lock;
mod lock;
mod mutex;
mod spinlock;

pub use atomic_lock::AtomicLock;
pub use channel::Channel;
pub use cs_lock::CsLock;
pub use lock::Lock;
pub use mutex::{Mutex, MutexGuard};
pub use spinlock::Spinlock;
