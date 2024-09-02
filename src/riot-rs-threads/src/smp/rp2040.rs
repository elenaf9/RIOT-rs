use super::{CoreId, Multicore};
use crate::arch::{Arch, Cpu};

use critical_section::CriticalSection;
use embassy_rp::{
    multicore::{spawn_core1, Stack},
    peripherals::CORE1,
};
use rp_pac::{interrupt, SIO};

use spinlock::Spinlock;

pub struct Chip;

impl Multicore for Chip {
    const CORES: u32 = 2;
    const SPINLOCKS: u8 = 30;

    fn core_id() -> CoreId {
        CoreId(SIO.cpuid().read() as u8)
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

    fn cs_with<R>(id: usize, f: impl FnOnce(CriticalSection<'_>) -> R) -> R {
        let _lock = unsafe { Spinlock::acquire(id) };
        unsafe { f(CriticalSection::new()) }
    }

    fn no_preemption_with<R>(f: impl FnOnce() -> R) -> R {
        // Helper for making sure `release` is called even if `f` panics.
        struct Guard {
            interrupts_active: bool,
        }

        impl Drop for Guard {
            #[inline(always)]
            fn drop(&mut self) {
                if self.interrupts_active {
                    unsafe {
                        cortex_m::interrupt::enable();
                    }
                }
            }
        }

        let interrupts_active = cortex_m::register::primask::read().is_active();
        cortex_m::interrupt::disable();

        let _guard = Guard { interrupts_active };

        f()
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

mod spinlock {
    use core::sync::atomic::Ordering;

    use rp_pac::SIO;

    /// Hardware Spinlock.
    pub struct Spinlock {
        id: usize,
    }

    impl Spinlock {
        pub unsafe fn acquire(id: usize) -> Self {
            // Spin until we get the lock
            loop {
                // Ensure the compiler doesn't re-order accesses and violate safety here
                core::sync::atomic::fence(Ordering::SeqCst);
                // Read the spinlock reserved for the internal `critical_section`
                if SIO.spinlock(id).read() > 0 {
                    // We just acquired the lock.
                    break;
                }
            }
            // If we broke out of the loop we have just acquired the lock
            // We want to remember the interrupt status to restore later
            Self { id }
        }

        unsafe fn release(&self) {
            // Ensure the compiler doesn't re-order accesses and violate safety here
            core::sync::atomic::fence(Ordering::SeqCst);
            // Release the spinlock to allow others to enter critical_section again
            SIO.spinlock(self.id).write_value(1);
        }
    }

    impl Drop for Spinlock {
        fn drop(&mut self) {
            // This is safe because we own the object, and hence hold the lock.
            unsafe { self.release() }
        }
    }
}
