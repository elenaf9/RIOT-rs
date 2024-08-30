use crate::arch::{Arch as _, Cpu};

use internal_critical_section::SpinlockCS;

use cortex_m::peripheral::SCB;
use critical_section::CriticalSection;
use embassy_rp::{
    interrupt,
    interrupt::InterruptExt as _,
    multicore::{spawn_core1, Stack},
    peripherals::CORE1,
};
use rp_pac::SIO;
use static_cell::ConstStaticCell;

use super::{CoreId, Multicore, ISR_STACKSIZE_CORE1};

pub struct Chip;

impl Multicore for Chip {
    const CORES: u32 = 2;

    fn core_id() -> CoreId {
        CoreId(SIO.cpuid().read() as u8)
    }

    fn startup_other_cores() {
        // TODO: How much stack do we really need here?
        static STACK: ConstStaticCell<Stack<ISR_STACKSIZE_CORE1>> =
            ConstStaticCell::new(Stack::new());
        // Trigger scheduler.
        let start_threading = move || {
            unsafe {
                interrupt::SIO_IRQ_PROC1.enable();
            }
            Cpu::start_threading();
            unreachable!()
        };
        unsafe {
            spawn_core1(CORE1::steal(), STACK.take(), start_threading);
            interrupt::SIO_IRQ_PROC0.enable();
        }
    }

    fn schedule_on_core(id: CoreId) {
        if id == Self::core_id() {
            schedule();
        } else {
            schedule_other_core();
        }
    }

    fn no_preemption_with<R>(f: impl FnOnce() -> R) -> R {
        cortex_m::interrupt::free(|_| f())
    }

    fn multicore_lock_with<R>(f: impl FnOnce(CriticalSection<'_>) -> R) -> R {
        let _lock = SpinlockCS::acquire();
        unsafe { f(CriticalSection::new()) }
    }
}

fn schedule() {
    if SCB::is_pendsv_pending() {
        // If a scheduling attempt is already pending, there must have been multiple
        // changes in the runqueue at the same time.
        // Trigger the scheduler on the other core as well to make sure that both schedulers
        // have the most recent runqueue state.
        return schedule_other_core();
    }
    crate::schedule()
}

fn schedule_other_core() {
    // Use the FIFO queue between the cores to trigger the scheduler
    // on the other core.
    let sio = SIO;
    // If its already full, no need to send another `SCHEDULE_TOKEN`.
    if !sio.fifo().st().read().rdy() {
        return;
    }
    sio.fifo().wr().write_value(SCHEDULE_TOKEN);
}

const SCHEDULE_TOKEN: u32 = 0x11;

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

mod internal_critical_section {
    use core::sync::atomic::{compiler_fence, Ordering};

    use rp_pac::SIO;

    pub struct Spinlock<const N: usize>;

    impl<const N: usize> Spinlock<N> {
        pub fn acquire() -> Self {
            // Ensure the compiler doesn't re-order accesses and violate safety here
            compiler_fence(Ordering::Acquire);
            // Spin until we get the lock.
            while SIO.spinlock(N).read() == 0 {}
            // If we broke out of the loop we have just acquired the lock
            // We want to remember the interrupt status to restore later
            Self
        }

        fn release(&mut self) {
            // Release the spinlock to allow others to enter critical_section again
            SIO.spinlock(N).write_value(1);
            // Ensure the compiler doesn't re-order accesses and violate safety here
            compiler_fence(Ordering::Release);
        }
    }

    impl<const N: usize> Drop for Spinlock<N> {
        fn drop(&mut self) {
            self.release()
        }
    }

    pub type SpinlockCS = Spinlock<30>;
}
