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

pub struct Chip;

impl Multicore for Chip {
    const CORES: u32 = 2;

    fn cpuid() -> CoreId {
        SIO.cpuid().read() as CoreId
    }

    fn startup_cores() {
        let stack: &'static mut Stack<4096> = static_cell::make_static!(Stack::new());
        let start_threading = move || {
            Cpu::start_threading();
            loop {}
        };
        unsafe {
            spawn_core1(CORE1::steal(), stack, start_threading);
        }
    }
}
