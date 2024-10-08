#![allow(dead_code)]
#![allow(non_camel_case_types)]

use core::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use paste::paste;

use riot_rs_runqueue::{RunQueue, RunqueueId, ThreadId};

#[cfg(feature = "multicore")]
use crate::{core_id, CORES_NUMOF};
use crate::{
    sync::{Spinlock, SpinlockGuard, SpinlockGuardMut},
    thread::Thread,
    SCHED_PRIO_LEVELS, THREADS_NUMOF,
};

pub struct Threads {
    /// Global thread runqueue.
    runqueue: Spinlock<RunQueue<SCHED_PRIO_LEVELS, THREADS_NUMOF>, 1>,
    /// The actual TCBs.
    tcbs: Spinlock<TCBs, 2>,
    /// `Some` when a thread is blocking another thread due to conflicting
    /// resource access.
    thread_blocklist: Spinlock<[Option<ThreadId>; THREADS_NUMOF], 3>,

    /// The currently running thread(s).
    #[cfg(feature = "multicore")]
    current_threads: Spinlock<[Option<(ThreadId, RunqueueId)>; CORES_NUMOF], 4>,
    #[cfg(not(feature = "multicore"))]
    current_thread: Spinlock<Option<(ThreadId, RunqueueId)>, 4>,
}

impl Threads {
    pub const fn new() -> Self {
        Self {
            runqueue: Spinlock::new(RunQueue::new()),
            tcbs: Spinlock::new(TCBs::new()),
            thread_blocklist: Spinlock::new([const { None }; THREADS_NUMOF]),
            #[cfg(feature = "multicore")]
            current_threads: Spinlock::new([None; CORES_NUMOF]),
            #[cfg(not(feature = "multicore"))]
            current_thread: Spinlock::new(None),
        }
    }

    pub fn runqueue(&mut self) -> SpinlockGuard<RunQueue<SCHED_PRIO_LEVELS, THREADS_NUMOF>, 1> {
        self.runqueue.lock()
    }

    pub fn runqueue_mut(
        &mut self,
    ) -> SpinlockGuardMut<RunQueue<SCHED_PRIO_LEVELS, THREADS_NUMOF>, 1> {
        self.runqueue.lock_mut()
    }

    pub fn tcbs(&mut self) -> SpinlockGuard<TCBs, 2> {
        self.tcbs.lock()
    }

    pub fn tcbs_mut(&mut self) -> SpinlockGuardMut<TCBs, 2> {
        self.tcbs.lock_mut()
    }

    pub fn thread_blocklist(&mut self) -> SpinlockGuard<[Option<ThreadId>; THREADS_NUMOF], 3> {
        self.thread_blocklist.lock()
    }

    pub fn thread_blocklist_mut(
        &mut self,
    ) -> SpinlockGuardMut<[Option<ThreadId>; THREADS_NUMOF], 3> {
        self.thread_blocklist.lock_mut()
    }

    #[cfg(not(feature = "multicore"))]
    pub fn current_thread(&mut self) -> SpinlockGuard<Option<(ThreadId, RunqueueId)>, 4> {
        self.current_thread.lock()
    }

    #[cfg(not(feature = "multicore"))]
    pub fn current_thread_mut(&mut self) -> SpinlockGuardMut<Option<(ThreadId, RunqueueId)>, 4> {
        self.current_thread.lock_mut()
    }

    #[cfg(feature = "multicore")]
    pub fn current_threads(
        &mut self,
    ) -> SpinlockGuard<[Option<(ThreadId, RunqueueId)>; CORES_NUMOF], 4> {
        self.current_threads.lock()
    }

    #[cfg(feature = "multicore")]
    pub fn current_threads_mut(
        &mut self,
    ) -> SpinlockGuardMut<[Option<(ThreadId, RunqueueId)>; CORES_NUMOF], 4> {
        self.current_threads.lock_mut()
    }

    /// Returns the ID of the current thread, or [`None`] if no thread is currently
    /// running.
    ///
    /// On multicore, it returns the ID of the thread that is running on the
    /// current core.
    #[inline]
    pub fn current_pid(&self) -> Option<ThreadId> {
        #[cfg(feature = "multicore")]
        {
            self.current_threads.lock()[usize::from(core_id())]
                .unzip()
                .0
        }
        #[cfg(not(feature = "multicore"))]
        {
            self.current_thread.lock().unzip().0
        }
    }

    #[inline]
    pub fn current_pid_prio(&self) -> Option<(ThreadId, RunqueueId)> {
        #[cfg(feature = "multicore")]
        {
            self.current_threads.lock()[usize::from(core_id())]
        }
        #[cfg(not(feature = "multicore"))]
        {
            *self.current_thread.lock()
        }
    }

    /// Returns a mutable reference to the current thread ID, or [`None`]
    /// if no thread is currently running.
    ///
    /// On multicore, it refers to the ID of the thread that is running on the
    /// current core.
    #[allow(dead_code, reason = "used in scheduler implementation")]
    pub fn set_current_pid(&mut self, pid: ThreadId, prio: RunqueueId) {
        #[cfg(feature = "multicore")]
        {
            self.current_threads.lock_mut()[usize::from(core_id())] = Some((pid, prio))
        }
        #[cfg(not(feature = "multicore"))]
        {
            *self.current_thread.lock_mut() = Some((pid, prio))
        }
    }

    /// Returns a guard that allows to lock multiple properties at the same time using a
    /// builder pattern.
    /// The order in which the items have to be locked is:
    /// 1. runqueue
    /// 2. tcbs
    /// 3. thread-blocklist
    /// 4. current-thread(s)
    ///
    /// Only the items that are used have to be blocked.
    pub fn guard(&mut self) -> Guard<Empty> {
        Guard {
            t: self,
            inner: Empty { inner: PhantomData },
        }
    }
}

pub struct Guard<'a, TInner> {
    t: &'a Threads,
    inner: TInner,
}

impl<'a, TInner> Guard<'a, TInner> {
    pub fn release(self) {}
}

impl<'a, TInner> Deref for Guard<'a, TInner> {
    type Target = TInner;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a, TInner> DerefMut for Guard<'a, TInner> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

pub struct WithTempty<'a, TInner> {
    inner: PhantomData<&'a TInner>,
}

type Empty<'a> = WithTempty<'a, ()>;

macro_rules! access_multiple {
    ($i:literal struct => empty: $prop_ty:ty) => { };
    ($i:literal struct $($mut:ident)? => $prop:ident: $prop_ty:ty) => {
        paste! {
            pub struct [<With $($mut)? T $prop>]<'a, TInner> {
                pub $prop: [<SpinlockGuard $($mut)?>]<'a, $prop_ty, $i>,
                inner: TInner
            }
            impl<'a, TInner> Deref for [<With $($mut)? T $prop>]<'a, TInner> {
                type Target = TInner;
                fn deref(&self) -> &Self::Target {
                    &self.inner
                }
            }
            impl<'a, TInner> DerefMut for [<With $($mut)? T $prop>]<'a, TInner> {
                fn deref_mut(&mut self) -> &mut Self::Target {
                    &mut self.inner
                }
            }
        }
    };
    (impl $($mut:ident)? => $prop:ident: $prop_ty:ty, $($n_prop:ident),*) => {
        paste! {
            impl<'a, TInner> Guard<'a, [<With $($mut)? T $prop>]<'a, TInner>> {
            $(
                pub fn [<with_ $n_prop>](self) -> Guard<'a, [<With T $n_prop>]<'a,  <Self as Deref>::Target>> {
                    let Self { inner, t } = self;
                    let $n_prop = t.$n_prop.lock();
                    Guard { t, inner: [<With T $n_prop>]{ $n_prop, inner} }
                }
                pub fn [<with_mut_ $n_prop>](self) -> Guard<'a, [<WithMut T $n_prop>]<'a, <Self as Deref>::Target>> {
                    let Self { inner, t } = self;
                    let $n_prop = t.$n_prop.lock_mut();
                    Guard { t, inner: [<WithMut T $n_prop>]{ $n_prop, inner} }
                }
            )*
            }
        }
    };
    ($i:literal => $prop:ident: $prop_ty:ty) => {
        access_multiple!{ $i struct => $prop: $prop_ty }
        access_multiple!{ $i struct Mut => $prop: $prop_ty }
    };
    ($i:literal => $prop:ident: $prop_ty:ty, $($n_i:literal => $n_prop:ident: $n_prop_ty:ty),*) => {
        access_multiple!{ $i struct => $prop: $prop_ty }
        access_multiple!{ $i struct Mut => $prop: $prop_ty }
        access_multiple!{ impl => $prop: $prop_ty, $($n_prop),* }
        access_multiple!{ impl Mut => $prop: $prop_ty, $($n_prop),* }
        access_multiple!{ $($n_i => $n_prop: $n_prop_ty),* }
    }
}

#[cfg(not(feature = "multicore"))]
access_multiple! {
    0 => empty: (),
    1 => runqueue: RunQueue<SCHED_PRIO_LEVELS, THREADS_NUMOF>,
    2 => tcbs: TCBs,
    3 => thread_blocklist: [Option<ThreadId>; THREADS_NUMOF],
    4 => current_thread: Option<(ThreadId, RunqueueId)>
}

#[cfg(feature = "multicore")]
access_multiple! {
    0 => empty: (),
    1 => runqueue: RunQueue<SCHED_PRIO_LEVELS, THREADS_NUMOF>,
    2 => tcbs: TCBs,
    3 => thread_blocklist: [Option<ThreadId>; THREADS_NUMOF],
    4 => current_threads: [Option<(ThreadId, RunqueueId)>; CORES_NUMOF]
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
}
