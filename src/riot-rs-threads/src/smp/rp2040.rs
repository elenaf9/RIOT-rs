use crate::{
    arch::{Arch, Cpu},
    CoreId,
};

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

    fn core_id() -> CoreId {
        CoreId::new(SIO.cpuid().read() as u8)
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
