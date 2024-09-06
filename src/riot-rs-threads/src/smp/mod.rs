use crate::CORES_NUMOF;

/// ID of a physical core.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct CoreId(pub(crate) u8);

impl CoreId {
    /// Creates a new [`CoreId`].
    ///
    /// # Panics
    ///
    /// Panics if `value` >= [`Chip::CORES`].
    pub fn new(value: u8) -> Self {
        if value >= Chip::CORES as u8 {
            panic!(
                "Invalid CoreId {}: only {} cores available.",
                value, CORES_NUMOF
            )
        }
        Self(value)
    }
}

impl From<CoreId> for usize {
    fn from(value: CoreId) -> Self {
        value.0 as usize
    }
}

pub trait Multicore {
    /// Number of available core.
    const CORES: u32;
    /// Stack size for the idle threads.
    const IDLE_THREAD_STACK_SIZE: usize = 256;

    /// Id of the current core.
    fn core_id() -> CoreId;

    /// Start other available cores so that the scheduler
    /// is triggered there
    fn startup_other_cores();

    /// Trigger the scheduler on core `id`.
    fn schedule_on_core(id: CoreId);
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

            fn core_id() -> CoreId {
                CoreId(0)
            }

            fn startup_other_cores() {}

            fn schedule_on_core(_id: CoreId) {
                Cpu::schedule();
            }
        }
    }
}

/// Trigger the scheduler on core `id`.
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
    /// The thread may run on any core.
    pub const fn no_affinity() -> Self {
        Self(2u8.pow(Chip::CORES) - 1)
    }

    /// The thread can only run on one core.
    pub fn one(core: CoreId) -> Self {
        Self(1 << core.0)
    }

    /// Check if the affinity mask "allows" this `core`.
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
