//! Synchronization primitives.
mod atomic_lock;
mod channel;
mod lock;
mod mutex;

pub use atomic_lock::AtomicLock;
pub use channel::Channel;
pub use lock::Lock;
pub use mutex::{Mutex, MutexGuard};
