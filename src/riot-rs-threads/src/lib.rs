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
pub use thread_flags as flags;

#[cfg(feature = "core-affinity")]
pub use smp::CoreAffinity;

#[doc(hidden)]
pub use arch::schedule;

use arch::{Arch, Cpu, ThreadData};
use ensure_once::EnsureOnce;
use riot_rs_runqueue::RunQueue;
use smp::{global_cs_with, schedule_on_core, Multicore, MulticoreLock};
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
    runqueue: MulticoreLock<RunQueue<SCHED_PRIO_LEVELS, THREADS_NUMOF>>,
    /// The actual TCBs.
    threads: [MulticoreLock<Thread>; THREADS_NUMOF],
    /// `Some` when a thread is blocking another thread due to conflicting
    /// resource access.
    thread_blocklist: MulticoreLock<[Option<ThreadId>; THREADS_NUMOF]>,
    /// The currently running thread.
    current_threads: MulticoreLock<[Option<(ThreadId, RunqueueId)>; CORES_NUMOF]>,
}

impl Threads {
    const fn new() -> Self {
        Self {
            runqueue: MulticoreLock::new(RunQueue::new()),
            current_threads: MulticoreLock::new([None; CORES_NUMOF]),
            thread_blocklist: MulticoreLock::new([const { None }; THREADS_NUMOF]),
            threads: [const { MulticoreLock::new(Thread::default()) }; THREADS_NUMOF],
        }
    }

    fn runqueue_with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut RunQueue<SCHED_PRIO_LEVELS, THREADS_NUMOF>) -> R,
    {
        let lock_id = 1 % smp::Chip::SPINLOCKS as usize;
        self.runqueue.with(lock_id, f)
    }

    fn current_threads_with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut [Option<(ThreadId, RunqueueId)>; CORES_NUMOF]) -> R,
    {
        let lock_id = 2 % smp::Chip::SPINLOCKS as usize;
        self.current_threads.with(lock_id, f)
    }

    fn thread_blocklist_with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut [Option<ThreadId>; THREADS_NUMOF]) -> R,
    {
        let lock_id = 3 % smp::Chip::SPINLOCKS as usize;
        self.thread_blocklist.with(lock_id, f)
    }

    /// Provides mutable access to the current thread.
    fn current_with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(Option<&mut Thread>) -> R,
    {
        let current = self.current_threads_with(|ct| ct[usize::from(core_id())]);
        let pid = match current {
            Some((pid, _)) => pid,
            None => return f(None),
        };
        self.threads[usize::from(pid)].with(lock_id(pid), |thread| f(Some(thread)))
    }

    fn current_pid(&self) -> Option<ThreadId> {
        self.current_pid_prio().map(|(pid, _)| pid)
    }

    fn current_pid_prio(&self) -> Option<(ThreadId, RunqueueId)> {
        self.current_threads_with(|ct| ct[usize::from(core_id())])
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
    ) -> Option<ThreadId> {
        let pid = self.get_unused()?;
        self.get_unchecked_with(pid, |thread| {
            Cpu::setup_stack(thread, stack, func, arg);
            thread.pid = pid;
            thread.prio = prio;
            #[cfg(feature = "core-affinity")]
            {
                thread.core_affinity = _core_affinity.unwrap_or_default();
            }
            Some(thread.pid)
        })
    }

    /// Provides mutable access to any thread data.
    ///
    /// # Panics
    ///
    /// Panics if `thread_id` is >= [`THREADS_NUMOF`].
    /// If the thread for this `thread_id` is in an invalid state, the
    /// data in the returned [`Thread`] is undefined, i.e. empty or outdated.
    fn get_unchecked_with<F, R>(&self, thread_id: ThreadId, f: F) -> R
    where
        F: FnOnce(&mut Thread) -> R,
    {
        self.threads[usize::from(thread_id)].with(lock_id(thread_id), |thread| f(thread))
    }

    /// Returns an unused ThreadId / Thread slot.
    fn get_unused(&self) -> Option<ThreadId> {
        for i in 0..THREADS_NUMOF {
            if self.threads[i].with(lock_id(i), |t| t.state) == ThreadState::Invalid {
                return Some(ThreadId::new(i as u8));
            }
        }
        None
    }

    /// Checks if a thread with valid state exists for this `thread_id`.
    fn is_valid_pid(&self, thread_id: ThreadId) -> bool {
        if usize::from(thread_id) >= THREADS_NUMOF {
            false
        } else {
            self.threads[usize::from(thread_id)].with(lock_id(thread_id), |thread| {
                thread.state != ThreadState::Invalid
            })
        }
    }

    /// Sets the state of a thread.
    ///
    /// # Panics
    ///
    /// Panics if `pid` is >= [`THREADS_NUMOF`].
    fn set_state(&self, pid: ThreadId, new_state: ThreadState) -> ThreadState {
        let (prio, old_state) = self.get_unchecked_with(pid, |thread| {
            let old_state = core::mem::replace(&mut thread.state, new_state);
            (thread.prio, old_state)
        });
        match (new_state, old_state) {
            (new, old) if new == old => {}
            (ThreadState::Running, ThreadState::Invalid) => {
                self.runqueue_with(|rq| rq.add(pid, prio));
                // We are in the startup phase were all threads are created.
                // Don't trigger the scheduler before `start_threading`.
                // FIXME: threads aren't only created during startup, so
                // we need to find another fix for thos.
            }
            (ThreadState::Running, _) => {
                self.runqueue_with(|rq| rq.add(pid, prio));
                self.schedule_if_needed(pid, prio)
            }
            (_, ThreadState::Running) => schedule(),
            _ => {}
        }
        old_state
    }

    fn get_priority(&self, thread_id: ThreadId) -> Option<RunqueueId> {
        if usize::from(thread_id) >= THREADS_NUMOF {
            return None;
        }
        self.get_unchecked_with(thread_id, |thread| {
            if thread.state == ThreadState::Invalid {
                return None;
            }
            Some(thread.prio)
        })
    }

    /// Change the priority of a thread.
    fn set_priority(&self, thread_id: ThreadId, new_prio: RunqueueId) {
        if usize::from(thread_id) >= THREADS_NUMOF {
            return;
        }
        let Some(old_prio) = self.get_unchecked_with(thread_id, |thread| {
            let old_prio = thread.prio;
            if old_prio == new_prio {
                return None;
            }
            thread.prio = new_prio;
            if thread.state != ThreadState::Running {
                return None;
            }
            Some(old_prio)
        }) else {
            return;
        };
        let running_on_core = self.current_threads_with(|ct| {
            ct.iter()
                .position(|t| t.is_some_and(|(pid, _)| pid == thread_id))
        });

        if running_on_core.is_none() {
            self.runqueue_with(|rq| {
                rq.del(thread_id);
                rq.add(thread_id, new_prio);
            });
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
            let affinity = self.get_unchecked_with(_thread_id, |t| t.core_affinity);
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
        self.current_threads_with(|ct| {
            ct.iter()
                .enumerate()
                .filter_map(|(core, thread)| {
                    let rq = thread.unzip().1.unwrap_or(RunqueueId::new(0));
                    Some((CoreId::new(core as u8), rq))
                })
                .min_by_key(|(_, rq)| *rq)
                .unwrap()
        })
    }

    #[cfg(feature = "core-affinity")]
    fn lowest_running_prio(&self, affinity: &CoreAffinity) -> (CoreId, RunqueueId) {
        self.current_threads_with(|ct| {
            ct.iter()
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
        })
    }

    #[cfg(feature = "core-affinity")]
    fn is_affine_to_curr_core(&self, pid: ThreadId) -> bool {
        self.get_unchecked_with(pid, |t| t.core_affinity.contains(crate::core_id()))
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

fn lock_id<ID: Into<usize>>(pid: ID) -> usize {
    let offset = match smp::Chip::SPINLOCKS {
        // Single MulticoreLock that is used for all shared objects.
        1 => 0,
        // Distribute among all available spinlocks apart from MulticoreLock 1, which is used
        // as global critical section.
        2..4 => 1,
        // MulticoreLock 0 is reserved for our global section.
        // Spinlocks 1-3 for runqueue, current_threads and blocklist.
        // So distribute threads among spinlocks 4..smp::Chip::SPINLOCKS.
        _ => 4,
    };
    let n_thread_spinlocks = smp::Chip::SPINLOCKS as usize - offset;
    offset + (pid.into() % n_thread_spinlocks)
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
    THREADS.with(|threads| {
        let prio = RunqueueId::new(prio);
        let thread_id = threads
            .create(func, arg, stack, prio, core_affinity)
            .unwrap();
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
    THREADS.with(|threads| {
        let thread_id = threads.current_pid().unwrap();
        threads.set_state(thread_id, ThreadState::Invalid);
    });

    unreachable!();
}

/// "Yields" to another thread with the same priority.
pub fn yield_same() {
    if THREADS.with(|threads| {
        let Some(prio) = threads.current_with(|t| Some(t?.prio)) else {
            return false;
        };
        threads.runqueue_with(|rq| !rq.is_empty(prio))
    }) {
        schedule();
    }
}

/// Suspends/ pauses the current thread's execution.
pub fn sleep() {
    THREADS.with(|threads| {
        let pid = threads.current_pid().unwrap();
        threads.set_state(pid, ThreadState::Paused);
    });
}

/// Wakes up a thread and adds it to the runqueue.
///
/// Returns `false` if no paused thread exists for `thread_id`.
pub fn wakeup(thread_id: ThreadId) -> bool {
    THREADS.with(|threads| {
        if usize::from(thread_id) >= THREADS_NUMOF {
            return false;
        }
        if !threads.get_unchecked_with(thread_id, |t| t.state == ThreadState::Paused) {
            return false;
        }
        threads.set_state(thread_id, ThreadState::Running);
        true
    })
}

/// Get the priority of a thread.
///
/// Returns `None` if this is not a valid thread.
pub fn get_priority(thread_id: ThreadId) -> Option<u8> {
    THREADS.with(|threads| threads.get_priority(thread_id).map(|rq| *rq))
}

/// Change the priority of a thread.
///
/// This might trigger a context switch.
pub fn set_priority(thread_id: ThreadId, prio: u8) {
    THREADS.with(|threads| threads.set_priority(thread_id, RunqueueId::new(prio)))
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
