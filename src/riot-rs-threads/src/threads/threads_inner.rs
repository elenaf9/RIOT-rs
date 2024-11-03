use paste::paste;

use super::{BlockList, CurrentThreads, RunQueue, TCBs};
use crate::{
    sync::{Spinlock, SpinlockGuard},
    THREADS_NUMOF,
};

pub struct ThreadsInner {
    /// The actual TCBs.
    tcbs: Spinlock<TCBs, 1>,
    /// Global thread runqueue.
    current_threads: Spinlock<CurrentThreads, 2>,
    /// `Some` when a thread is blocking another thread due to conflicting
    runqueue: Spinlock<RunQueue, 3>,
    /// The currently running thread(s).
    /// resource access.
    thread_blocklist: Spinlock<BlockList, 4>,
}

impl ThreadsInner {
    pub const fn new() -> Self {
        Self {
            runqueue: Spinlock::new_internal(RunQueue::new()),
            tcbs: Spinlock::new_internal(TCBs::new()),
            thread_blocklist: Spinlock::new_internal([const { None }; THREADS_NUMOF]),
            current_threads: Spinlock::new_internal(CurrentThreads::new()),
        }
    }
}

macro_rules! access_multiple {
    ($i:literal => $prop:ident: $prop_ty:ty $(, $($n_i:literal => $n_prop:ident: $n_prop_ty:ty),*)?) => { paste! {
        pub struct [<With $prop_ty>]<'a> {
            threads: &'a ThreadsInner,
        }
        impl ThreadsInner {
            pub fn $prop(&mut self) -> SpinlockGuard<$prop_ty, $i> {
                self.$prop.lock()
            }
            pub fn [<with_ $prop>](&mut self) -> ( [<With $prop_ty>], SpinlockGuard<$prop_ty, $i>) {
                ([<With $prop_ty>] {threads: self }, self.$prop.lock())
            }
        }
        $(
            impl<'a> [<With $prop_ty>]<'a> {
                $(
                    pub fn $n_prop(&mut self) -> SpinlockGuard<$n_prop_ty, $n_i> {
                        self.threads.$n_prop.lock()
                    }
                    pub fn [<with_ $n_prop>](&mut self) -> ( [<With $n_prop_ty>], SpinlockGuard<$n_prop_ty, $n_i>) {
                        let $n_prop = self.threads.$n_prop.lock();
                        ([<With $n_prop_ty>] {threads: self.threads }, $n_prop )
                    }
                )*
            }
            access_multiple!{ $($n_i => $n_prop: $n_prop_ty),* }
        )?
    }};
}

access_multiple! {
    1 => tcbs: TCBs,
    2 => current_threads: CurrentThreads,
    3 => runqueue: RunQueue,
    4 => thread_blocklist: BlockList
}
