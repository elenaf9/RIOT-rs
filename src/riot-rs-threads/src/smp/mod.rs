use critical_section::CriticalSection;
use riot_rs_utils::usize_from_env_or;

use crate::CoreId;

impl CoreId {
    /// Creates a new [`CoreId`].
    ///
    /// # Panics
    ///
    /// Panics if `value` >= [`Chip::CORES`].
    pub fn new(value: u8) -> Self {
        if value >= Chip::CORES as u8 {
            panic!(
                "Invalid CoreId {}: only core ids 0..{} available.",
                value,
                Chip::CORES
            )
        }
        Self(value)
    }
}

pub trait Multicore {
    /// Number of available core.
    const CORES: u32;
    /// Stack size for the idle threads.
    const IDLE_THREAD_STACK_SIZE: usize = 256;

    /// Returns the ID of the current core.
    fn core_id() -> CoreId;

    /// Starts other available cores.
    ///
    /// This is called at boot time by the first core.
    fn startup_other_cores();

    /// Triggers the scheduler on core `id`.
    fn schedule_on_core(id: CoreId);

    /// Executes a function inside a critical section.
    ///
    /// Mutual exclusion with `critical_section::with` is not guaranteed.
    /// The implementation may use `critical_section::with`, but can also be
    /// independent.
    fn critical_section_with<R>(f: impl FnOnce(CriticalSection<'_>) -> R) -> R;
}

cfg_if::cfg_if! {
    if #[cfg(context = "rp2040")] {
        mod rp2040;
        pub use rp2040::Chip;
    } else if #[cfg(context = "esp32s3")] {
        mod esp32s3;
        pub use esp32s3::Chip;
    }
    else {
        use crate::{Arch as _, Cpu};

        pub struct Chip;
        impl Multicore for Chip {
            const CORES: u32 = 1;

            fn core_id() -> CoreId {
                CoreId(0)
            }

            fn startup_other_cores() {}

            fn schedule_on_core(_id: CoreId) {
                Cpu::schedule();
            }
            fn critical_section_with<R>(f: impl FnOnce(CriticalSection<'_>) -> R) -> R {
                critical_section::with(f)
            }
        }
    }
}

/// Executes a function inside a critical section.
///
/// Mutual exclusion with `critical_section::with` is not guaranteed.
/// The implementation may use `critical_section::with`, but can also be
/// independent.
pub fn critical_section_with<R>(f: impl FnOnce(CriticalSection<'_>) -> R) -> R {
    Chip::critical_section_with(f)
}

/// Triggers the scheduler on core `id`.
pub fn schedule_on_core(id: CoreId) {
    Chip::schedule_on_core(id)
}

/// Affinity mask that defines on what cores a thread can be scheduled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[cfg(feature = "core-affinity")]
pub struct CoreAffinity(u8);

#[cfg(feature = "core-affinity")]
impl CoreAffinity {
    /// Allows a thread to be scheduled on any core and to migrate
    /// from one core to another between executions.
    pub const fn no_affinity() -> Self {
        Self(2u8.pow(Chip::CORES) - 1)
    }

    /// Restricts the thread execution to a specific core.
    ///
    /// The thread can only be scheduled on this core, even
    /// if other cores are idle or execute a lower priority thread.
    #[cfg(feature = "core-affinity")]
    pub fn one(core: CoreId) -> Self {
        Self(1 << core.0)
    }

    /// Checks if the affinity mask "allows" this `core`.
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

/// Main stack size for the second core, that is also used by the ISR.
///
/// Uses default from `riot-rs-rt` if not specified.
/// The `CONFIG_ISR_STACKSIZE` env name and default is copied from
/// `riot-rs-rt`.
#[allow(dead_code, reason = "used in chip submodules")]
const ISR_STACKSIZE_CORE1: usize = usize_from_env_or!(
    "CONFIG_ISR_STACKSIZE_CORE1",
    usize_from_env_or!("CONFIG_ISR_STACKSIZE", 8192, "ISR stack size (in bytes)"),
    "Core 1 ISR stack size (in bytes)"
);
