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
        threads.current_with(|thread| {
            let thread = thread.unwrap();
            let res = thread.flags & mask;
            thread.flags &= !mask;
            res
        })
    })
}

/// Returns the flags set for the current thread.
///
/// # Panics
///
/// Panics if this is called outside of a thread context.
pub fn get() -> ThreadFlags {
    // TODO: current() requires us to use mutable `threads` here
    THREADS.with(|threads| threads.current_with(|t| t.unwrap().flags))
}

impl Threads {
    // thread flags implementation
    fn flag_set(&self, thread_id: ThreadId, mask: ThreadFlags) {
        self.get_unchecked_with(thread_id, |thread| {
            thread.flags |= mask;
            match thread.state {
                ThreadState::FlagBlocked(WaitMode::Any(bits)) if thread.flags & bits != 0 => {}
                ThreadState::FlagBlocked(WaitMode::All(bits)) if thread.flags & bits == bits => {}
                _ => return,
            };
        });
        self.set_state(thread_id, ThreadState::Running);
    }

    fn flag_wait<F>(&self, cond: F, mode: WaitMode) -> Option<ThreadFlags>
    where
        F: FnOnce(u16) -> Option<u16>,
    {
        let (res, pid) = self.current_with(|thread| {
            let thread = thread.unwrap();
            let res = cond(thread.flags);
            if let Some(res) = res {
                thread.flags &= !res;
            }
            (res, thread.pid)
        });
        if res.is_none() {
            self.set_state(pid, ThreadState::FlagBlocked(mode));
        }
        res
    }

    fn flag_wait_all(&self, mask: ThreadFlags) -> Option<ThreadFlags> {
        self.flag_wait(
            |thread_flags| {
                let res = thread_flags & mask;
                (res == mask).then(|| mask)
            },
            WaitMode::All(mask),
        )
    }

    fn flag_wait_any(&self, mask: ThreadFlags) -> Option<ThreadFlags> {
        self.flag_wait(
            |thread_flags| {
                let res = thread_flags & mask;
                (res != 0).then(|| res)
            },
            WaitMode::Any(mask),
        )
    }

    fn flag_wait_one(&self, mask: ThreadFlags) -> Option<ThreadFlags> {
        self.flag_wait(
            |thread_flags| {
                let res = thread_flags & mask;
                (res != 0).then(|| res & (!res + 1))
            },
            WaitMode::Any(mask),
        )
    }
}
