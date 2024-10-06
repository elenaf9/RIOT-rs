//! Synchronization primitives.
mod channel;
mod lock;
mod spinlock;

pub use channel::Channel;
pub use lock::Lock;
pub use spinlock::{
    Cs, GenericSpinlock, GenericSpinlockGuard, Spinlock, SpinlockBackend, SpinlockGuard,
};

#[cfg(target_has_atomic)]
pub use spinlock::Atomic;
#[cfg(context = "rp2040")]
pub use spinlock::Hardware;
