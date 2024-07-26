use core::arch::asm;
use core::ptr::write_volatile;
use cortex_m::peripheral::{scb::SystemHandler, SCB};

use crate::{cleanup, smp::Multicore, Arch, Thread, THREADS};

#[cfg(not(any(armv6m, armv7m, armv8m)))]
compile_error!("no supported ARM variant selected");

pub struct Cpu;

impl Arch for Cpu {
    /// Callee-save registers.
    type ThreadData = [usize; 8];

    const DEFAULT_THREAD_DATA: Self::ThreadData = [0; 8];

    /// The exact order in which Cortex-M pushes the registers to the stack when
    /// entering the ISR is:
    ///
    /// +---------+ <- sp
    /// |   r0    |
    /// |   r1    |
    /// |   r2    |
    /// |   r3    |
    /// |   r12   |
    /// |   LR    |
    /// |   PC    |
    /// |   PSR   |
    /// +---------+
    fn setup_stack(thread: &mut Thread, stack: &mut [u8], func: usize, arg: usize) {
        let stack_start = stack.as_ptr() as usize;

        // 1. The stack starts at the highest address and grows downwards.
        // 2. A full stored context also contains R4-R11 and the stack pointer,
        //    thus an additional 60 bytes need to be reserved.
        // 3. Cortex-M expects the SP to be 8 byte aligned, so we chop the lowest
        //    7 bits by doing `& 0xFFFFFFF8`.
        let stack_pos = ((stack_start + stack.len() - 60) & 0xFFFFFFF8) as *mut usize;

        unsafe {
            write_volatile(stack_pos.offset(8), arg); // -> R0
            write_volatile(stack_pos.offset(9), 1); // -> R1
            write_volatile(stack_pos.offset(10), 2); // -> R2
            write_volatile(stack_pos.offset(11), 3); // -> R3
            write_volatile(stack_pos.offset(12), 12); // -> R12
            write_volatile(stack_pos.offset(13), cleanup as usize); // -> LR
            write_volatile(stack_pos.offset(14), func); // -> PC
            write_volatile(stack_pos.offset(15), 0x01000000); // -> APSR
        }

        thread.sp = stack_pos as usize;
    }

    /// Triggers a PendSV exception.
    #[inline(always)]
    fn schedule() {
        SCB::set_pendsv();
        cortex_m::asm::isb();
    }

    #[inline(always)]
    fn start_threading() {
        unsafe {
            // Make sure PendSV has a low priority.
            let mut p = cortex_m::Peripherals::steal();
            p.SCB.set_priority(SystemHandler::PendSV, 0xFF);
            cortex_m::register::psp::write(0);
        }
        Self::schedule();
    }

    fn wfi() {
        cortex_m::asm::wfi();

        // see https://cliffle.com/blog/stm32-wfi-bug/
        #[cfg(context = "stm32")]
        cortex_m::asm::isb();
    }
}

#[cfg(any(armv7m, armv8m))]
#[naked]
#[no_mangle]
#[allow(non_snake_case)]
unsafe extern "C" fn PendSV() {
    unsafe {
        asm!(
            "
            mrs.n r0, psp
            
            cmp r0, #0
            beq 95f

            stmfd r0!, {{r4-r11}}
            msr.n psp, r0

            95:
            bl {sched}

            cmp r0, #0
            beq 98f

            ldmfd r0!, {{r4-r11}}
            msr.n psp, r0
            b 99f


            98:
            mrs.n r0, psp
            adds r0, 32
            msr.n psp, r0

            99:
            movw LR, #0xFFFd
            movt LR, #0xFFFF
            bx LR
            ",
            sched = sym sched,
            options(noreturn)
        )
    };
}

#[cfg(any(armv6m))]
#[naked]
#[no_mangle]
#[allow(non_snake_case)]
unsafe extern "C" fn PendSV() {
    unsafe {
        asm!(
            "
            mrs.n  r0, psp
            
            cmp r0, #0
            beq 95f

            mrs.n r0, psp
            subs r0, r0, 32
            msr.n psp, r0

            stmea r0!, {{r4-r7}}
            mov r8,  r4
            mov r9,  r5
            mov r10, r6
            mov r11, r7
            stmea r0!, {{r4-r7}}

            95:
            bl sched

            cmp r0, #0
            beq 98f

            ldmfd r0!, {{r4-r7}}
            mov r8,  r4
            mov r9,  r5
            mov r10, r6
            mov r11, r7
            ldmfd r0!, {{r4-r7}}

            msr.n psp, r0
            b 99f

            98:
            mrs.n r0, psp
            adds r0, 32
            msr.n psp, r0

            99:
            ldr r0, 999f
            mov LR, r0
            bx lr

            .align 4
            999:
            .word 0xFFFFFFFD
            ",
            options(noreturn)
        )
    };
}

/// Schedule the next thread.
///
/// It selects the next thread that should run from the runqueue.
/// This may be current thread, or a new one.
///
/// Returns:
/// - `r0`: 0 if the next thread in the runqueue is the currently running thread, else the SP of the next thread.
///
/// This function is called in PendSV.
#[no_mangle]
unsafe fn sched() -> usize {
    loop {
        if let Some(res) = THREADS.with_mut(|mut threads| {
            let next_pid = threads.runqueue.pop_next()?;

            if let Some(current_pid) = threads.current_pid() {
                if next_pid == current_pid {
                    return Some(0);
                }

                threads.threads[usize::from(current_pid)].sp =
                    cortex_m::register::psp::read() as usize;
            }

            *threads.current_pid_mut() = Some(next_pid);

            let next = &threads.threads[usize::from(next_pid)];
            let next_sp = next.sp as usize;
            Some(next_sp)
        }) {
            break res;
        }
        crate::smp::Chip::wait_for_wakeup();
    }
}
