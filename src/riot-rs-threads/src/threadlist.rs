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
    ///
    /// # Panics
    ///
    /// Panics if this is called outside of a thread context.
    pub fn put_current(&mut self, cs: CriticalSection, state: ThreadState) {
        THREADS.with_cs(cs, |threads| {
            let pid = threads
                .current_pid()
                .expect("Function should be called inside a thread context.");
            let prio = threads.get_priority(pid);
            let mut curr = None;
            let mut next = self.head;
            let mut thread_blocklist = threads.thread_blocklist();
            while let Some(n) = next {
                if threads.get_priority(n) < prio {
                    break;
                }
                curr = next;
                next = thread_blocklist[usize::from(n)];
            }
            thread_blocklist[usize::from(pid)] = next;
            match curr {
                Some(curr) => thread_blocklist[usize::from(curr)] = Some(pid),
                _ => self.head = Some(pid),
            }
            threads.set_state(pid, state);
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
        THREADS.with_cs(cs, |threads| {
            self.head = threads.thread_blocklist()[usize::from(head)].take();
            let old_state = threads.set_state(head, ThreadState::Running);
            Some((head, old_state))
        })
    }

    /// Determines if this [`ThreadList`] is empty.
    pub fn is_empty(&self, _cs: CriticalSection) -> bool {
        self.head.is_none()
    }
}
