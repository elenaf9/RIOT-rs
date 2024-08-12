use crate::{
    arch::{Arch, Cpu},
    CoreId,
};

use super::Multicore;
use embassy_rp::{
    interrupt,
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

    fn schedule_on_core(id: CoreId) {
        if id == Self::core_id() {
            crate::schedule();
            return;
        }
        let sio = SIO;
        // We only use the FIFO queue to trigger the scheduler.
        // If its already full, no need to send another `SCHEDULE_TOKEN`.
        if !sio.fifo().st().read().rdy() {
            return;
        }
        sio.fifo().wr().write_value(SCHEDULE_TOKEN);
        // Wake up other core if it `WFE`s.
        cortex_m::asm::sev();
    }
}

const SCHEDULE_TOKEN: u32 = 0x111;

#[interrupt]
unsafe fn SIO_IRQ_PROC0() {
    handle_fifo_msg();
}

#[interrupt]
unsafe fn SIO_IRQ_PROC1() {
    handle_fifo_msg();
}

fn handle_fifo_msg() {
    let sio = SIO;
    // Clear IRQ
    sio.fifo().st().write(|w| w.set_wof(false));

    while sio.fifo().st().read().vld() {
        if sio.fifo().rd().read() == SCHEDULE_TOKEN {
            crate::schedule();
        }
    }
}
