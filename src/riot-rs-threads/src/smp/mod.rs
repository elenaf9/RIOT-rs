use core::cell::UnsafeCell;

use critical_section::CriticalSection;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct CoreId(u8);

impl CoreId {
    pub const fn new(value: u8) -> Self {
        Self(value)
    }
}

impl From<CoreId> for usize {
    fn from(value: CoreId) -> Self {
        value.0 as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct CoreAffinity(u8);

impl CoreAffinity {
    pub const fn no_affinity() -> Self {
        Self(2 ^ Chip::CORES as u8 - 1)
    }

    #[cfg(feature = "core-affinity")]
    pub fn one(core: CoreId) -> Self {
        Self(1 << core.0)
    }

    #[cfg(feature = "core-affinity")]
    pub fn contains(&self, core: CoreId) -> bool {
        self.0 & (1 << core.0) > 0
    }
}

#[cfg(feature = "core-affinity")]
impl Default for CoreAffinity {
    fn default() -> Self {
        Self::no_affinity()
    }
}

pub trait Multicore {
    const CORES: u32;
    const SPINLOCKS: u8;

    fn core_id() -> CoreId;

    fn startup_cores();

    #[allow(dead_code)]
    fn wait_for_wakeup();

    fn schedule_on_core(id: CoreId);

    fn cs_with<const ID: usize, R>(f: impl FnOnce(CriticalSection<'_>) -> R) -> R;

    fn no_preemption_with<R>(f: impl FnOnce() -> R) -> R;
}

cfg_if::cfg_if! {
    if #[cfg(context = "rp2040")] {
        mod rp2040;
        pub use rp2040::Chip;
    }
    else {
        use crate::{Arch, Cpu};


        pub struct Chip;
        impl Multicore for Chip {
            const CORES: u32 = 1;
            const SPINLOCKS: u8 = u8::MAX;

            fn core_id() -> CoreId {
                CoreId(0)
            }

            fn startup_cores() {}

            fn wait_for_wakeup() {
                Cpu::wfi();
            }

            fn schedule_on_core(_id: CoreId) {
                Cpu::schedule();
            }

            fn cs_with<const ID: usize, R>(f: impl FnOnce(CriticalSection<'_>) -> R) -> R {
                unsafe  { f(CriticalSection::new()) }
            }

            fn no_preemption_with<R>(f: impl FnOnce() -> R ) -> R {
                critical_section::with(|_| f())
            }
        }
    }
}

pub fn schedule_on_core(id: CoreId) {
    Chip::schedule_on_core(id)
}

pub fn global_cs_with<R>(f: impl FnOnce(CriticalSection<'_>) -> R) -> R {
    no_preemption_with(|| Chip::cs_with::<0, _>(f))
}

pub fn no_preemption_with<R>(f: impl FnOnce() -> R) -> R {
    Chip::no_preemption_with(f)
}

pub struct MulticoreLock<const N: usize, T> {
    inner: UnsafeCell<T>,
}

impl<const N: usize, T> MulticoreLock<N, T> {
    /// Creates new **unlocked** Spinlock
    pub const fn new(inner: T) -> Self {
        Self {
            inner: UnsafeCell::new(inner),
        }
    }

    pub fn with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        Chip::cs_with::<N, _>(|cs| self.with_cs(cs, f))
    }

    pub fn with_cs<F, R>(&self, _: CriticalSection, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        let inner = unsafe { &mut *self.inner.get() };
        f(inner)
    }
}

unsafe impl<const N: usize, T> Sync for MulticoreLock<N, T> {}
