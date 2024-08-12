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

pub trait Multicore {
    const CORES: u32;

    fn core_id() -> CoreId;

    fn startup_cores();

    fn wait_for_wakeup();

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

            fn startup_cores() {}

            fn wait_for_wakeup() {
                Cpu::wfi();
            }

            fn schedule_on_core(_id: CoreId) {}
        }
    }
}

pub fn schedule_on_core(id: CoreId) {
    Chip::schedule_on_core(id)
}
