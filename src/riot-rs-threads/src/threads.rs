#![allow(dead_code)]
#![allow(non_camel_case_types)]

mod threads_inner;

use core::ops::{Deref, DerefMut};

use riot_rs_runqueue::{RunqueueId, ThreadId};
use threads_inner::ThreadsInner;

use crate::{
    arch::{schedule, Arch, Cpu},
    thread::{Thread, ThreadState},
    CoreAffinity, SCHED_PRIO_LEVELS, THREADS_NUMOF,
};

#[cfg(feature = "multi-core")]
use crate::{core_id, smp::schedule_on_core, CoreId, CORES_NUMOF};

pub type RunQueue = riot_rs_runqueue::RunQueue<SCHED_PRIO_LEVELS, THREADS_NUMOF>;
pub type BlockList = [Option<ThreadId>; THREADS_NUMOF];

/// Struct holding all scheduler state
pub struct Threads {
    inner: threads_inner::ThreadsInner,
}

impl Threads {
    pub const fn new() -> Self {
        Self {
            inner: threads_inner::ThreadsInner::new(),
        }
    }

    /// Returns the ID of the current thread, or [`None`] if no thread is currently
    /// running.
    ///
    /// On multicore, it returns the ID of the thread that is running on the
    /// current core.
    #[inline]
    pub fn current_pid(&mut self) -> Option<ThreadId> {
        self.current_threads().current_pid()
    }

    /// Creates a new thread.
    ///
    /// This sets up the stack and TCB for this thread.
    ///
    /// Returns `None` if there is no free thread slot.
    pub fn create(
        &mut self,
        func: usize,
        arg: usize,
        stack: &'static mut [u8],
        prio: RunqueueId,
        _core_affinity: Option<CoreAffinity>,
    ) -> Option<ThreadId> {
        let mut tcbs = self.tcbs_mut();
        for i in 0..THREADS_NUMOF {
            let thread_id = ThreadId::new(i as u8);
            let thread = tcbs.get_unchecked_mut(thread_id);
            if thread.state != ThreadState::Invalid {
                continue;
            }
            Cpu::setup_stack(thread, stack, func, arg);
            thread.pid = thread_id;
            thread.prio = prio;
            #[cfg(feature = "core-affinity")]
            {
                thread.core_affinity = _core_affinity.unwrap_or_default();
            }
            return Some(thread_id);
        }
        None
    }

    /// Checks if a thread ID is in bound.
    pub fn is_in_bound(&self, thread_id: ThreadId) -> bool {
        usize::from(thread_id) < THREADS_NUMOF
    }

    /// Checks if a thread with valid state exists for this `thread_id`.
    pub fn is_valid_pid(&mut self, thread_id: ThreadId) -> bool {
        if !self.is_in_bound(thread_id) {
            return false;
        }
        self.tcbs().get_unchecked(thread_id).state != ThreadState::Invalid
    }

    /// Returns the state of a thread.
    pub fn get_state(&mut self, thread_id: ThreadId) -> Option<ThreadState> {
        if usize::from(thread_id) >= THREADS_NUMOF {
            return None;
        }
        Some(self.tcbs().get_unchecked(thread_id).state)
    }

    /// Sets the state of a thread and triggers the scheduler if needed.
    ///
    /// This function also handles adding/ removing the thread to the Runqueue depending
    /// on its previous or new state.
    ///
    /// # Panics
    ///
    /// Panics if `pid` is >= [`THREADS_NUMOF`].
    pub fn set_state(&mut self, pid: ThreadId, state: ThreadState) -> ThreadState {
        let mut tcbs = self.tcbs_mut();
        let thread = tcbs.get_unchecked_mut(pid);
        let old_state = core::mem::replace(&mut thread.state, state);
        let prio = thread.prio;
        tcbs.release();
        if state == ThreadState::Running {
            self.runqueue_mut().add(pid, prio);
            self.schedule_if_higher_prio(pid, prio);
        } else if old_state == ThreadState::Running {
            // A running thread is only set to a non-running state
            // if it itself initiated it.
            debug_assert_eq!(Some(pid), self.current_pid());

            // On multicore, the currently running thread is not in the runqueue
            // anyway, so we don't need to remove it here.
            #[cfg(not(feature = "multi-core"))]
            self.runqueue_mut().pop_head(pid, prio);

            schedule();
        }
        old_state
    }

    /// Returns the priority of a thread.
    pub fn get_priority(&mut self, thread_id: ThreadId) -> RunqueueId {
        self.tcbs().get_priority(thread_id)
    }

    /// Change the priority of a thread and triggers the scheduler if needed.
    pub fn set_priority(&mut self, thread_id: ThreadId, prio: RunqueueId) {
        if !self.is_in_bound(thread_id) {
            return;
        }
        let mut tcbs = self.tcbs_mut();
        let thread = tcbs.get_unchecked_mut(thread_id);
        if thread.state == ThreadState::Invalid {
            return;
        }
        let old_prio = thread.prio;
        if old_prio == prio {
            return;
        }
        thread.prio = prio;

        if thread.state != ThreadState::Running {
            // No runqueue changes or scheduler invocations needed.
            return;
        }
        tcbs.release();

        // Check if the thread is among the current threads and trigger scheduler if
        // its prio decreased and another thread might have a higher prio now.
        // This has to be done on multicore **before the runqueue changes below**, because
        // a currently running thread is not in the runqueue and therefore the runqueue changes
        // should not be applied.
        #[cfg(feature = "multi-core")]
        if let Some(core) = self.is_running(thread_id) {
            self.current_threads_mut().current_threads[0] = Some(thread_id);
            if prio < old_prio {
                schedule_on_core(CoreId(core as u8));
            }
            return;
        }

        // Update the runqueue.
        {
            let mut runqueue = self.runqueue_mut();
            if runqueue.peek_head(old_prio) == Some(thread_id) {
                runqueue.pop_head(thread_id, old_prio);
            } else {
                runqueue.del(thread_id);
            }
            runqueue.add(thread_id, prio);
        }

        // Check & handle if the thread is among the current threads for single-core,
        // analogous to the above multicore implementation.
        #[cfg(not(feature = "multi-core"))]
        if self.is_running(thread_id).is_some() {
            self.current_threads_mut().set_current_pid(thread_id);
            if prio < old_prio {
                schedule()
            }
            return;
        }

        // Thread isn't running.
        // Only schedule if the thread has a higher priority than a running one.
        if prio > old_prio {
            self.schedule_if_higher_prio(thread_id, prio);
        }
    }

    /// Triggers the scheduler if the thread has a higher priority than (one of)
    /// the running thread(s).
    pub fn schedule_if_higher_prio(&mut self, _thread_id: ThreadId, prio: RunqueueId) {
        #[cfg(not(feature = "multi-core"))]
        match self.current_pid().map(|pid| self.get_priority(pid)) {
            Some(curr_prio) if curr_prio < prio => schedule(),
            _ => {}
        }
        #[cfg(feature = "multi-core")]
        match self.lowest_running_prio(_thread_id) {
            (core, Some(lowest_prio)) if lowest_prio < prio => schedule_on_core(core),
            _ => {}
        }
    }

    /// Returns `Some` if the thread is currently running on a core.
    ///
    /// On multicore, the core-id is returned as usize, on single-core
    /// the usize is always 0.
    fn is_running(&mut self, thread_id: ThreadId) -> Option<usize> {
        self.current_threads().is_running(thread_id)
    }

    /// Adds the thread that is running on the current core to the
    /// runqueue if it has state [`ThreadState::Running`].
    #[cfg(feature = "multi-core")]
    #[allow(dead_code, reason = "used in scheduler implementation")]
    pub fn add_current_thread_to_rq(
        current_threads: &CurrentThreads,
        runqueue: &mut RunQueue,
        tcbs: &TCBs,
    ) {
        let Some(current_pid) = current_threads.current_pid() else {
            return;
        };
        let &Thread { state, prio, .. } = tcbs.get_unchecked(current_pid);
        if state == ThreadState::Running {
            runqueue.add(current_pid, prio);
        }
    }

    /// Returns the next thread from the runqueue.
    ///
    /// On single-core, the thread remains in the runqueue, so subsequent calls
    /// will return the same thread.
    ///
    /// On multi-core, the thread is removed so that subsequent calls will each
    /// return a different thread. This prevents that a thread is picked multiple
    /// times by the scheduler when it is invoked on different cores.
    #[allow(dead_code, reason = "used in scheduler implementation")]
    pub fn get_next_pid(runqueue: &mut RunQueue, _tcbs: &TCBs) -> Option<ThreadId> {
        // On single-core, only read the head of the runqueue.
        #[cfg(not(feature = "multi-core"))]
        {
            runqueue.get_next()
        }

        // On multi-core, the head is popped of the runqueue.
        #[cfg(all(feature = "multi-core", not(feature = "core-affinity")))]
        {
            runqueue.pop_next()
        }

        // On multicore with core-affinities, get next thread with matching affinity.
        #[cfg(all(feature = "multi-core", feature = "core-affinity"))]
        {
            let core = crate::core_id();
            let next = runqueue.get_next_filter(|&t| {
                // Check if thread can be scheduled on the current core.
                _tcbs.get_unchecked(t).core_affinity.contains(core)
            })?;
            // Delete thread from runqueue to match the `pop_next`.
            runqueue.del(next);
            Some(next)
        }
    }

    /// Searches for the lowest priority thread among the currently running threads.
    ///
    /// Returns the core that the lowest priority thread is running on, and its priority.
    /// Returns `None` for the priority if an idle core was found, which is only the case
    /// during startup.
    ///
    /// If core-affinities are enabled, the parameter `_pid` restricts the search to only
    /// consider the cores that match this thread's [`CoreAffinity`].
    #[cfg(feature = "multi-core")]
    fn lowest_running_prio(&mut self, _pid: ThreadId) -> (CoreId, Option<RunqueueId>) {
        let (mut guard, tcbs) = self.with_tcbs();
        let current_threads = guard.current_threads();
        #[cfg(feature = "core-affinity")]
        let affinity = tcbs.get_unchecked(_pid).core_affinity;
        // Find the lowest priority thread among the currently running threads.
        current_threads
            .current_threads
            .iter()
            .enumerate()
            .filter_map(|(core, pid)| {
                let core = CoreId(core as u8);
                // Skip cores that don't match the core-affinity.
                #[cfg(feature = "core-affinity")]
                if !affinity.contains(core) {
                    return None;
                }
                let prio = pid.map(|pid| tcbs.get_priority(pid));
                Some((core, prio))
            })
            .min_by_key(|(_, rq)| *rq)
            .unwrap()
    }
}

impl Deref for Threads {
    type Target = ThreadsInner;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Threads {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

pub struct CurrentThreads {
    #[cfg(feature = "multi-core")]
    pub current_threads: [Option<ThreadId>; CORES_NUMOF],
    #[cfg(not(feature = "multi-core"))]
    pub current_thread: Option<ThreadId>,
}

impl CurrentThreads {
    pub const fn new() -> Self {
        Self {
            #[cfg(feature = "multi-core")]
            current_threads: [None; CORES_NUMOF],
            #[cfg(not(feature = "multi-core"))]
            current_thread: None,
        }
    }

    /// Returns the ID of the current thread, or [`None`] if no thread is currently
    /// running.
    ///
    /// On multicore, it returns the ID of the thread that is running on the
    /// current core.
    #[inline]
    pub fn current_pid(&self) -> Option<ThreadId> {
        #[cfg(feature = "multi-core")]
        {
            self.current_threads[usize::from(core_id())]
        }
        #[cfg(not(feature = "multi-core"))]
        {
            self.current_thread
        }
    }

    /// Sets the pid that is running on the current core.
    #[allow(dead_code, reason = "used in scheduler implementation")]
    pub fn set_current_pid(&mut self, pid: ThreadId) {
        #[cfg(feature = "multi-core")]
        {
            self.current_threads[usize::from(core_id())] = Some(pid)
        }
        #[cfg(not(feature = "multi-core"))]
        {
            self.current_thread = Some(pid)
        }
    }

    /// Returns `Some` if the thread is currently running on a core.
    ///
    /// On multicore, the core-id is returned as usize, on single-core
    /// the usize is always 0.
    pub fn is_running(&self, thread_id: ThreadId) -> Option<usize> {
        #[cfg(not(feature = "multi-core"))]
        {
            self.current_pid()
                .and_then(|pid| (pid == thread_id).then_some(0))
        }

        #[cfg(feature = "multi-core")]
        {
            self.current_threads
                .iter()
                .position(|pid| *pid == Some(thread_id))
        }
    }
}

pub struct TCBs {
    pub tcbs: [Thread; crate::THREADS_NUMOF],
}

impl TCBs {
    pub const fn new() -> Self {
        TCBs {
            tcbs: [const { Thread::default() }; crate::THREADS_NUMOF],
        }
    }
    /// Returns immutable access to any thread data.
    ///
    /// # Panics
    ///
    /// Panics if `thread_id` is >= [`THREADS_NUMOF`].
    /// If the thread for this `thread_id` is in an invalid state, the
    /// data in the returned [`Thread`] is undefined, i.e. empty or outdated.
    #[allow(dead_code, reason = "used in scheduler implementation")]
    pub fn get_unchecked(&self, thread_id: ThreadId) -> &Thread {
        &self.tcbs[usize::from(thread_id)]
    }

    /// Returns mutable access to any thread data.
    ///
    /// # Panics
    ///
    /// Panics if `thread_id` is >= [`THREADS_NUMOF`].
    /// If the thread for this `thread_id` is in an invalid state, the
    /// data in the returned [`Thread`] is undefined, i.e. empty or outdated.
    pub fn get_unchecked_mut(&mut self, thread_id: ThreadId) -> &mut Thread {
        &mut self.tcbs[usize::from(thread_id)]
    }

    /// Returns the priority of a thread.
    pub fn get_priority(&self, thread_id: ThreadId) -> RunqueueId {
        self.get_unchecked(thread_id).prio
    }
}
