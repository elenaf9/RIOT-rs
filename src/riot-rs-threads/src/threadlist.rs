use critical_section::CriticalSection;

use crate::{ThreadId, ThreadState, THREADS};

/// Manages blocked [`super::Thread`]s for a resource, and triggering the scheduler when needed.
#[derive(Debug, Default)]
pub struct ThreadList {
    /// Next thread to run once the resource is available.
    pub head: Option<ThreadId>,
}

impl ThreadList {
    /// Creates a new empty [`ThreadList`]
    pub const fn new() -> Self {
        Self { head: None }
    }

    /// Puts the current (blocked) thread into this [`ThreadList`] and triggers the scheduler.
    pub fn put_current(&mut self, cs: CriticalSection, state: ThreadState) {
        THREADS.with_mut_cs(cs, |mut threads| {
            let thread_id = threads.current_pid().unwrap();
            threads.thread_blocklist[usize::from(thread_id)] = self.head;
            self.head = Some(thread_id);
            threads.set_state(thread_id, state);
        });
    }

    /// Removes the head from this [`ThreadList`].
    ///
    /// Sets the thread's [`ThreadState`] to [`ThreadState::Running`] and triggers
    /// the scheduler.
    ///
    /// Returns the thread's [`ThreadId`] and its previous [`ThreadState`].
    pub fn pop(&mut self, cs: CriticalSection) -> Option<(ThreadId, ThreadState)> {
        let head = self.head?;
        THREADS.with_mut_cs(cs, |mut threads| {
            self.head = threads.thread_blocklist[usize::from(head)].take();
            let old_state = threads.set_state(head, ThreadState::Running);
            Some((head, old_state))
        })
    }

    /// Determines if this [`ThreadList`] is empty.
    pub fn is_empty(&self, _cs: CriticalSection) -> bool {
        self.head.is_none()
    }
}
