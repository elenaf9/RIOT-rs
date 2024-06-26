use crate::arch::{Arch, Cpu};

use super::Multicore;
use embassy_rp::{
    multicore::{spawn_core1, Stack},
    peripherals::CORE1,
};
use rp_pac::SIO;
use static_cell::ConstStaticCell;

pub struct Chip;

impl Multicore for Chip {
    const CORES: u32 = 2;

    fn core_id() -> u32 {
        SIO.cpuid().read()
    }

    fn startup_cores() {
        static STACK: ConstStaticCell<Stack<4096>> = ConstStaticCell::new(Stack::new());
        let start_threading = move || {
            Cpu::start_threading();
            loop {}
        };
        unsafe {
            spawn_core1(CORE1::steal(), STACK.take(), start_threading);
        }
    }

    fn wait_for_wakeup() {
        cortex_m::asm::wfe()
    }

    fn sev() {
        cortex_m::asm::sev()
    }
}
