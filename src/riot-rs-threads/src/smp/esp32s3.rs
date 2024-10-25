use critical_section::CriticalSection;
use esp_hal::{
    cpu_control::{CpuControl, Stack},
    interrupt,
    peripherals::{Interrupt, CPU_CTRL, SYSTEM},
    Cpu,
};

use static_cell::ConstStaticCell;

use super::{CoreId, Multicore, ISR_STACKSIZE_CORE1};

impl From<Cpu> for CoreId {
    fn from(value: Cpu) -> Self {
        match value {
            Cpu::ProCpu => CoreId(0),
            Cpu::AppCpu => CoreId(1),
        }
    }
}

pub struct Chip;

impl Multicore for Chip {
    const CORES: u32 = 2;
    const IDLE_THREAD_STACK_SIZE: usize = 2048;

    fn core_id() -> CoreId {
        esp_hal::get_core().into()
    }

    fn startup_other_cores() {
        // TODO: How much stack do we really need here?
        static STACK: ConstStaticCell<Stack<ISR_STACKSIZE_CORE1>> =
            ConstStaticCell::new(Stack::new());
        // Trigger scheduler.
        let start_threading = move || {
            // Use `CPU_INTR1` to trigger the scheduler on our second core.
            // We need to use a different interrupt here than on the first core so that
            // we specifically trigger the scheduler on one or the other core.
            interrupt::disable(esp_hal::Cpu::ProCpu, Interrupt::FROM_CPU_INTR1);
            Self::schedule_on_core(Self::core_id());
            // Panics if `FROM_CPU_INTR1` is among `esp_hal::interrupt::RESERVED_INTERRUPTS`,
            // which isn't the case.
            interrupt::enable(Interrupt::FROM_CPU_INTR1, interrupt::Priority::min()).unwrap();

            unreachable!()
        };

        let mut cpu_ctrl = unsafe { CpuControl::new(CPU_CTRL::steal()) };
        let guard = cpu_ctrl.start_app_core(STACK.take(), start_threading);

        // Dropping the guard would park the other core.
        core::mem::forget(guard)
    }

    fn schedule_on_core(id: CoreId) {
        let ptr = unsafe { &*SYSTEM::PTR };
        let mut id = id.0;
        let already_set = match id {
            0 => ptr
                .cpu_intr_from_cpu_0()
                .read()
                .cpu_intr_from_cpu_0()
                .bit_is_set(),
            1 => ptr
                .cpu_intr_from_cpu_1()
                .read()
                .cpu_intr_from_cpu_1()
                .bit_is_set(),
            _ => unreachable!(),
        };
        if already_set {
            // If a scheduling attempt is already pending, there must have been multiple
            // changes in the runqueue at the same time.
            // Trigger the scheduler on the other core as well to make sure that both schedulers
            // have the most recent runqueue state.
            id ^= 1;
        }
        match id {
            0 => ptr
                .cpu_intr_from_cpu_0()
                .write(|w| w.cpu_intr_from_cpu_0().set_bit()),
            1 => ptr
                .cpu_intr_from_cpu_1()
                .write(|w| w.cpu_intr_from_cpu_1().set_bit()),
            _ => unreachable!(),
        };
    }

    fn critical_section_with<R>(f: impl FnOnce(CriticalSection<'_>) -> R) -> R {
        let _guard = internal_critical_section::acquire();
        unsafe { f(CriticalSection::new()) }
    }
}
mod internal_critical_section {
    use core::sync::atomic::{AtomicUsize, Ordering};

    static MULTICORE_LOCK: AtomicUsize = AtomicUsize::new(0);

    pub struct Guard {
        tkn: u32,
    }

    impl Guard {
        fn release(&mut self) {
            MULTICORE_LOCK.store(0, Ordering::Relaxed);
            // Ensure the compiler doesn't re-order accesses and violate safety here
            core::sync::atomic::compiler_fence(Ordering::Release);
            // Copied from `esp_hal::critical_section_impl::xtensa::release`
            const RESERVED_MASK: u32 = 0b1111_1111_1111_1000_1111_0000_0000_0000;
            debug_assert!(self.tkn & RESERVED_MASK == 0);
            unsafe {
                core::arch::asm!(
                    "wsr.ps {0}",
                    "rsync", in(reg) self.tkn)
            }
        }
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            self.release()
        }
    }

    pub fn acquire() -> Guard {
        let mut tkn: u32;
        unsafe {
            core::arch::asm!("rsil {0}, 5", out(reg) tkn);
        }
        // Ensure the compiler doesn't re-order accesses and violate safety here
        core::sync::atomic::compiler_fence(Ordering::Acquire);
        while MULTICORE_LOCK.swap(1, Ordering::Relaxed) > 0 {}

        Guard { tkn }
    }
}
