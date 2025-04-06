//! Multi-threading for Ariel OS.
//!
//! Implements a scheduler based on fixed priorities and preemption.
//! Within one priority level, threads are scheduled cooperatively.
//! This means that there is no time slicing that would equally distribute CPU time among same-priority threads.
//! **Instead, you need to use [`yield_same()`] to explicitly yield to another thread with the same priority.**
//! If no thread is ready, the core is prompted to enter deep sleep until a next thread is ready.
//!
//! Threads should be implemented using the `ariel_os_macros::thread` proc macro, which takes care
//! of calling the necessary initialization methods and linking the thread function element it into the binary.
//! A [`ThreadId`] between 0 and [`THREAD_COUNT`] is assigned to each thread in the order in
//! which the threads are declared.
//!
//! Optionally, the stacksize and a priority between 1 and [`SCHED_PRIO_LEVELS`] can be configured.
//! By default, the stack size is 2048 bytes and priority is 1.
//!
//! # Synchronization
//!
//! The `threading` module supports three basic synchronization primitives:
//! - [`Channel`](sync::Channel): synchronous (blocking) channel for sending data between threads
//! - [`Lock`](sync::Lock): basic locking object
//! - [`thread_flags`]: thread-flag implementation for signaling between threads

#![cfg_attr(not(test), no_std)]
#![feature(negative_impls)]
#![feature(used_with_arg)]
#![cfg_attr(target_arch = "xtensa", feature(asm_experimental_arch))]
#![deny(clippy::pedantic)]
// Disable indexing lints for now, possible panics are documented or rely on internally-enforced
// invariants
#![allow(clippy::indexing_slicing)]
#![expect(clippy::cast_possible_truncation)]

#[cfg(feature = "core-affinity")]
compile_error!("core-affinities are not supported in \"reallocation\" variant");

mod arch;
mod autostart_thread;
mod ensure_once;
mod thread;
mod threadlist;

#[cfg(feature = "multi-core")]
mod smp;

pub mod sync;
pub mod thread_flags;

#[doc(hidden)]
pub mod macro_reexports {
    // Used by `autostart_thread`
    pub use linkme;
    pub use paste;
    pub use static_cell;
}

#[doc(hidden)]
pub mod events {
    use crate::sync::Event;
    // this is set in `ariel_os_embassy::init_task()`
    pub static THREAD_START_EVENT: Event = Event::new();
}

pub use ariel_os_runqueue::{CoreId, RunqueueId, ThreadId};
pub use thread_flags as flags;

#[cfg(feature = "core-affinity")]
pub use smp::CoreAffinity;

use arch::{Arch, Cpu, ThreadData, schedule};
use ariel_os_runqueue::{GlobalRunqueue, RunQueue};
use ensure_once::EnsureOnce;
use thread::{Thread, ThreadState};

#[cfg(feature = "multi-core")]
use smp::{Multicore, schedule_on_core};
#[cfg(feature = "multi-core")]
use static_cell::ConstStaticCell;

/// Dummy type that is needed because [`CoreAffinity`] is part of the general API.
///
/// To configure core affinities for threads, the `core-affinity` feature must be enabled.
#[cfg(not(feature = "core-affinity"))]
pub struct CoreAffinity {
    // Phantom field to ensure that `CoreAffinity` can never be constructed by a user.
    _phantom: core::marker::PhantomData<()>,
}

#[cfg(not(feature = "multi-core"))]
fn schedule_on_core(_: CoreId) {
    schedule()
}

/// The number of possible priority levels.
pub const SCHED_PRIO_LEVELS: usize = THREAD_COUNT;

/// The maximum number of concurrent threads that can be created.
pub const THREAD_COUNT: usize = 16;

/// Number of processor cores.
pub const CORE_COUNT: usize = {
    #[cfg(not(feature = "multi-core"))]
    const CORE_COUNT: usize = 1;
    #[cfg(feature = "multi-core")]
    const CORE_COUNT: usize = smp::Chip::CORES as usize;
    CORE_COUNT
};
/// Stack size of the idle threads (in bytes).
#[cfg(feature = "multi-core")]
pub const IDLE_THREAD_STACK_SIZE: usize = smp::Chip::IDLE_THREAD_STACK_SIZE;

static SCHEDULER: EnsureOnce<Scheduler> = EnsureOnce::new(Scheduler::new());

#[doc(hidden)]
pub type ThreadFn = fn();

#[doc(hidden)]
#[linkme::distributed_slice]
pub static THREAD_FNS: [ThreadFn] = [..];

/// Struct holding all scheduler state
struct Scheduler {
    /// Global thread runqueue.
    #[cfg(not(feature = "multi-core"))]
    runqueue: RunQueue<SCHED_PRIO_LEVELS, THREAD_COUNT>,
    #[cfg(feature = "multi-core")]
    runqueue: RunQueue<SCHED_PRIO_LEVELS, THREAD_COUNT, CORE_COUNT>,
    /// The actual TCBs.
    threads: [Thread; THREAD_COUNT],
    /// `Some` when a thread is blocking another thread due to conflicting
    /// resource access.
    thread_blocklist: [Option<ThreadId>; THREAD_COUNT],

    /// The currently running thread(s).
    #[cfg(feature = "multi-core")]
    current_threads: [Option<ThreadId>; CORE_COUNT],
    #[cfg(not(feature = "multi-core"))]
    current_thread: Option<ThreadId>,
}

impl Scheduler {
    const fn new() -> Self {
        Self {
            runqueue: RunQueue::new(),
            threads: [const { Thread::default() }; THREAD_COUNT],
            thread_blocklist: [const { None }; THREAD_COUNT],
            #[cfg(feature = "multi-core")]
            current_threads: [None; CORE_COUNT],
            #[cfg(not(feature = "multi-core"))]
            current_thread: None,
        }
    }

    // pub(crate) fn by_tid_unckecked(&mut self, thread_id: ThreadId) -> &mut Thread {
    //     &mut self.threads[thread_id as usize]
    // }

    /// Returns checked mutable access to the thread data of the currently
    /// running thread.
    ///
    /// Returns `None` if there is no current thread.
    fn current(&mut self) -> Option<&mut Thread> {
        self.current_tid()
            .map(|tid| &mut self.threads[usize::from(tid)])
    }

    /// Returns the ID of the current thread, or [`None`] if no thread is currently
    /// running.
    ///
    /// On multi-core, it returns the ID of the thread that is running on the
    /// current core.
    #[inline]
    fn current_tid(&self) -> Option<ThreadId> {
        #[cfg(feature = "multi-core")]
        {
            self.current_threads[usize::from(core_id())]
        }
        #[cfg(not(feature = "multi-core"))]
        {
            self.current_thread
        }
    }

    /// Returns a mutable reference to the current thread ID, or [`None`]
    /// if no thread is currently running.
    ///
    /// On multi-core, it refers to the ID of the thread that is running on the
    /// current core.
    #[allow(dead_code, reason = "used in scheduler implementation")]
    fn current_tid_mut(&mut self) -> &mut Option<ThreadId> {
        #[cfg(feature = "multi-core")]
        {
            &mut self.current_threads[usize::from(core_id())]
        }
        #[cfg(not(feature = "multi-core"))]
        {
            &mut self.current_thread
        }
    }

    /// Creates a new thread.
    ///
    /// This sets up the stack and TCB for this thread.
    ///
    /// Returns `None` if there is no free thread slot.
    fn create(
        &mut self,
        func: usize,
        arg: usize,
        stack: &'static mut [u8],
        prio: RunqueueId,
        _core_affinity: Option<CoreAffinity>,
    ) -> Option<ThreadId> {
        let (thread, tid) = self.get_unused()?;
        Cpu::setup_stack(thread, stack, func, arg);
        thread.prio = prio;
        thread.tid = tid;
        thread.state = ThreadState::Parked;
        #[cfg(feature = "core-affinity")]
        {
            thread.core_affinity = _core_affinity.unwrap_or_default();
        }

        Some(tid)
    }

    /// Returns immutable access to any thread data.
    ///
    /// # Panics
    ///
    /// Panics if `thread_id` is >= [`THREAD_COUNT`].
    /// If the thread for this `thread_id` is in an invalid state, the
    /// data in the returned [`Thread`] is undefined, i.e. empty or outdated.
    fn get_unchecked(&self, thread_id: ThreadId) -> &Thread {
        &self.threads[usize::from(thread_id)]
    }

    /// Returns mutable access to any thread data.
    ///
    /// # Panics
    ///
    /// Panics if `thread_id` is >= [`THREAD_COUNT`].
    /// If the thread for this `thread_id` is in an invalid state, the
    /// data in the returned [`Thread`] is undefined, i.e. empty or outdated.
    fn get_unchecked_mut(&mut self, thread_id: ThreadId) -> &mut Thread {
        &mut self.threads[usize::from(thread_id)]
    }

    /// Returns an unused [`ThreadId`] / Thread slot.
    fn get_unused(&mut self) -> Option<(&mut Thread, ThreadId)> {
        for i in 0..THREAD_COUNT {
            if self.threads[i].state == ThreadState::Invalid {
                return Some((&mut self.threads[i], ThreadId::new(i as u8)));
            }
        }
        None
    }

    /// Checks if a thread with valid state exists for this `thread_id`.
    fn is_valid_tid(&self, thread_id: ThreadId) -> bool {
        if usize::from(thread_id) >= THREAD_COUNT {
            false
        } else {
            self.threads[usize::from(thread_id)].state != ThreadState::Invalid
        }
    }

    /// Sets the state of a thread.
    ///
    /// This function handles adding/ removing the thread to the Runqueue depending
    /// on its previous or new state.
    ///
    /// # Panics
    ///
    /// Panics if `tid` is >= [`THREAD_COUNT`].
    fn set_state(&mut self, tid: ThreadId, state: ThreadState) -> (ThreadState, Option<CoreId>) {
        let thread = &mut self.threads[usize::from(tid)];
        let old_state = thread.state;
        thread.state = state;
        let core = match (old_state, state) {
            (old, new) if old == new => None,
            (_, ThreadState::Running) => self.runqueue.add(thread.tid, thread.prio),
            (ThreadState::Running, _) => {
                #[cfg(not(feature = "multi-core"))]
                {
                    self.runqueue.pop_head(thread.tid, thread.prio)
                }
                #[cfg(feature = "multi-core")]
                {
                    self.runqueue.del(thread.tid, thread.prio)
                }
            }
            _ => None,
        };
        (old_state, core)
    }

    /// Returns the state of a thread.
    fn get_state(&self, thread_id: ThreadId) -> Option<ThreadState> {
        if self.is_valid_tid(thread_id) {
            Some(self.threads[usize::from(thread_id)].state)
        } else {
            None
        }
    }

    /// Returns the priority of a thread.
    fn get_priority(&self, thread_id: ThreadId) -> Option<RunqueueId> {
        self.is_valid_tid(thread_id)
            .then(|| self.get_unchecked(thread_id).prio)
    }

    /// Changes the priority of a thread.
    ///
    /// Returns the information if the scheduler should be invoked because the runqueue order
    /// might have changed.
    /// `false` if the thread isn't in the runqueue (in which case the priority is still changed)
    /// or if the new priority equals the current one.
    fn set_priority(&mut self, thread_id: ThreadId, prio: RunqueueId) -> Option<CoreId> {
        if !self.is_valid_tid(thread_id) {
            return None;
        }
        let thread = self.get_unchecked_mut(thread_id);
        let old_prio = thread.prio;
        if old_prio == prio {
            return None;
        }
        thread.prio = prio;
        if thread.state != ThreadState::Running {
            return None;
        }

        let sched_after_del = self.runqueue.del(thread_id, old_prio);
        let sched_after_add = self.runqueue.add(thread_id, prio);
        sched_after_del.or(sched_after_add)
    }
}

/// Starts threading.
///
/// Supposed to be started early on by OS startup code.
///
/// # Safety
///
/// This function is crafted to be called at a specific point in the Ariel OS
/// initialization, by `ariel-os-rt`. Don't call this unless you know you need to.
///
/// Currently it expects at least:
/// - Cortex-M: to be called from the reset handler while MSP is active
#[doc(hidden)]
pub unsafe fn start_threading() {
    #[cfg(feature = "multi-core")]
    {
        ariel_os_debug::log::debug!("ariel-os-threads: SMP mode with {} cores", CORE_COUNT);

        // Idle thread that prompts the core to enter deep sleep.
        fn idle_thread() {
            loop {
                Cpu::wfi();
            }
        }

        // Stacks for the idle threads.
        // Creating them inside the below for-loop is not possible because it would result in
        // duplicate identifiers for the created `static`.
        static STACKS: [ConstStaticCell<[u8; IDLE_THREAD_STACK_SIZE]>; CORE_COUNT] =
            [const { ConstStaticCell::new([0u8; IDLE_THREAD_STACK_SIZE]) }; CORE_COUNT];

        // Create one idle thread for each core with lowest priority.
        for stack in &STACKS {
            create_noarg(idle_thread, stack.take(), 0, None);
        }

        smp::Chip::startup_other_cores();
    }
    Cpu::start_threading();
}

/// Trait for types that can be used as argument for threads.
///
/// # Safety
///
/// This trait must only be implemented on types whose binary representation fits into a single
/// general-purpose register on *all supported architectures*.
pub unsafe trait Arguable {
    /// Returns the ABI representation.
    fn into_arg(self) -> usize;
}

// SAFETY: this is the identity.
unsafe impl Arguable for usize {
    fn into_arg(self) -> usize {
        self
    }
}

// SAFETY:
// This is only implemented on *static* references because the references passed to a thread must
// be valid for its entire lifetime.
unsafe impl<T: Sync + Sized> Arguable for &'static T {
    fn into_arg(self) -> usize {
        // Ensure that a pointer does fit into a single machine word.
        const {
            assert!(size_of::<*const T>() == size_of::<u32>());
        }
        core::ptr::from_ref::<T>(self) as usize
    }
}

/// Low-level function to create a thread that runs
/// `func` with `arg`.
///
/// This sets up the stack for the thread and adds it to
/// the runqueue.
///
/// # Panics
///
/// Panics if more than [`THREAD_COUNT`] concurrent threads have been created.
pub fn create<T: Arguable + Send>(
    func: fn(arg: T),
    arg: T,
    stack: &'static mut [u8],
    prio: u8,
    core_affinity: Option<CoreAffinity>,
) -> ThreadId {
    let arg = arg.into_arg();
    unsafe { create_raw(func as usize, arg, stack, prio, core_affinity) }
}

/// Low-level function to create a thread without argument
///
/// # Panics
///
/// Panics if more than [`THREAD_COUNT`] concurrent threads have been created.
pub fn create_noarg(
    func: fn(),
    stack: &'static mut [u8],
    prio: u8,
    core_affinity: Option<CoreAffinity>,
) -> ThreadId {
    unsafe { create_raw(func as usize, 0, stack, prio, core_affinity) }
}

/// Creates a thread, low-level.
///
/// # Safety
///
/// Only use when you know what you are doing.
#[doc(hidden)]
pub unsafe fn create_raw(
    func: usize,
    arg: usize,
    stack: &'static mut [u8],
    prio: u8,
    core_affinity: Option<CoreAffinity>,
) -> ThreadId {
    SCHEDULER.with_mut(|mut scheduler| {
        let thread_id = scheduler
            .create(func, arg, stack, RunqueueId::new(prio), core_affinity)
            .expect("Max `THREAD_COUNT` concurrent threads should be created.");
        scheduler.set_state(thread_id, ThreadState::Running);
        thread_id
    })
}

/// Returns the [`ThreadId`] of the currently active thread.
///
/// Note: when called from ISRs, this will return the thread id of the thread
/// that was interrupted.
pub fn current_tid() -> Option<ThreadId> {
    SCHEDULER.with(|scheduler| scheduler.current_tid())
}

/// Returns the id of the CPU that this thread is running on.
#[must_use]
pub fn core_id() -> CoreId {
    #[cfg(not(feature = "multi-core"))]
    {
        CoreId::new(0)
    }
    #[cfg(feature = "multi-core")]
    {
        smp::Chip::core_id()
    }
}

/// Checks if a given [`ThreadId`] is valid.
pub fn is_valid_tid(thread_id: ThreadId) -> bool {
    SCHEDULER.with(|scheduler| scheduler.is_valid_tid(thread_id))
}

/// Thread cleanup function.
///
/// This gets hooked into a newly created thread stack so it gets called when
/// the thread function returns.
///
/// # Panics
///
/// Panics if this is called outside of a thread context.
#[allow(unused)]
fn cleanup() -> ! {
    SCHEDULER.with_mut(|mut scheduler| {
        let thread_id = scheduler.current_tid().unwrap();
        scheduler.set_state(thread_id, ThreadState::Invalid);
    });

    schedule();

    unreachable!();
}

/// "Yields" to another thread with the same priority.
pub fn yield_same() {
    SCHEDULER.with_mut(|mut scheduler| {
        let Some((tid, prio)) = scheduler.current().map(|t| (t.tid, t.prio)) else {
            return;
        };
        if scheduler.runqueue.advance(tid, prio).is_some() {
            schedule();
        }
    })
}

/// Suspends/ pauses the current thread's execution.
#[doc(alias = "sleep")]
pub fn park() {
    SCHEDULER.with_mut(|mut scheduler| {
        let Some(tid) = scheduler.current_tid() else {
            return;
        };
        scheduler.set_state(tid, ThreadState::Parked);
        schedule();
    });
}

/// Wakes up a thread and adds it to the runqueue.
///
/// Returns `false` if no parked thread exists for `thread_id`.
#[doc(alias = "wakeup")]
pub fn unpark(thread_id: ThreadId) -> bool {
    SCHEDULER.with_mut(|mut scheduler| {
        match scheduler.get_state(thread_id) {
            Some(ThreadState::Parked) => {}
            _ => return false,
        }
        if let Some(core_id) = scheduler.set_state(thread_id, ThreadState::Running).1 {
            schedule_on_core(core_id);
        }
        true
    })
}

/// Returns the priority of a thread.
///
/// Returns `None` if this is not a valid thread.
pub fn get_priority(thread_id: ThreadId) -> Option<RunqueueId> {
    SCHEDULER.with_mut(|scheduler| scheduler.get_priority(thread_id))
}

/// Changes the priority of a thread.
///
/// This might trigger a context switch.
pub fn set_priority(thread_id: ThreadId, prio: RunqueueId) {
    SCHEDULER.with_mut(|mut scheduler| {
        if let Some(core) = scheduler.set_priority(thread_id, prio) {
            schedule_on_core(core);
        }
    })
}
