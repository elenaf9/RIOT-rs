use critical_section::CriticalSection;

use crate::THREADS;
use crate::{Arch, Cpu};

/// Schedule the next thread.
///
/// It selects the next thread that should run from the runqueue.
/// This may be current thread, or a new one.
///
/// Input:
/// - old_sp: the stack pointer of the currently running thread.
///
/// Returns:
/// - `0` if the next thread in the runqueue is the currently running thread
/// - Else: the next sp, and pointers to the old and new thread data are written into arch-specific registers
#[no_mangle]
pub unsafe fn sched(old_sp: usize) -> usize {
    unsafe {
        let cs = CriticalSection::new();
        let next_pid;

        loop {
            {
                if let Some(pid) = (&*THREADS.as_ptr(cs)).runqueue.get_next() {
                    next_pid = pid;
                    break;
                }
            }
            Cpu::wfi();
        }

        let threads = &mut *THREADS.as_ptr(cs);
        let current_high_regs;

        if let Some(current_pid) = threads.current_pid() {
            if next_pid == current_pid {
                return 0;
            }
            //println!("current: {} next: {}", current_pid, next_pid);
            threads.threads[current_pid as usize].sp = old_sp;
            threads.current_thread = Some(next_pid);
            current_high_regs = Some(&threads.threads[current_pid as usize].data);
        } else {
            current_high_regs = None;
        }

        let next = &threads.threads[next_pid as usize];
        let next_sp = next.sp;
        let next_high_regs = &next.data;

        // Return pointers to the old and new thread data in arch specific registers
        // so that the exception handler save and restore the state.
        Cpu::return_data_in_regs(current_high_regs, next_high_regs);

        next_sp
    }
}
