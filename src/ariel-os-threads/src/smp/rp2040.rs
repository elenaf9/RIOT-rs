use embassy_rp::{
    interrupt,
    interrupt::InterruptExt as _,
    multicore::{spawn_core1, Stack},
    peripherals::CORE1,
};
use rp_pac::SIO;
use static_cell::ConstStaticCell;

use super::ISR_STACKSIZE_CORE1;
use crate::{
    arch::{Arch, Cpu},
    CoreId, Multicore,
};

pub struct Chip;

impl Multicore for Chip {
    const CORES: u32 = 2;

    fn core_id() -> CoreId {
        CoreId::new(SIO.cpuid().read() as u8)
    }

    fn startup_cores() {
        // TODO: How much stack do we really need here?
        static STACK: ConstStaticCell<Stack<ISR_STACKSIZE_CORE1>> =
            ConstStaticCell::new(Stack::new());
        // Trigger scheduler.
        let start_threading = move || {
            unsafe {
                interrupt::SIO_IRQ_PROC1.enable();
            }
            Cpu::start_threading();
            loop {}
        };
        unsafe {
            spawn_core1(CORE1::steal(), STACK.take(), start_threading);
            interrupt::SIO_IRQ_PROC0.enable();
        }
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

/// Handles FIFO message from other core and triggers scheduler
/// if a [`SCHEDULE_TOKEN`] was received.
///
/// This method is injected into the `embassy_rp` interrupt handler
/// for FIFO messages.
#[no_mangle]
#[link_section = ".data.ram_func"]
#[inline]
fn handle_fifo_token(token: u32) -> bool {
    if token != SCHEDULE_TOKEN {
        return false;
    }
    crate::schedule();
    true
}
