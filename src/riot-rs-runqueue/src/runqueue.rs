// Disable indexing lints for now
#![allow(clippy::indexing_slicing)]

use core::mem;

use crate::clist::CList;

const USIZE_BITS: usize = mem::size_of::<usize>() * 8;

/// Runqueue number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct RunqueueId(u8);

impl RunqueueId {
    pub const fn new(value: u8) -> Self {
        Self(value)
    }
}

impl From<RunqueueId> for usize {
    fn from(value: RunqueueId) -> Self {
        usize::from(value.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ThreadId(u8);

impl ThreadId {
    pub const fn new(value: u8) -> Self {
        Self(value)
    }
}

impl From<ThreadId> for usize {
    fn from(value: ThreadId) -> Self {
        usize::from(value.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct CoreId(u8);

impl CoreId {
    pub const fn new(value: u8) -> Self {
        Self(value)
    }
}

impl From<CoreId> for usize {
    fn from(value: CoreId) -> Self {
        usize::from(value.0)
    }
}

trait FromBitmap: Sized {
    fn from_bitmap(bitmap: usize) -> Option<Self>;
}
impl FromBitmap for u8 {
    fn from_bitmap(bitmap: usize) -> Option<Self> {
        if bitmap == 0 {
            return None;
        }
        Some(ffs(bitmap) as u8 - 1)
    }
}

/// Runqueue for `N_QUEUES`, supporting `N_THREADS` total.
///
/// Assumptions:
/// - runqueue numbers (corresponding priorities) are 0..N_QUEUES (exclusive)
/// - higher runqueue number ([`RunqueueId`]) means higher priority
/// - runqueue numbers fit in usize bits (supporting max 32 priority levels)
/// - [`ThreadId`]s range from 0..N_THREADS
/// - `N_THREADS` is <255 (as u8 is used to store them, but 0xFF is used as
///   special value)
///
/// The current implementation needs an usize for the bit cache,
/// an `[RunqueueId; N_QUEUES]` array for the list tail indexes
/// and an `[ThreadId; N_THREADS]` for the list next indexes.
pub struct RunQueue<const N_QUEUES: usize, const N_THREADS: usize, const N_CORES: usize = 1> {
    /// Bitcache that represents the currently used queues
    /// in `0..N_QUEUES`.
    bitcache: usize,
    queues: CList<N_QUEUES, N_THREADS>,
    next: [(bool, ThreadId); N_CORES],
}

impl<const N_QUEUES: usize, const N_THREADS: usize, const N_CORES: usize>
    RunQueue<N_QUEUES, N_THREADS, N_CORES>
{
    // NOTE: we don't impl Default here because hax does not support it yet. When it does, we
    // should impl it.
    #[allow(clippy::new_without_default)]
    pub const fn new() -> RunQueue<N_QUEUES, N_THREADS, N_CORES> {
        // unfortunately we cannot assert!() on N_QUEUES and N_THREADS,
        // as panics in const fn's are not (yet) implemented.
        RunQueue {
            bitcache: 0,
            queues: CList::new(),
            next: [(false, ThreadId::new(N_THREADS as u8)); N_CORES],
        }
    }

    /// Returns the `n` highest priority threads in the [`RunQueue`].
    ///
    /// This iterates through all non-empty runqueues with descending
    /// priority, until `N_CORES` threads have been found or all
    /// queues have been checked.
    ///
    /// Complexity is O(n).
    fn get_next_n(&self) -> [Option<ThreadId>; N_CORES] {
        let mut next_list = [None; N_CORES];
        let mut bitcache = self.bitcache;
        // Get head from highest priority queue.
        let mut head = match self.peek_head(bitcache) {
            Some(head) => {
                next_list[0] = Some(head);
                head.0
            }
            None => return next_list,
        };
        let mut thread = head;
        // Iterate through threads in the queue.
        for i in 1..N_CORES {
            thread = self.queues.peek_next(thread);
            if thread == head {
                // Switch to next runqueue.
                bitcache &= !(1 << (ffs(bitcache) - 1));
                head = match self.peek_head(bitcache) {
                    Some(h) => h.0,
                    // Early return instead of break, to make hax happy.
                    None => return next_list,
                };
                thread = head;
            };
            next_list[i] = Some(ThreadId(thread));
        }
        next_list
    }

    fn peek_head(&self, bitcache: usize) -> Option<ThreadId> {
        // Switch to highest priority runqueue remaining
        // in the bitcache.
        let rq = match u8::from_bitmap(bitcache) {
            Some(rq) => rq,
            None => return None,
        };
        self.queues.peek_head(rq).map(ThreadId)
    }
}

pub trait GlobalRunqueue<const N_QUEUES: usize, const N_THREADS: usize, const N_CORES: usize> {
    /// Adds thread with pid `n` to runqueue number `rq`.
    ///
    /// Returns a [`CoreId`] if the allocation for this core changed.
    ///
    fn add(&mut self, n: ThreadId, rq: RunqueueId) -> Option<CoreId>;

    /// Removes thread with pid `n` from runqueue number `rq`.
    ///
    /// Returns a [`CoreId`] if the allocation for this core changed.
    ///
    /// # Panics
    ///
    /// Panics for `N_CORES == 1`` if `n` is not the queue's head.
    /// This is fine, RIOT-rs only ever calls `del()` for the current thread.
    fn del(&mut self, n: ThreadId, rq: RunqueueId) -> Option<CoreId>;

    /// Advances from thread `n` in runqueue number `rq`.
    ///
    /// This is used to "yield" to another thread of *the same* priority.
    ///
    /// Returns a [`CoreId`] if the allocation for this core changed.
    ///
    /// **Warning: If `n` it not head if the run queue, this changes
    /// the order of the queue because the thread is moved to the
    /// tail.**
    fn advance(&mut self, n: ThreadId, rq: RunqueueId) -> Option<CoreId>;

    /// Update `self.next` so that the highest `N_CORES` threads
    /// are allocated.
    ///
    /// This only changes allocations if a thread was previously allocated
    /// and is now not part of the new list anymore, or the other way around.
    /// It assumes that there was maximum one change in the runqueue since the
    /// last reallocation (only one add/ delete or a runqueue advancement)!
    ///
    /// Returns a [`CoreId`] if the allocation for this core changed.
    fn reallocate(&mut self) -> Option<CoreId>;

    /// Returns the next thread that should run on this core.
    fn get_next(&self, core: CoreId) -> Option<ThreadId>;
}

impl<const N_QUEUES: usize, const N_THREADS: usize, const N_CORES: usize>
    GlobalRunqueue<N_QUEUES, N_THREADS, N_CORES> for RunQueue<N_QUEUES, N_THREADS, N_CORES>
{
    default fn add(&mut self, n: ThreadId, rq: RunqueueId) -> Option<CoreId> {
        debug_assert!((n.0 as usize) < N_THREADS);
        debug_assert!((rq.0 as usize) < N_QUEUES);
        self.bitcache |= 1 << rq.0;
        self.queues.push(n.0, rq.0);
        self.reallocate()
    }

    default fn del(&mut self, n: ThreadId, rq: RunqueueId) -> Option<CoreId> {
        debug_assert!((n.0 as usize) < N_THREADS);
        debug_assert!((rq.0 as usize) < N_QUEUES);

        if self.queues.peek_head(rq.0) == Some(n.0) {
            let popped = self.queues.pop_head(rq.0);
            assert_eq!(popped, Some(n.0));
        } else {
            self.queues.del(n.0, rq.0);
        }

        if self.queues.is_empty(rq.0) {
            self.bitcache &= !(1 << rq.0);
        }
        self.reallocate()
    }

    default fn advance(&mut self, n: ThreadId, rq: RunqueueId) -> Option<CoreId> {
        debug_assert!((rq.0 as usize) < N_QUEUES);
        if Some(n.0) == self.queues.peek_head(rq.0) {
            self.queues.advance(rq.0);
        } else {
            // If the thread is not the head remove it
            // from queue and re-insert it at tail.
            self.queues.del(n.0, rq.0);
            self.queues.push(n.0, rq.0);
        }
        self.reallocate()
    }

    default fn reallocate(&mut self) -> Option<CoreId> {
        let next = self.get_next_n();
        let mut bitmap_next = 0;
        let mut bitmap_allocated = 0;
        let mut bitmap_prev = 0;
        for i in 0..N_CORES {
            if let Some(id) = next[i] {
                bitmap_next |= 1 << id.0
            }
            let (is_running, id) = self.next[i];
            bitmap_prev |= 1 << id.0;

            if is_running {
                bitmap_allocated |= 1 << id.0
            }
        }
        if bitmap_next == bitmap_allocated {
            return None;
        }
        let diff = bitmap_next ^ bitmap_allocated;

        // Check if thread was previously running on a now idle core.
        // If that's the case, reassign to the same core.
        let reassigned = diff & bitmap_next & bitmap_prev;
        if reassigned > 0 {
            let id = u8::from_bitmap(reassigned).map(ThreadId).unwrap();
            let changed_core = self.next.iter().position(|(_, i)| *i == id).unwrap();
            self.next[changed_core].0 = true;
            return Some(CoreId(changed_core as u8));
        }

        let prev_allocated = u8::from_bitmap(bitmap_allocated & diff).map(ThreadId);
        let new_allocated = u8::from_bitmap(bitmap_next & diff).map(ThreadId);
        let changed_core = self
            .next
            .iter()
            .position(|(running, id)| running.then(|| *id) == prev_allocated)
            .unwrap();

        if let Some(id) = new_allocated {
            self.next[changed_core] = (true, id);
        } else {
            self.next[changed_core].0 = false;
        }
        return Some(CoreId(changed_core as u8));
    }

    default fn get_next(&self, core: CoreId) -> Option<ThreadId> {
        if usize::from(core) >= N_CORES {
            return None;
        }
        let (is_running, next) = self.next[usize::from(core)];
        is_running.then(|| next)
    }
}

impl<const N_QUEUES: usize, const N_THREADS: usize> GlobalRunqueue<N_QUEUES, N_THREADS, 1>
    for RunQueue<N_QUEUES, N_THREADS>
{
    fn add(&mut self, n: ThreadId, rq: RunqueueId) -> Option<CoreId> {
        debug_assert!(usize::from(n) < N_THREADS);
        debug_assert!(usize::from(rq) < N_QUEUES);
        let bitcache = self.bitcache;
        self.bitcache |= 1 << rq.0;
        self.queues.push(n.0, rq.0);
        (self.bitcache > bitcache).then(|| CoreId(0))
    }

    /// Advances runqueue number `rq`.
    ///
    /// This is used to "yield" to another thread of *the same* priority.
    ///
    /// Returns a [`CoreId`] if the allocation for this core changed.
    fn advance(&mut self, _: ThreadId, rq: RunqueueId) -> Option<CoreId> {
        debug_assert!((rq.0 as usize) < N_QUEUES);
        self.queues.advance(rq.0).then(|| CoreId(0))
    }

    fn del(&mut self, n: ThreadId, rq: RunqueueId) -> Option<CoreId> {
        debug_assert!((n.0 as usize) < N_THREADS);
        debug_assert!((rq.0 as usize) < N_QUEUES);
        let popped = self.queues.pop_head(rq.0);
        assert_eq!(popped, Some(n.0));
        if self.queues.is_empty(rq.0) {
            self.bitcache &= !(1 << rq.0);
        }
        Some(CoreId(0))
    }

    fn get_next(&self, _core: CoreId) -> Option<ThreadId> {
        self.peek_head(self.bitcache)
    }

    fn reallocate(&mut self) -> Option<CoreId> {
        unimplemented!()
    }
}

impl<const N_QUEUES: usize, const N_THREADS: usize> GlobalRunqueue<N_QUEUES, N_THREADS, 2>
    for RunQueue<N_QUEUES, N_THREADS, 2>
{
    // fn reallocate(&mut self) -> Option<CoreId> {
    //     let next = self.get_next_n();

    //     if self.next[0] == next[0] {
    //         if self.next[1] == next[1] {
    //             return None;
    //         }
    //         self.next[1] = next[1];
    //         return Some(CoreId(1));
    //     }
    //     if self.next[1] == next[0] {
    //         if self.next[0] == next[1] {
    //             return None;
    //         }
    //         self.next[0] = next[1];
    //         return Some(CoreId(0));
    //     }
    //     if self.next[1] == next[1] {
    //         self.next[0] = next[0];
    //         return Some(CoreId(0));
    //     } else {
    //         self.next[1] = next[0];
    //         Some(CoreId(1))
    //     }
    // }

    // fn reallocate(&mut self) -> Option<CoreId> {
    //     let next = self.get_next_n();
    //     let self_next_0 = self.next[0].0.then(|| self.next[0].1);
    //     let self_next_1 = self.next[1].0.then(|| self.next[1].1);

    //     if next[0].is_none() {
    //         // No thread running.
    //         if self.next[0].0 {
    //             self.next[0].0 = false;
    //             return Some(CoreId(0));
    //         }
    //         if self.next[1].0 {
    //             self.next[1].0 = false;
    //             return Some(CoreId(1));
    //         }
    //         return None;
    //     }
    //     let next_0 = next[0].unwrap();
    //     if next_0 == self.next[0].1 {
    //         if !self.next[0].0 {
    //             self.next[0].0 = true;
    //             return Some(CoreId(0));
    //         }
    //     } else if next_0 == self.next[1].1 {
    //         if !self.next[1].0 {
    //             self.next[1].0 = true;
    //             return Some(CoreId(1));
    //         }
    //     } else {
    //         if !self.next[0].1 {
    //             self.next[0] = (true, next_0);
    //             return Some(CoreId(0));

    //         }
    //     }

    fn advance(&mut self, n: ThreadId, rq: RunqueueId) -> Option<CoreId> {
        debug_assert!((rq.0 as usize) < N_QUEUES);
        if Some(n.0) == self.queues.peek_head(rq.0) {
            self.queues.advance(rq.0);
        } else {
            // If the thread is not the head remove it
            // from queue and re-insert it at tail.
            self.queues.pop_next(rq.0);
            self.queues.push(n.0, rq.0);
        }
        self.reallocate()
    }
}

fn ffs(val: usize) -> u32 {
    USIZE_BITS as u32 - val.leading_zeros()
}
