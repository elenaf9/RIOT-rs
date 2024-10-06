//! Multi-threading for RIOT-rs.
//!
//! Implements a scheduler based on fixed priorities and preemption.
//! Within one priority level, threads are scheduled cooperatively.
//! This means that there is no time slicing that would equally distribute CPU time among same-priority threads.
//! **Instead, you need to use [`yield_same()`] to explicitly yield to another thread with the same priority.**
//! If no thread is ready, the core is prompted to enter deep sleep until a next thread is ready.
//!
//! Threads should be implemented using the `riot_rs_macros::thread` proc macro, which takes care
//! of calling the necessary initialization methods and linking the thread function element it into the binary.
//! A [`ThreadId`] between 0 and [`THREADS_NUMOF`] is assigned to each thread in the order in
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
#![feature(naked_functions)]
#![feature(used_with_arg)]
#![cfg_attr(target_arch = "xtensa", feature(asm_experimental_arch))]
// Disable indexing lints for now, possible panics are documented or rely on internally-enforced
// invariants
#![allow(clippy::indexing_slicing)]

mod arch;
mod autostart_thread;
mod critical_section;
mod scheduler_lock;
mod thread;
mod threadlist;
mod threads;

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

pub use riot_rs_runqueue::{RunqueueId, ThreadId};
pub use thread_flags as flags;

#[cfg(feature = "core-affinity")]
pub use smp::CoreAffinity;

use arch::{schedule, Arch, Cpu, ThreadData};
use scheduler_lock::SchedulerLock;
use thread::{Thread, ThreadState};
use threads::Threads;

#[cfg(feature = "multi-core")]
use smp::Multicore;
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

/// The number of possible priority levels.
pub const SCHED_PRIO_LEVELS: usize = 12;

/// The maximum number of concurrent threads that can be created.
pub const THREADS_NUMOF: usize = 16;

#[cfg(feature = "multi-core")]
pub const CORES_NUMOF: usize = smp::Chip::CORES as usize;
#[cfg(feature = "multi-core")]
pub const IDLE_THREAD_STACK_SIZE: usize = smp::Chip::IDLE_THREAD_STACK_SIZE;

static THREADS: SchedulerLock<Threads> = SchedulerLock::new(Threads::new());

pub type ThreadFn = fn();

#[linkme::distributed_slice]
pub static THREAD_FNS: [ThreadFn] = [..];

/// ID of a physical core.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct CoreId(pub(crate) u8);

impl From<CoreId> for usize {
    fn from(value: CoreId) -> Self {
        value.0 as usize
    }
}

/// Starts threading.
///
/// Supposed to be started early on by OS startup code.
///
/// # Safety
///
/// This function is crafted to be called at a specific point in the RIOT-rs
/// initialization, by `riot-rs-rt`. Don't call this unless you know you need to.
///
/// Currently it expects at least:
/// - Cortex-M: to be called from the reset handler while MSP is active
pub unsafe fn start_threading() {
    #[cfg(feature = "multi-core")]
    {
        // Idle thread that prompts the core to enter deep sleep.
        fn idle_thread() {
            loop {
                Cpu::wfi();
            }
        }

        // Stacks for the idle threads.
        // Creating them inside the below for-loop is not possible because it would result in
        // duplicate identifiers for the created `static`.
        static STACKS: [ConstStaticCell<[u8; IDLE_THREAD_STACK_SIZE]>; CORES_NUMOF] =
            [const { ConstStaticCell::new([0u8; IDLE_THREAD_STACK_SIZE]) }; CORES_NUMOF];

        // Create one idle thread for each core with lowest priority.
        for stack in &STACKS {
            thread_create_noarg(idle_thread, stack.take(), 0, None);
        }

        smp::Chip::startup_other_cores();
    }
    Cpu::start_threading();
}

/// Trait for types that fit into a single register.
///
/// Currently implemented for static references (`&'static T`) and usize.
pub trait Arguable {
    fn into_arg(self) -> usize;
}

impl Arguable for usize {
    fn into_arg(self) -> usize {
        self
    }
}

impl Arguable for () {
    fn into_arg(self) -> usize {
        0
    }
}

/// [`Arguable`] is only implemented on *static* references because the references passed to a
/// thread must be valid for its entire lifetime.
impl<T> Arguable for &'static T {
    fn into_arg(self) -> usize {
        self as *const T as usize
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
/// Panics if more than [`THREADS_NUMOF`] concurrent threads have been created.
pub fn thread_create<T: Arguable + Send>(
    func: fn(arg: T),
    arg: T,
    stack: &'static mut [u8],
    prio: u8,
    core_affinity: Option<CoreAffinity>,
) -> ThreadId {
    let arg = arg.into_arg();
    unsafe { thread_create_raw(func as usize, arg, stack, prio, core_affinity) }
}

/// Low-level function to create a thread without argument
///
/// # Panics
///
/// Panics if more than [`THREADS_NUMOF`] concurrent threads have been created.
pub fn thread_create_noarg(
    func: fn(),
    stack: &'static mut [u8],
    prio: u8,
    core_affinity: Option<CoreAffinity>,
) -> ThreadId {
    unsafe { thread_create_raw(func as usize, 0, stack, prio, core_affinity) }
}

/// Creates a thread, low-level.
///
/// # Safety
///
/// Only use when you know what you are doing.
pub unsafe fn thread_create_raw(
    func: usize,
    arg: usize,
    stack: &'static mut [u8],
    prio: u8,
    core_affinity: Option<CoreAffinity>,
) -> ThreadId {
    THREADS.with(|threads| {
        let thread_id = threads
            .create(func, arg, stack, RunqueueId::new(prio), core_affinity)
            .expect("Max `THREADS_NUMOF` concurrent threads should be created.");
        threads.set_state(thread_id, ThreadState::Running);
        thread_id
    })
}

/// Returns the [`ThreadId`] of the currently active thread.
///
/// Note: when called from ISRs, this will return the thread id of the thread
/// that was interrupted.
pub fn current_pid() -> Option<ThreadId> {
    THREADS.with(|threads| threads.current_pid())
}

/// Returns the id of the CPU that this thread is running on.
pub fn core_id() -> CoreId {
    #[cfg(not(feature = "multi-core"))]
    {
        CoreId(0)
    }
    #[cfg(feature = "multi-core")]
    {
        smp::Chip::core_id()
    }
}

/// Checks if a given [`ThreadId`] is valid.
pub fn is_valid_pid(thread_id: ThreadId) -> bool {
    THREADS.with(|threads| threads.is_valid_pid(thread_id))
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
    THREADS.with(|threads| {
        let thread_id = threads.current_pid().unwrap();
        threads.set_state(thread_id, ThreadState::Invalid);
    });

    unreachable!();
}

/// "Yields" to another thread with the same priority.
pub fn yield_same() {
    THREADS.with(|threads| {
        let Some(current_pid) = threads.current_pid() else {
            return;
        };
        let tcbs = threads.tcbs();
        let &Thread {
            prio,
            #[cfg(feature = "core-affinity")]
                core_affinity: _affinity,
            ..
        } = tcbs.get_unchecked(current_pid);
        drop(tcbs);

        #[cfg(not(feature = "multi-core"))]
        if threads.runqueue_mut().advance(prio) {
            schedule()
        }

        // On multi-core, the current thread is removed from the runqueue, and then
        // re-added **at the tail** in `sched` the next time the scheduler is invoked.
        // Simply triggering the scheduler therefore implicitly advances the runqueue.
        #[cfg(feature = "multi-core")]
        if !threads.runqueue().is_empty(prio) {
            schedule();

            // Check if the yielding thread can continue their execution on another
            // core that currently runs a lower priority thread.
            // This is only necessary when core-affinities are enabled, because only
            // then it is possible that a lower prio thread runs while a higher prio
            // runqueue isn't empty.
            #[cfg(feature = "core-affinity")]
            if _affinity == CoreAffinity::no_affinity() {
                threads.schedule_if_higher_prio(current_pid, prio);
            }
        }
    })
}

/// Suspends/ pauses the current thread's execution.
pub fn sleep() {
    THREADS.with(|threads| {
        let Some(pid) = threads.current_pid() else {
            return;
        };
        threads.set_state(pid, ThreadState::Paused);
    });
}

/// Wakes up a thread and adds it to the runqueue.
///
/// Returns `false` if no paused thread exists for `thread_id`.
pub fn wakeup(thread_id: ThreadId) -> bool {
    THREADS.with(|threads| {
        match threads.get_state(thread_id) {
            Some(ThreadState::Paused) => {}
            _ => return false,
        }
        threads.set_state(thread_id, ThreadState::Running);
        true
    })
}

/// Returns the priority of a thread.
///
/// Returns `None` if this is not a valid thread.
pub fn get_priority(thread_id: ThreadId) -> Option<RunqueueId> {
    THREADS.with(|threads| {
        if !threads.is_in_bound(thread_id) {
            return None;
        }
        let tcbs = threads.tcbs();
        let thread = tcbs.get_unchecked(thread_id);
        if thread.state == ThreadState::Invalid {
            return None;
        }
        Some(thread.prio)
    })
}

/// Changes the priority of a thread.
///
/// This might trigger a context switch.
pub fn set_priority(thread_id: ThreadId, prio: RunqueueId) {
    THREADS.with(|threads| threads.set_priority(thread_id, prio))
}

/// Returns the size of the internal structure that holds the
/// a thread's data.
pub fn thread_struct_size() -> usize {
    core::mem::size_of::<Thread>()
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_basic() {
        assert_eq!(1, 1);
    }
}
