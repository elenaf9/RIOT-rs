use super::{CoreId, Multicore};
use crate::arch::{Arch, Cpu};

use internal_critical_section::SpinlockCS;

use critical_section::CriticalSection;
use embassy_rp::{
    multicore::{spawn_core1, Stack},
    peripherals::CORE1,
};
use rp_pac::{interrupt, SIO};

pub struct Chip;

impl Multicore for Chip {
    const CORES: u32 = 2;

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

    fn cs_with<R>(f: impl FnOnce(CriticalSection<'_>) -> R) -> R {
        let _lock = unsafe { SpinlockCS::acquire() };
        unsafe { f(CriticalSection::new()) }
    }

    fn no_preemption_with<R>(f: impl FnOnce() -> R) -> R {
        internal_preemption_lock::with(f)
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

mod internal_critical_section {
    use rp_pac::SIO;

    pub struct Spinlock<const N: usize> {
        token: u8,
    }

    impl<const N: usize> Spinlock<N> {
        pub unsafe fn acquire() -> Self {
            // Store the initial interrupt state and current core id in stack variables
            let interrupts_active = cortex_m::register::primask::read().is_active();
            // Spin until we get the lock
            loop {
                // Need to disable interrupts to ensure that we will not deadlock
                // if an interrupt enters critical_section::Impl after we acquire the lock
                cortex_m::interrupt::disable();
                // Read the spinlock reserved for the internal `critical_section`
                if SIO.spinlock(N).read() > 0 {
                    // We just acquired the lock.
                    break;
                }
                // We didn't get the lock, enable interrupts if they were enabled before we started
                if interrupts_active {
                    unsafe {
                        cortex_m::interrupt::enable();
                    }
                }
            }
            // If we broke out of the loop we have just acquired the lock
            // We want to remember the interrupt status to restore later
            Self {
                token: interrupts_active as u8,
            }
        }

        unsafe fn release(&mut self) {
            // Release the spinlock to allow others to enter critical_section again
            SIO.spinlock(N).write_value(1);
            // Re-enable interrupts if they were enabled when we first called acquire()
            if self.token != 0 {
                unsafe {
                    cortex_m::interrupt::enable();
                }
            }
        }
    }

    impl<const N: usize> Drop for Spinlock<N> {
        fn drop(&mut self) {
            // This is safe because we own the object, and hence hold the lock.
            unsafe { self.release() }
        }
    }

    pub type SpinlockCS = Spinlock<30>;
}

mod internal_preemption_lock {

    unsafe fn enable_interrupts(interrupts_active: bool) {
        if interrupts_active {
            unsafe {
                cortex_m::interrupt::enable();
            }
        }
    }

    unsafe fn disable_interrupts() -> bool {
        let interrupts_active = cortex_m::register::primask::read().is_active();
        if interrupts_active {
            cortex_m::interrupt::disable();
        }
        interrupts_active
    }

    pub fn with<R>(f: impl FnOnce() -> R) -> R {
        // Helper for making sure `release` is called even if `f` panics.
        struct Guard {
            interrupts_enabled: bool,
        }

        impl Drop for Guard {
            #[inline(always)]
            fn drop(&mut self) {
                unsafe { enable_interrupts(self.interrupts_enabled) }
            }
        }

        let interrupts_enabled = unsafe { disable_interrupts() };
        let _guard = Guard { interrupts_enabled };

        f()
    }
}
