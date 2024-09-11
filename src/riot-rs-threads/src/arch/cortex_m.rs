use super::Arch;
use crate::{CoreId, Thread};
use core::arch::asm;
use core::ptr::write_volatile;
use cortex_m::peripheral::SCB;

use crate::{cleanup, THREADS};

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
        //    thus an additional 36 bytes need to be reserved.
        // 3. Cortex-M expects the SP to be 8 byte aligned, so we chop the lowest
        //    7 bits by doing `& 0xFFFFFFF8`.
        let stack_pos = ((stack_start + stack.len() - 36) & 0xFFFFFFF8) as *mut usize;

        unsafe {
            write_volatile(stack_pos.offset(0), arg); // -> R0
            write_volatile(stack_pos.offset(1), 1); // -> R1
            write_volatile(stack_pos.offset(2), 2); // -> R2
            write_volatile(stack_pos.offset(3), 3); // -> R3
            write_volatile(stack_pos.offset(4), 12); // -> R12
            write_volatile(stack_pos.offset(5), cleanup as usize); // -> LR
            write_volatile(stack_pos.offset(6), func); // -> PC
            write_volatile(stack_pos.offset(7), 0x01000000); // -> APSR
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
        Self::schedule();
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
            bl {sched}
            cmp r0, #0
            /* label rules:
             * - number only
             * - no combination of *only* [01]
             * - add f or b for 'next matching forward/backward'
             * so let's use '99' forward ('99f')
             */
            beq 99f
            stmia r1, {{r4-r11}}
            ldmia r2, {{r4-r11}}
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
            bl sched
            cmp r0, #0
            beq 99f

            //stmia r1!, {{r4-r7}}
            str r4, [r1, #16]
            str r5, [r1, #20]
            str r6, [r1, #24]
            str r7, [r1, #28]

            mov  r4, r8
            mov  r5, r9
            mov  r6, r10
            mov  r7, r11

            str r4, [r1, #0]
            str r5, [r1, #4]
            str r6, [r1, #8]
            str r7, [r1, #12]

            //
            ldmia r2!, {{r4-r7}}
            mov r11, r7
            mov r10, r6
            mov r9,  r5
            mov r8,  r4
            ldmia r2!, {{r4-r7}}

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
/// - `0` in `r0` if the next thread in the runqueue is the currently running thread
/// - Else it writes into the following registers:
///   - `r1`: pointer to [`Thread::high_regs`] from old thread (to store old register state)
///   - `r2`: pointer to [`Thread::high_regs`] from new thread (to load new register state)
///   - `r0`: stack-pointer for new thread
///
/// This function is called in PendSV.
// TODO: make arch independent, or move to arch
#[no_mangle]
unsafe fn sched() -> u128 {
    let core = CoreId::new(0);
    loop {
        if let Some(res) = critical_section::with(|cs| {
            let threads = unsafe { &mut *THREADS.as_ptr(cs) };
            let next_pid = match threads.runqueue.get_next(core) {
                Some(pid) => pid,
                None => {
                    cortex_m::asm::wfi();

                    // see https://cliffle.com/blog/stm32-wfi-bug/
                    #[cfg(context = "stm32")]
                    cortex_m::asm::isb();

                    // this fence seems necessary, see #310.
                    core::sync::atomic::fence(core::sync::atomic::Ordering::Acquire);
                    return None;
                }
            };

            let current_high_regs;
            if let Some(current_pid) = threads.current_pid() {
                if next_pid == current_pid {
                    return Some(0);
                }

                threads.threads[usize::from(current_pid)].sp =
                    cortex_m::register::psp::read() as usize;
                threads.current_thread = Some(next_pid);

                current_high_regs = threads.threads[usize::from(current_pid)].data.as_ptr();
            } else {
                threads.current_thread = Some(next_pid);
                current_high_regs = core::ptr::null();
            };

            let next = &threads.threads[usize::from(next_pid)];
            let next_sp = next.sp as usize;
            let next_high_regs = next.data.as_ptr() as usize;

            // PendSV expects these three pointers in r0, r1 and r2:
            // r0 = &next.sp
            // r1 = &current.high_regs
            // r2 = &next.high_regs
            // On Cortex-M, a u128 as return value is passed in registers r0-r3.
            // So let's use that.
            let res: u128 =
                //  (r0)                     (r1)                        (r2)
                (next_sp as u128) |  ((current_high_regs as u128) << 32) | ((next_high_regs as u128) << 64);
            Some(res)
        }) {
            break res;
        }
    }
}
