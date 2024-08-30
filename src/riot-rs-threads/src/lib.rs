#![cfg_attr(not(test), no_std)]
#![feature(naked_functions)]
#![feature(used_with_arg)]
#![feature(type_alias_impl_trait)]
// Disable indexing lints for now, possible panics are documented or rely on internally-enforced
// invariants
#![allow(clippy::indexing_slicing)]

mod arch;
mod autostart_thread;
mod ensure_once;
mod smp;
mod thread;
mod threadlist;

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
pub use smp::CoreId;
use sync::{Spinlock, SpinlockGuard, SpinlockGuardMut};
pub use thread_flags as flags;

#[cfg(feature = "core-affinity")]
pub use smp::CoreAffinity;

#[doc(hidden)]
pub use arch::schedule;

use arch::{Arch, Cpu, ThreadData};
use ensure_once::EnsureOnce;
use riot_rs_runqueue::RunQueue;
use smp::{cs_with, schedule_on_core, Multicore};
use thread::{Thread, ThreadState};

#[cfg(not(feature = "core-affinity"))]
use smp::CoreAffinity;

/// a global defining the number of possible priority levels
pub const SCHED_PRIO_LEVELS: usize = 12;

/// a global defining the number of threads that can be created
pub const THREADS_NUMOF: usize = 16;

pub const CORES_NUMOF: usize = smp::Chip::CORES as usize;

static THREADS: EnsureOnce<Threads> = EnsureOnce::new(Threads::new());

pub type ThreadFn = fn();

#[linkme::distributed_slice]
pub static THREAD_FNS: [ThreadFn] = [..];

/// Struct holding all scheduler state
struct Threads {
    /// Global thread runqueue.
    runqueue: Spinlock<RunQueue<SCHED_PRIO_LEVELS, THREADS_NUMOF>>,
    /// The actual TCBs.
    threads: [Spinlock<Thread>; THREADS_NUMOF],
    /// `Some` when a thread is blocking another thread due to conflicting
    /// resource access.
    thread_blocklist: [Spinlock<Option<ThreadId>>; THREADS_NUMOF],
    /// The currently running thread.
    current_threads: Spinlock<[Option<(ThreadId, RunqueueId)>; CORES_NUMOF]>,
}

impl Threads {
    const fn new() -> Self {
        Self {
            runqueue: Spinlock::new(RunQueue::new()),
            threads: [const { Spinlock::new(Thread::default()) }; THREADS_NUMOF],
            thread_blocklist: [const { Spinlock::new(None) }; THREADS_NUMOF],
            current_threads: Spinlock::new([None; CORES_NUMOF]),
        }
    }

    // pub(crate) fn by_pid_unckecked(&self, thread_id: ThreadId) -> &mut Thread {
    //     &self.threads[thread_id as usize]
    // }

    /// Returns checked mutable access to the thread data of the currently
    /// running thread.
    ///
    /// Returns `None` if there is no current thread.
    fn current(&self) -> Option<SpinlockGuardMut<Thread>> {
        self.current_threads.acquire()[usize::from(core_id())]
            .map(|(pid, _)| self.threads[usize::from(pid)].acquire_mut())
    }

    fn current_pid(&self) -> Option<ThreadId> {
        self.current_threads.acquire()[usize::from(core_id())].map(|(id, _)| id)
    }

    #[allow(dead_code)]
    fn set_current(&self, pid: ThreadId, prio: RunqueueId) {
        self.current_threads.acquire_mut()[usize::from(core_id())] = Some((pid, prio))
    }

    /// Creates a new thread.
    ///
    /// This sets up the stack and TCB for this thread.
    ///
    /// Returns `None` if there is no free thread slot.
    fn create(
        &self,
        func: usize,
        arg: usize,
        stack: &'static mut [u8],
        prio: RunqueueId,
        _core_affinity: Option<CoreAffinity>,
    ) -> Option<SpinlockGuardMut<Thread>> {
        if let Some((mut thread, pid)) = self.get_unused() {
            Cpu::setup_stack(&mut *thread, stack, func, arg);
            thread.prio = prio;
            thread.pid = pid;
            #[cfg(feature = "core-affinity")]
            {
                thread.core_affinity = _core_affinity.unwrap_or_default();
            }

            Some(thread)
        } else {
            None
        }
    }

    /// Returns access to any thread data.
    ///
    /// # Panics
    ///
    /// Panics if `thread_id` is >= [`THREADS_NUMOF`].
    /// If the thread for this `thread_id` is in an invalid state, the
    /// data in the returned [`Thread`] is undefined, i.e. empty or outdated.
    fn get_unchecked(&self, thread_id: ThreadId) -> SpinlockGuard<Thread> {
        self.threads[usize::from(thread_id)].acquire()
    }

    /// Returns mutable access to any thread data.
    ///
    /// # Panics
    ///
    /// Panics if `thread_id` is >= [`THREADS_NUMOF`].
    /// If the thread for this `thread_id` is in an invalid state, the
    /// data in the returned [`Thread`] is undefined, i.e. empty or outdated.
    fn get_unchecked_mut(&self, thread_id: ThreadId) -> SpinlockGuardMut<Thread> {
        self.threads[usize::from(thread_id)].acquire_mut()
    }

    /// Returns an unused ThreadId / Thread slot.
    fn get_unused(&self) -> Option<(SpinlockGuardMut<Thread>, ThreadId)> {
        for i in 0..THREADS_NUMOF {
            let thread = self.threads[i].acquire_mut();
            if thread.state == ThreadState::Invalid {
                return Some((thread, ThreadId::new(i as u8)));
            }
        }
        None
    }

    /// Checks if a thread with valid state exists for this `thread_id`.
    fn is_valid_pid(&self, thread_id: ThreadId) -> bool {
        if usize::from(thread_id) >= THREADS_NUMOF {
            false
        } else {
            self.threads[usize::from(thread_id)].acquire().state != ThreadState::Invalid
        }
    }

    /// Sets the state of a thread.
    ///
    /// # Panics
    ///
    /// Panics if `pid` is >= [`THREADS_NUMOF`].
    fn set_state(&self, pid: ThreadId, new_state: ThreadState) -> ThreadState {
        let mut thread = self.get_unchecked_mut(pid);
        let old_state = core::mem::replace(&mut thread.state, new_state);
        let prio = thread.prio;
        drop(thread);
        match (new_state, old_state) {
            (new, old) if new == old => {}
            (ThreadState::Running, ThreadState::Invalid) => {
                self.runqueue.acquire_mut().add(pid, prio);
                // We are in the startup phase were all threads are created.
                // Don't trigger the scheduler before `start_threading`.
                // FIXME: threads aren't only created during startup, so
                // we need to find another fix for thos.
            }
            (ThreadState::Running, _) => {
                self.runqueue.acquire_mut().add(pid, prio);
                self.schedule_if_needed(pid, prio)
            }
            (_, ThreadState::Running) => schedule(),
            _ => {}
        }
        old_state
    }

    fn get_priority(&self, thread_id: ThreadId) -> Option<RunqueueId> {
        self.is_valid_pid(thread_id)
            .then(|| self.get_unchecked(thread_id).prio)
    }

    /// Change the priority of a thread.
    fn set_priority(&self, thread_id: ThreadId, new_prio: RunqueueId) {
        if !self.is_valid_pid(thread_id) {
            return;
        }
        let mut thread = self.get_unchecked_mut(thread_id);
        let old_prio = thread.prio;
        if old_prio == new_prio {
            return;
        }
        thread.prio = new_prio;
        if thread.state != ThreadState::Running {
            return;
        }
        drop(thread);

        let current_threads = self.current_threads.acquire();
        let running_on_core = current_threads
            .iter()
            .position(|t| t.is_some_and(|(pid, _)| pid == thread_id));
        drop(current_threads);

        if running_on_core.is_none() {
            let mut runqueue = self.runqueue.acquire_mut();
            runqueue.del(thread_id);
            runqueue.add(thread_id, new_prio);
        }

        match running_on_core {
            Some(running_core) if new_prio < old_prio => {
                // Another thread might have higher prio now, so trigger the scheduler.
                schedule_on_core(CoreId::new(running_core as u8));
            }
            None if new_prio > old_prio => self.schedule_if_needed(thread_id, new_prio),
            _ => {}
        }
    }

    fn schedule_if_needed(&self, _thread_id: ThreadId, prio: RunqueueId) {
        #[cfg(feature = "core-affinity")]
        let (core, lowest_prio) = {
            let thread = self.get_unchecked(_thread_id);
            let affinity = thread.core_affinity;
            drop(thread);
            self.lowest_running_prio(&affinity)
        };
        #[cfg(not(feature = "core-affinity"))]
        let (core, lowest_prio) = self.lowest_running_prio();
        if lowest_prio <= prio {
            schedule_on_core(core);
        }
    }

    #[cfg(not(feature = "core-affinity"))]
    fn lowest_running_prio(&self) -> (CoreId, RunqueueId) {
        self.current_threads
            .acquire()
            .iter()
            .enumerate()
            .filter_map(|(core, thread)| {
                let rq = thread.unzip().1.unwrap_or(RunqueueId::new(0));
                Some((CoreId::new(core as u8), rq))
            })
            .min_by_key(|(_, rq)| *rq)
            .unwrap()
    }

    #[cfg(feature = "core-affinity")]
    fn lowest_running_prio(&self, affinity: &CoreAffinity) -> (CoreId, RunqueueId) {
        self.current_threads
            .acquire()
            .iter()
            .enumerate()
            .filter_map(|(core, thread)| {
                let core = CoreId::new(core as u8);
                if !affinity.contains(core) {
                    return None;
                }
                let rq = thread.unzip().1.unwrap_or(RunqueueId::new(0));
                Some((core, rq))
            })
            .min_by_key(|(_, rq)| *rq)
            .unwrap()
    }

    #[cfg(feature = "core-affinity")]
    fn is_affine_to_curr_core(&self, pid: ThreadId) -> bool {
        self.get_unchecked(pid)
            .core_affinity
            .contains(crate::core_id())
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
    smp::Chip::startup_cores();
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
/// only use when you know what you are doing.
pub unsafe fn thread_create_raw(
    func: usize,
    arg: usize,
    stack: &'static mut [u8],
    prio: u8,
    core_affinity: Option<CoreAffinity>,
) -> ThreadId {
    THREADS.with_mut(|threads| {
        let prio = RunqueueId::new(prio);
        let thread = threads.create(func, arg, stack, prio, core_affinity);
        let thread_id = thread.unwrap().pid;
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
    smp::Chip::core_id()
}

/// Checks if a given [`ThreadId`] is valid
pub fn is_valid_pid(thread_id: ThreadId) -> bool {
    THREADS.with(|threads| threads.is_valid_pid(thread_id))
}

/// Thread cleanup function.
///
/// This gets hooked into a newly created thread stack so it gets called when
/// the thread function returns.
#[allow(unused)]
fn cleanup() -> ! {
    THREADS.with_mut(|threads| {
        let thread_id = threads.current_pid().unwrap();
        threads.set_state(thread_id, ThreadState::Invalid);
    });

    unreachable!();
}

/// "Yields" to another thread with the same priority.
pub fn yield_same() {
    if THREADS.with_mut(|threads| {
        let Some(rq) = threads.current().map(|t| t.prio) else {
            return false;
        };
        !threads.runqueue.acquire().is_empty(rq)
    }) {
        schedule();
    }
}

/// Suspends/ pauses the current thread's execution.
pub fn sleep() {
    THREADS.with_mut(|threads| {
        let pid = threads.current_pid().unwrap();
        threads.set_state(pid, ThreadState::Paused);
    });
}

/// Wakes up a thread and adds it to the runqueue.
///
/// Returns `false` if no paused thread exists for `thread_id`.
pub fn wakeup(thread_id: ThreadId) -> bool {
    THREADS.with_mut(|threads| {
        if usize::from(thread_id) >= THREADS_NUMOF {
            return false;
        }
        let thread = threads.threads[usize::from(thread_id)].acquire();
        if thread.state != ThreadState::Paused {
            return false;
        }
        drop(thread);
        threads.set_state(thread_id, ThreadState::Running);
        true
    })
}

/// Get the priority of a thread.
///
/// Returns `None` if this is not a valid thread.
pub fn get_priority(thread_id: ThreadId) -> Option<u8> {
    THREADS.with_mut(|threads| threads.get_priority(thread_id).map(|rq| *rq))
}

/// Change the priority of a thread.
///
/// This might trigger a context switch.
pub fn set_priority(thread_id: ThreadId, prio: u8) {
    THREADS.with_mut(|threads| threads.set_priority(thread_id, RunqueueId::new(prio)))
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
