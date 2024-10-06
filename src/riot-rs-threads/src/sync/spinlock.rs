//! This module provides a GenericSpinlock implementation.
use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    sync::atomic::Ordering,
};

pub use backend_cs::Cs;

#[cfg(target_has_atomic)]
pub use backend_atomic::Atomic;
#[cfg(context = "rp2040")]
pub use backend_hardware::Hardware;
#[cfg(not(feature = "multi-core"))]
pub use backend_noop::Noop;

/// Trait for the spinlock backend that implements the
/// acquisition and release of the lock.
pub trait SpinlockBackend<const N: usize> {
    /// Try acquire access to the spinlock.
    fn try_acquire(&self) -> bool;

    /// Release access to spinlock.
    fn release(&self);
}

#[cfg(all(feature = "multi-core", context = "rp2040"))]
type Backend<const N: usize> = Hardware<N>;

// RP2040 doesn't have atomic, so no duplicated identifier here.
#[cfg(all(feature = "multi-core", target_has_atomic))]
type Backend<const N: usize> = Atomic;

#[cfg(all(
    feature = "multi-core",
    not(any(context = "rp2040", target_has_atomic))
))]
type Backend<const N: usize> = Cs;

#[cfg(not(feature = "multi-core"))]
type Backend<const N: usize> = Noop;

/// Spinlock with default backend, used for all internal spinlocks.
pub type Spinlock<T, const N: usize = 0> = GenericSpinlock<T, Backend<N>, N>;
/// Guard for [`Spinlock`].
pub type SpinlockGuard<'a, T, const N: usize> = GenericSpinlockGuard<'a, T, Backend<N>, N>;

/// A generic spinlock that supports multiple backends
/// for acquiring and releasing the lock.
pub struct GenericSpinlock<T, B, const N: usize> {
    backend: B,
    inner: UnsafeCell<T>,
}

impl<T, B, const N: usize> GenericSpinlock<T, B, N>
where
    B: SpinlockBackend<N>,
{
    /// Acquire the spinlock to get immutable access to the inner data.
    pub fn lock(&self) -> GenericSpinlockGuard<T, B, N> {
        while !self.backend.try_acquire() {
            core::hint::spin_loop();
        }
        core::sync::atomic::fence(Ordering::Acquire);
        GenericSpinlockGuard { lock: self }
    }

    fn release(&self) {
        core::sync::atomic::fence(Ordering::Release);
        self.backend.release();
    }
}

#[cfg(not(feature = "multi-core"))]
impl<T, const N: usize> GenericSpinlock<T, Noop, N> {
    pub const fn new(inner: T) -> Self {
        Self {
            inner: UnsafeCell::new(inner),
            backend: Noop,
        }
    }
    #[allow(dead_code)]
    pub(crate) const fn new_internal(inner: T) -> Self {
        Self::new(inner)
    }
}

#[cfg(target_has_atomic)]
impl<T, const N: usize> GenericSpinlock<T, Atomic, N> {
    pub const fn new(inner: T) -> Self {
        Self {
            inner: UnsafeCell::new(inner),
            backend: Atomic::new(),
        }
    }
    #[allow(dead_code)]
    pub(crate) const fn new_internal(inner: T) -> Self {
        Self::new(inner)
    }
}

impl<T, const N: usize> GenericSpinlock<T, Cs, N> {
    pub const fn new(inner: T) -> Self {
        Self {
            inner: UnsafeCell::new(inner),
            backend: Cs::new(),
        }
    }
    #[allow(dead_code)]
    pub(crate) const fn new_internal(inner: T) -> Self {
        Self::new(inner)
    }
}

#[cfg(context = "rp2040")]
impl<T, const N: usize> GenericSpinlock<T, Hardware<N>, N> {
    pub const fn new(inner: T) -> Self {
        Self {
            inner: UnsafeCell::new(inner),
            backend: Hardware::new(),
        }
    }
    #[allow(dead_code)]
    pub const fn new_internal(inner: T) -> Self {
        Self {
            inner: UnsafeCell::new(inner),
            backend: Hardware::new_internal(),
        }
    }
}

/// Grants access to a [`GenericSpinlock`] inner data.
///
/// Dropping the [`GenericSpinlockGuard`] will unlock the [`GenericSpinlock`];
pub struct GenericSpinlockGuard<'a, T, B, const N: usize>
where
    B: SpinlockBackend<N>,
{
    lock: &'a GenericSpinlock<T, B, N>,
}

impl<'a, T, B, const N: usize> GenericSpinlockGuard<'a, T, B, N>
where
    B: SpinlockBackend<N>,
{
    /// Release the lock.
    pub fn release(self) {
        // dropping self will automatically release the lock.
    }
}

impl<'a, T, B, const N: usize> Deref for GenericSpinlockGuard<'a, T, B, N>
where
    B: SpinlockBackend<N>,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.inner.get() }
    }
}

impl<'a, T, B, const N: usize> DerefMut for GenericSpinlockGuard<'a, T, B, N>
where
    B: SpinlockBackend<N>,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.inner.get() }
    }
}

impl<'a, T, B, const N: usize> Drop for GenericSpinlockGuard<'a, T, B, N>
where
    B: SpinlockBackend<N>,
{
    fn drop(&mut self) {
        self.lock.release();
    }
}

unsafe impl<T, B, const N: usize> Sync for GenericSpinlock<T, B, N> {}

#[cfg(not(feature = "multi-core"))]
mod backend_noop {
    use super::SpinlockBackend;

    /// Spinlock that is using atomics to represent the spinlock state.
    pub struct Noop;

    impl<const N: usize> SpinlockBackend<N> for Noop {
        fn try_acquire(&self) -> bool {
            true
        }

        fn release(&self) {}
    }
}

/// Backend that uses atomics to represent the spinlock state.
#[cfg(target_has_atomic)]
mod backend_atomic {
    use core::sync::atomic::{AtomicUsize, Ordering};

    use super::SpinlockBackend;

    /// Spinlock that is using atomics to represent the spinlock state.
    pub struct Atomic {
        state: AtomicUsize,
    }

    impl Atomic {
        pub const fn new() -> Self {
            Self {
                state: AtomicUsize::new(0),
            }
        }
    }

    impl<const N: usize> SpinlockBackend<N> for Atomic {
        fn try_acquire(&self) -> bool {
            self.state.swap(1, Ordering::Relaxed) == 0
        }

        fn release(&self) {
            self.state.store(0, Ordering::Relaxed);
        }
    }
}

/// Backend that uses a critical-section protected state enum.
mod backend_cs {
    use core::{cell::UnsafeCell, usize};

    use crate::critical_section::multicore_lock_with;

    use super::SpinlockBackend;

    /// Spinlock backend with a critical-section protected state enum.
    ///
    /// It differentiates between read and write accesses.
    pub struct Cs {
        state: UnsafeCell<LockState>,
    }

    impl Cs {
        pub const fn new() -> Self {
            Self {
                state: UnsafeCell::new(LockState::Unlocked),
            }
        }
    }

    #[derive(Debug, PartialEq, Eq)]
    #[cfg_attr(feature = "defmt", derive(defmt::Format))]
    enum LockState {
        Unlocked,
        Locked,
    }

    impl<const N: usize> SpinlockBackend<N> for Cs {
        fn try_acquire(&self) -> bool {
            multicore_lock_with::<0, _>(|| {
                let state = unsafe { &mut *self.state.get() };
                if let LockState::Unlocked = state {
                    *state = LockState::Locked;
                    true
                } else {
                    false
                }
            })
        }
        fn release(&self) {
            multicore_lock_with::<0, _>(|| {
                let state = unsafe { &mut *self.state.get() };
                *state = LockState::Unlocked
            });
        }
    }
}

/// Backend based on hardware spinlocks.
#[cfg(context = "rp2040")]
mod backend_hardware {
    use rp_pac::SIO;

    use super::SpinlockBackend;

    const RESERVED: usize = 9;

    /// Spinlock backend based on hardware spinlocks.
    pub struct Hardware<const N: usize>;

    impl<const N: usize> Hardware<N> {
        pub const fn new() -> Self {
            const {
                assert!(
                    N > RESERVED,
                    "Spinlock 0..10 are reserved for internal use."
                )
            };
            const { assert!(N < 32, "Only 32 Spinlocks are supported") };
            Self {}
        }

        pub const fn new_internal() -> Self {
            const { assert!(N < 10, "Internal Spinlock must be in range 0..10") };
            Self {}
        }
    }

    impl<const N: usize> SpinlockBackend<N> for Hardware<N> {
        fn try_acquire(&self) -> bool {
            SIO.spinlock(N).read() != 0
        }

        fn release(&self) {
            // Release the spinlock to allow others to enter critical_section again
            SIO.spinlock(N).write_value(1);
        }
    }
}
