use core::arch::asm;
use core::ptr::write_volatile;
use cortex_m::peripheral::{scb::SystemHandler, SCB};

use crate::{cleanup, smp::Multicore, Arch, Thread, ThreadState, THREADS};

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
        //    thus an additional 64 bytes need to be reserved.
        // 3. Cortex-M expects the SP to be 8 byte aligned, so we chop the lowest
        //    7 bits by doing `& 0xFFFFFFF8`.
        let stack_pos = ((stack_start + stack.len() - 64) & 0xFFFFFFF8) as *mut usize;

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
            it ne
            stmfdne r0!, {{r4-r11}}

            bl {sched}

            cmp r0, #0
            beq 99f

            ldmfd r0!, {{r4-r11}}
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
            mrs.n r0, psp

            cmp r0, #0
            beq 95f


            // The ldm section that restores the registers expects the stack:
            // | r8 - r11 | <- r0
            // | r4 - r7  |
            // |   ...    |
            // On ARMv6 without thumb instructions, `stmfd` is not supported,
            // and only registers r1-r7 can be pushed.
            // stmia is pushing in ascending manner.
            // So we first need to subtract the space for the 8 stored registers,
            // and then push the registers in the correct order.
            subs r0, 32
            mov r1,  r8
            mov r2,  r9
            stmia r0!, {{r1-r2}}
            mov r1,  r10
            mov r2,  r11
            stmia r0!, {{r1-r2}}
            stmia r0!, {{r4-r7}}
            
            // Move pointer back to bottom of stack.
            subs r0, r0, 32

            95:
            bl sched

            cmp r0, #0
            beq 99f

            ldmfd r0!, {{r4-r7}}
            mov r11, r7
            mov r10, r6
            mov r9,  r5
            mov r8,  r4
            ldmfd r0!, {{r4-r7}}

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
unsafe fn sched(old_sp: u32) -> u32 {
    THREADS.with_mut(|mut threads| {
        if let Some(current_pid) = threads.current_pid() {
            let thread = threads.get_unchecked_mut(current_pid);
            thread.sp = old_sp as usize;
            if thread.state == ThreadState::Running {
                let prio = thread.prio;
                threads.runqueue.add(current_pid, prio);
            }
        }
    });
    loop {
        if let Some(res) = THREADS.with_mut(|mut threads| {
            #[cfg(not(feature = "core-affinity"))]
            let next_pid = threads.runqueue.pop_next()?;
            #[cfg(feature = "core-affinity")]
            let next_pid = {
                let (mut next, prio) = threads.runqueue.peek_next()?;
                if !threads.is_affine_to_curr_core(next) {
                    let iter = threads.runqueue.iter_from(next, prio);
                    next = iter
                        .filter(|pid| threads.is_affine_to_curr_core(*pid))
                        .next()?;
                }
                threads.runqueue.del(next);
                next
            };
            if Some(next_pid) == threads.current_pid() {
                return Some(0);
            }
            let &Thread { prio, sp, .. } = threads.get_unchecked(next_pid);
            threads.set_current(next_pid, prio);

            Some(sp as u32)
        }) {
            break res;
        }
        crate::smp::Chip::wait_for_wakeup();
    }
}
