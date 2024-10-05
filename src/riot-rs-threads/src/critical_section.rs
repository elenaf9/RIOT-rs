pub use critical_section::CriticalSection;

#[cfg(feature = "multi-core")]
use crate::smp::{Chip, Multicore};

/// Executes a function inside a critical section.
///
/// Mutual exclusion with `critical_section::with` is not guaranteed.
/// The implementation may use `critical_section::with`, but can also be
/// independent.
pub fn with<R>(f: impl FnOnce(CriticalSection<'_>) -> R) -> R {
    #[cfg(not(feature = "multi-core"))]
    {
        critical_section::with(f)
    }
    #[cfg(feature = "multi-core")]
    {
        unsafe {
            Chip::no_preemption_with(|| multicore_lock_with::<0, _>(|| f(CriticalSection::new())))
        }
    }
}

/// Prevents that the function is preempted during execution by disabling
/// the scheduler.
pub fn no_preemption_with<R>(f: impl FnOnce() -> R) -> R {
    #[cfg(not(feature = "multi-core"))]
    {
        critical_section::with(|_| f())
    }
    #[cfg(feature = "multi-core")]
    {
        Chip::no_preemption_with(f)
    }
}

/// Executes a function inside a critical section.
///
/// Mutual exclusion with `critical_section::with` is not guaranteed.
/// The implementation may use `critical_section::with`, but can also be
/// independent.
pub fn multicore_lock_with<const N: usize, R>(f: impl FnOnce() -> R) -> R {
    #[cfg(not(feature = "multi-core"))]
    {
        f()
    }
    #[cfg(feature = "multi-core")]
    {
        struct Guard<const N: usize> {
            restore_state: <Chip as Multicore>::LockRestoreState,
        }
        impl<const N: usize> Drop for Guard<N> {
            fn drop(&mut self) {
                Chip::multicore_lock_release::<N>(self.restore_state);
            }
        }

        let restore_state = Chip::multicore_lock_acquire::<N>();
        let _guard = Guard::<N> { restore_state };
        f()
    }
}
