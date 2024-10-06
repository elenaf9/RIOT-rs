//! Synchronization primitives.
mod channel;
mod lock;
mod mutex;
mod spinlock;

pub use channel::Channel;
pub use lock::Lock;
pub use mutex::{Mutex, MutexGuard};
pub use spinlock::{
    Cs, GenericSpinlock, GenericSpinlockGuard, GenericSpinlockGuardMut, Spinlock, SpinlockGuard,
    SpinlockGuardMut,
};

#[cfg(target_has_atomic)]
pub use spinlock::Atomic;
#[cfg(all(feature = "multicore", context = "rp2040"))]
pub use spinlock::Hardware;
