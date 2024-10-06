use crate::arch::{Arch as _, Cpu};

pub use internal_cs::Spinlock;

use cortex_m::peripheral::SCB;
use embassy_rp::{
    interrupt,
    interrupt::InterruptExt as _,
    multicore::{spawn_core1, Stack},
    peripherals::CORE1,
};
use rp_pac::SIO;
use static_cell::ConstStaticCell;

use super::{CoreId, Multicore};

pub struct Chip;

impl Multicore for Chip {
    const CORES: u32 = 2;

    type LockRestoreState = ();

    fn core_id() -> CoreId {
        CoreId(SIO.cpuid().read() as u8)
    }

    fn startup_other_cores() {
        // TODO: How much stack do we really need here?
        static STACK: ConstStaticCell<Stack<4096>> = ConstStaticCell::new(Stack::new());
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
        internal_preemption_lock::with(|| f())
    }

    fn multicore_lock_acquire<const N: usize>() -> Self::LockRestoreState {
        unsafe { Spinlock::<N>::acquire() }
    }

    fn multicore_lock_release<const N: usize>(_: Self::LockRestoreState) {
        unsafe { Spinlock::<N>::release() }
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

// Handles FIFO message on core 0 from core 1.
#[interrupt]
unsafe fn SIO_IRQ_PROC0() {
    handle_fifo_msg();
}

// Handles FIFO message on core 1 from core 0.
#[interrupt]
unsafe fn SIO_IRQ_PROC1() {
    handle_fifo_msg();
}

/// Reads FIFO message from other core and triggers scheduler
/// if a [`SCHEDULE_TOKEN`] was received.
fn handle_fifo_msg() {
    let sio = SIO;
    // Clear IRQ
    sio.fifo().st().write(|w| w.set_wof(false));

    while sio.fifo().st().read().vld() {
        if sio.fifo().rd().read() == SCHEDULE_TOKEN {
            schedule();
        }
    }
}

mod internal_cs {
    use critical_section::CriticalSection;
    use rp_pac::SIO;

    pub struct Spinlock<const N: usize>;

    impl<const N: usize> Spinlock<N> {
        pub unsafe fn acquire() {
            // Spin until we get the lock
            unsafe { while !Self::try_acquire() {} }
            // If we broke out of the loop we have just acquired the lock
        }

        pub unsafe fn try_acquire() -> bool {
            SIO.spinlock(N).read() != 0
        }

        pub unsafe fn release() {
            // Release the spinlock to allow others to enter critical_section again
            SIO.spinlock(N).write_value(1);
        }
    }

    impl<const N: usize> Drop for Spinlock<N> {
        fn drop(&mut self) {
            // This is safe because we own the object, and hence hold the lock.
            unsafe { Self::release() }
        }
    }

    #[allow(unused)]
    pub fn with<R>(f: impl FnOnce(CriticalSection<'_>) -> R) -> R {
        unsafe { SpinlockCS::acquire() };
        let _lock = SpinlockCS {};
        unsafe { f(CriticalSection::new()) }
    }

    #[allow(unused)]
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
