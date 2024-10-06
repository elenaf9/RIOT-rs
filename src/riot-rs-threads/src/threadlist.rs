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
    ///
    /// # Panics
    ///
    /// Panics if this is called outside of a thread context.
    pub fn put_current(&mut self, threads: &Threads, state: ThreadState) -> Option<RunqueueId> {
        let (pid, prio) = threads
            .current_pid_prio()
            .expect("Function should be called inside a thread context.");
        let mut curr = None;
        let mut next = self.head;
        let mut thread_blocklist = threads.thread_blocklist.lock_mut();
        while let Some(n) = next {
            if threads.get_priority(n) < prio {
                break;
            }
            curr = next;
            next = thread_blocklist[usize::from(n)];
        }
        thread_blocklist[usize::from(pid)] = next;
        let inherit_priority = match curr {
            Some(curr) => {
                thread_blocklist[usize::from(curr)] = Some(pid);
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
    pub fn pop(&mut self, threads: &Threads) -> Option<(ThreadId, ThreadState)> {
        let head = self.head?;
        self.head = threads.thread_blocklist.lock_mut()[usize::from(head)].take();
        let old_state = threads.set_state(head, ThreadState::Running);
        Some((head, old_state))
    }

    /// Determines if this [`ThreadList`] is empty.
    pub fn is_empty(&self) -> bool {
        self.head.is_none()
    }
}
