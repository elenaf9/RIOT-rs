use core::ops::Deref;

use crate::{RunqueueId, ThreadId, ThreadState, Threads};

/// Manages blocked [`super::Thread`]s for a resource, and triggering the scheduler when needed.
#[derive(Debug, Default)]
pub struct ThreadList {
    /// Next thread to run once the resource is available.
    head: Option<ThreadId>,
}

impl ThreadList {
    /// Creates a new empty [`ThreadList`]
    pub const fn new() -> Self {
        Self { head: None }
    }

    /// Puts the current (blocked) thread into this [`ThreadList`] and triggers the scheduler.
    ///
    /// Returns a `RunqueueId` if the highest priority among the waiters in the list has changed.
    pub fn put_current<T: Deref<Target = Threads>>(
        &mut self,
        threads: &T,
        state: ThreadState,
    ) -> Option<RunqueueId> {
        let (pid, prio) = threads.current_pid_prio().unwrap();
        let mut curr = None;
        let mut next = self.head;
        while let Some(n) = next {
            let thread_b_prio = threads.get_unchecked_with(n, |t| t.prio);
            if thread_b_prio < prio {
                break;
            }
            curr = next;
            next = threads.thread_blocklist.with(|l| l[usize::from(n)]);
        }
        threads
            .thread_blocklist
            .with(|l| l[usize::from(pid)] = next);
        let inherit_priority = match curr {
            Some(curr) => {
                threads
                    .thread_blocklist
                    .with(|l| l[usize::from(curr)] = Some(pid));
                None
            }
            _ => {
                self.head = Some(pid);
                Some(prio)
            }
        };
        threads.set_state(pid, state);
        inherit_priority
    }

    /// Removes the head from this [`ThreadList`].
    ///
    /// Sets the thread's [`ThreadState`] to [`ThreadState::Running`] and triggers
    /// the scheduler.
    ///
    /// Returns the thread's [`ThreadId`] and its previous [`ThreadState`].
    pub fn pop<T: Deref<Target = Threads>>(
        &mut self,
        threads: &T,
    ) -> Option<(ThreadId, ThreadState)> {
        let head = self.head?;
        self.head = threads.thread_blocklist.with(|l| l[usize::from(head)]);
        let old_state = threads.set_state(head, ThreadState::Running);
        Some((head, old_state))
    }

    /// Determines if this [`ThreadList`] is empty.
    pub fn is_empty(&self) -> bool {
        self.head.is_none()
    }
}
