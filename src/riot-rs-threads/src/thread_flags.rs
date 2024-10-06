//! Thread flags.
use crate::{ThreadId, ThreadState, Threads, THREADS};

/// Bitmask that represent the flags that are set for a thread.
pub type ThreadFlags = u16;

/// Possible waiting modes for [`ThreadFlags`].
#[derive(Copy, Clone, PartialEq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum WaitMode {
    Any(ThreadFlags),
    All(ThreadFlags),
}

/// Sets flags for a thread.
///
/// If the thread was blocked on these flags it's unblocked and added
/// to the runqueue.
///
/// # Panics
///
/// Panics if `thread_id` is >= [`THREADS_NUMOF`](crate::THREADS_NUMOF).
pub fn set(thread_id: ThreadId, mask: ThreadFlags) {
    THREADS.with(|threads| threads.flag_set(thread_id, mask))
}

/// Waits until all flags in `mask` are set for the current thread.
///
/// Returns the set flags for this mask and clears them for the thread.
///
/// # Panics
///
/// Panics if this is called outside of a thread context.
pub fn wait_all(mask: ThreadFlags) -> ThreadFlags {
    loop {
        if let Some(flags) = THREADS.with(|threads| threads.flag_wait_all(mask)) {
            return flags;
        }
    }
}

/// Waits until any flag in `mask` is set for the current thread.
///
/// Returns all set flags for this mask and clears them for the thread.
///
/// # Panics
///
/// Panics if this is called outside of a thread context.
pub fn wait_any(mask: ThreadFlags) -> ThreadFlags {
    loop {
        if let Some(flags) = THREADS.with(|threads| threads.flag_wait_any(mask)) {
            return flags;
        }
    }
}

/// Waits until any flag in `mask` is set for the current thread.
///
/// Compared to [`wait_any`], this returns and clears only one flag
/// from the mask.
///
/// # Panics
///
/// Panics if this is called outside of a thread context.
pub fn wait_one(mask: ThreadFlags) -> ThreadFlags {
    loop {
        if let Some(flags) = THREADS.with(|threads| threads.flag_wait_one(mask)) {
            return flags;
        }
    }
}

/// Clears flags for the current thread.
///
/// # Panics
///
/// Panics if this is called outside of a thread context.
pub fn clear(mask: ThreadFlags) -> ThreadFlags {
    THREADS.with(|threads| {
        let thread_id = threads.current_threads().current_pid().unwrap();
        let mut tcbs = threads.tcbs_mut();
        let flags = &mut tcbs.get_unchecked_mut(thread_id).flags;
        let res = *flags & mask;
        *flags &= !mask;
        res
    })
}

/// Returns the flags set for the current thread.
///
/// # Panics
///
/// Panics if this is called outside of a thread context.
pub fn get() -> ThreadFlags {
    // TODO: current() requires us to use mutable `threads` here
    THREADS.with(|threads| {
        let thread_id = threads.current_threads().current_pid().unwrap();
        threads.tcbs().get_unchecked(thread_id).flags
    })
}

impl Threads {
    // thread flags implementation
    fn flag_set(&mut self, thread_id: ThreadId, mask: ThreadFlags) {
        let mut tcbs = self.tcbs_mut();
        let thread = tcbs.get_unchecked_mut(thread_id);
        thread.flags |= mask;
        match thread.state {
            ThreadState::FlagBlocked(WaitMode::Any(bits)) if thread.flags & bits != 0 => {}
            ThreadState::FlagBlocked(WaitMode::All(bits)) if thread.flags & bits == bits => {}
            _ => return,
        };
        tcbs.release();
        self.set_state(thread_id, ThreadState::Running);
    }

    fn flag_wait<F>(&mut self, cond: F, mode: WaitMode) -> Option<ThreadFlags>
    where
        F: Fn(u16) -> Option<u16>,
    {
        let thread_id = self.current_threads().current_pid().unwrap();
        let mut tcbs = self.tcbs_mut();
        let flags = &mut tcbs.get_unchecked_mut(thread_id).flags;
        match cond(*flags) {
            Some(res) => {
                *flags &= !res;
                Some(res)
            }
            None => {
                tcbs.release();
                self.set_state(thread_id, ThreadState::FlagBlocked(mode));
                None
            }
        }
    }

    fn flag_wait_all(&mut self, mask: ThreadFlags) -> Option<ThreadFlags> {
        self.flag_wait(
            |thread_flags| {
                let res = thread_flags & mask;
                (res == mask).then(|| mask)
            },
            WaitMode::All(mask),
        )
    }

    fn flag_wait_any(&mut self, mask: ThreadFlags) -> Option<ThreadFlags> {
        self.flag_wait(
            |thread_flags| {
                let res = thread_flags & mask;
                (res != 0).then(|| res)
            },
            WaitMode::Any(mask),
        )
    }

    fn flag_wait_one(&mut self, mask: ThreadFlags) -> Option<ThreadFlags> {
        self.flag_wait(
            |thread_flags| {
                let res = thread_flags & mask;
                (res != 0).then(|| res & (!res + 1))
            },
            WaitMode::Any(mask),
        )
    }
}
