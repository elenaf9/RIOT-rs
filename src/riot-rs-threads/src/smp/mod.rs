use super::CoreId;

#[cfg(not(context = "rp2040"))]
use super::{Arch, Cpu};

pub trait Multicore {
    const CORES: u32;

    fn core_id() -> CoreId;

    fn startup_cores();

    fn wait_for_wakeup();

    fn sev();
}

cfg_if::cfg_if! {
    if #[cfg(context = "rp2040")] {
        mod rp2040;
        pub use rp2040::Chip;
    }
    else {

        pub struct Chip;
        impl Multicore for Chip {
            const CORES: u32 = 1;

            fn core_id() -> CoreId {
                CoreId::new(0)
            }

            fn startup_cores() {}

            fn wait_for_wakeup() {
                Cpu::wfi();
            }

            fn sev() {}
        }
    }
}

pub fn sev() {
    Chip::sev()
}
