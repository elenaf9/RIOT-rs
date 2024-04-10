// Disable indexing lints for now
#![allow(clippy::indexing_slicing)]

use core::mem;

use self::clist::CList;

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
    queues: clist::CList<N_QUEUES, N_THREADS>,
    next: [Option<ThreadId>; N_CORES],
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
            next: [None; N_CORES],
        }
    }

    /// Adds thread with pid `n` to runqueue number `rq`.
    fn add(&mut self, n: ThreadId, rq: RunqueueId) {
        debug_assert!(usize::from(n) < N_THREADS);
        debug_assert!(usize::from(rq) < N_QUEUES);
        self.bitcache |= 1 << rq.0;
        self.queues.push(n.0, rq.0);
    }

    /// Removes thread with pid `n` from runqueue number `rq`.
    ///
    /// # Panics
    ///
    /// Panics if `n` is not the queue's head.
    /// This is fine, RIOT-rs only ever calls `pop_head()` for the current thread.
    pub fn pop_head(&mut self, n: ThreadId, rq: RunqueueId) {
        debug_assert!(usize::from(n) < N_THREADS);
        debug_assert!(usize::from(rq) < N_QUEUES);
        let popped = self.queues.pop_head(rq.0);
        //
        assert_eq!(popped, Some(n.0));
        if self.queues.is_empty(rq.0) {
            self.bitcache &= !(1 << rq.0);
        }
    }

    /// Removes thread with pid `n`.
    fn del(&mut self, n: ThreadId) {
        if let Some(empty_runqueue) = self.queues.del(n.0) {
            self.bitcache &= !(1 << empty_runqueue);
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
        let mut head = match self.peek_bitcache_head(bitcache) {
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
                head = match self.peek_bitcache_head(bitcache) {
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

    fn peek_bitcache_head(&self, bitcache: usize) -> Option<ThreadId> {
        // Switch to highest priority runqueue remaining
        // in the bitcache.
        let rq = match u8::from_bitmap(bitcache) {
            Some(rq) => rq,
            None => return None,
        };
        self.queues.peek_head(rq).map(ThreadId::new)
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
    /// Panics for `N_CORES == 1` if `n` is not the queue's head.
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
        RunQueue::<N_QUEUES, N_THREADS, N_CORES>::add(self, n, rq);
        self.reallocate()
    }

    default fn del(&mut self, n: ThreadId, rq: RunqueueId) -> Option<CoreId> {
        if self.queues.peek_head(rq.0) == Some(n.0) {
            self.pop_head(n, rq);
        } else {
            self.del(n);
        }
        self.reallocate()
    }

    default fn advance(&mut self, n: ThreadId, rq: RunqueueId) -> Option<CoreId> {
        debug_assert!((rq.0 as usize) < N_QUEUES);
        if self.queues.peek_head(rq.0) == Some(n.0) {
            self.queues.advance(rq.0);
        } else {
            // If the thread is not the head remove it
            // from queue and re-insert it at tail.
            self.queues.del(n.0);
            self.queues.push(n.0, rq.0);
        }
        self.reallocate()
    }

    default fn reallocate(&mut self) -> Option<CoreId> {
        let next = self.get_next_n();
        let mut bitmap_next = 0;
        let mut bitmap_allocated = 0;
        for i in 0..N_CORES {
            if let Some(id) = next[i] {
                bitmap_next |= 1 << id.0
            }
            if let Some(id) = self.next[i] {
                bitmap_allocated |= 1 << id.0
            }
        }
        if bitmap_next == bitmap_allocated {
            return None;
        }
        let diff = bitmap_next ^ bitmap_allocated;
        let prev_allocated = u8::from_bitmap(bitmap_allocated & diff).map(ThreadId);
        let new_allocated = u8::from_bitmap(bitmap_next & diff).map(ThreadId);

        let changed_core = self.next.iter().position(|i| *i == prev_allocated).unwrap();
        self.next[changed_core] = new_allocated;
        return Some(CoreId(changed_core as u8));
    }

    default fn get_next(&self, core: CoreId) -> Option<ThreadId> {
        if usize::from(core) >= N_CORES {
            return None;
        }
        self.next[usize::from(core)]
    }
}

impl<const N_QUEUES: usize, const N_THREADS: usize> GlobalRunqueue<N_QUEUES, N_THREADS, 1>
    for RunQueue<N_QUEUES, N_THREADS>
{
    fn add(&mut self, n: ThreadId, rq: RunqueueId) -> Option<CoreId> {
        let bitcache = self.bitcache;
        RunQueue::<N_QUEUES, N_THREADS>::add(self, n, rq);
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

    fn get_next(&self, _core: CoreId) -> Option<ThreadId> {
        self.peek_bitcache_head(self.bitcache)
    }

    fn reallocate(&mut self) -> Option<CoreId> {
        Some(CoreId(0))
    }
}

impl<const N_QUEUES: usize, const N_THREADS: usize> GlobalRunqueue<N_QUEUES, N_THREADS, 2>
    for RunQueue<N_QUEUES, N_THREADS, 2>
{
    fn reallocate(&mut self) -> Option<CoreId> {
        let next = self.get_next_n();

        if self.next[0] == next[0] {
            if self.next[1] == next[1] {
                return None;
            }
            self.next[1] = next[1];
            return Some(CoreId(1));
        }
        if self.next[1] == next[0] {
            if self.next[0] == next[1] {
                return None;
            }
            self.next[0] = next[1];
            return Some(CoreId(0));
        }
        if self.next[1] == next[1] {
            self.next[0] = next[0];
            return Some(CoreId(0));
        } else {
            self.next[1] = next[0];
            Some(CoreId(1))
        }
    }

    fn advance(&mut self, n: ThreadId, rq: RunqueueId) -> Option<CoreId> {
        debug_assert!((rq.0 as usize) < N_QUEUES);
        if self.queues.peek_head(rq.0) == Some(n.0) {
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

#[inline]
fn ffs(val: usize) -> u32 {
    USIZE_BITS as u32 - val.leading_zeros()
}

mod clist {
    //! This module implements an array of `N_QUEUES` circular linked lists over an
    //! array of size `N_THREADS`.
    //!
    //! The array is used for "next" pointers, so each integer value in the array
    //! corresponds to one element, which can only be in one of the lists.
    #[derive(Debug, Copy, Clone)]
    pub struct CList<const N_QUEUES: usize, const N_THREADS: usize> {
        tail: [u8; N_QUEUES],
        next_idxs: [u8; N_THREADS],
    }

    impl<const N_QUEUES: usize, const N_THREADS: usize> Default for CList<N_QUEUES, N_THREADS> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<const N_QUEUES: usize, const N_THREADS: usize> CList<N_QUEUES, N_THREADS> {
        pub const fn new() -> Self {
            // TODO: ensure N fits in u8
            // assert!(N<255); is not allowed in const because it could panic
            CList {
                tail: [Self::sentinel(); N_QUEUES],
                next_idxs: [Self::sentinel(); N_THREADS],
            }
        }

        pub const fn sentinel() -> u8 {
            0xFF
        }

        pub fn is_empty(&self, rq: u8) -> bool {
            self.tail[rq as usize] == Self::sentinel()
        }

        pub fn push(&mut self, n: u8, rq: u8) {
            assert!(n < Self::sentinel());
            if self.next_idxs[n as usize] != Self::sentinel() {
                return;
            }

            if let Some(head) = self.peek_head(rq) {
                // rq has an entry already, so
                // 1. n.next = old_tail.next ("first" in list)
                self.next_idxs[n as usize] = head;
                // 2. old_tail.next = n
                self.next_idxs[self.tail[rq as usize] as usize] = n;
                // 3. tail = n
                self.tail[rq as usize] = n;
            } else {
                // rq is empty, link both tail and n.next to n
                self.tail[rq as usize] = n;
                self.next_idxs[n as usize] = n;
            }
        }

        /// Removes a thread from the list.
        ///
        /// If the thread was the only thread in its runqueue, `Some` is returned
        /// with the ID of the now empty runqueue.
        pub fn del(&mut self, n: u8) -> Option<u8> {
            let mut empty_runqueue = None;

            // Find previous thread in circular runqueue.
            let prev = position(&self.next_idxs, n)?;

            // Handle if thread is tail of a runqueue.
            if let Some(rq) = position(&self.tail, n) {
                if prev == n as usize {
                    // Runqueue is empty now.
                    self.tail[rq] = Self::sentinel();
                    empty_runqueue = Some(rq as u8);
                } else {
                    self.tail[rq] = prev as u8;
                }
            }
            self.next_idxs[prev] = self.next_idxs[n as usize];
            self.next_idxs[n as usize] = Self::sentinel();
            empty_runqueue
        }

        pub fn pop_head(&mut self, rq: u8) -> Option<u8> {
            let head = self.peek_head(rq)?;

            if head == self.tail[rq as usize] {
                // rq's tail bites itself, so there's only one entry.
                // so, clear tail.
                self.tail[rq as usize] = Self::sentinel();
                // rq is now empty
            } else {
                // rq has multiple entries,
                // so set tail.next to head.next (second in list)
                self.next_idxs[self.tail[rq as usize] as usize] = self.next_idxs[head as usize];
            }

            // now clear head's next value
            self.next_idxs[head as usize] = Self::sentinel();
            Some(head)
        }

        #[inline]
        pub fn peek_head(&self, rq: u8) -> Option<u8> {
            if self.is_empty(rq) {
                None
            } else {
                Some(self.next_idxs[self.tail[rq as usize] as usize])
            }
        }

        pub fn advance(&mut self, rq: u8) -> bool {
            if let Some(head) = self.peek_head(rq) {
                self.tail[rq as usize] = head;
                true
            } else {
                false
            }
        }

        pub fn peek_next(&self, curr: u8) -> u8 {
            self.next_idxs[curr as usize]
        }

        /// Remove next thread after head in runqueue.
        pub fn pop_next(&mut self, rq: u8) -> Option<u8> {
            let head = self.peek_head(rq)?;
            let next = self.peek_next(head);
            if next == head {
                return None;
            }
            self.next_idxs[head as usize] = self.next_idxs[next as usize];
            self.next_idxs[next as usize] = Self::sentinel();
            Some(next)
        }
    }

    /// Helper function that is needed because hax doesn't support `Iterator::position` yet.
    fn position<const N: usize>(slice: &[u8; N], search_item: u8) -> Option<usize> {
        let mut i = 0;
        while i < N && slice[i] != search_item {
            i += 1;
        }
        if i < N {
            Some(i)
        } else {
            None
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_clist_basic() {
            let mut clist: CList<8, 32> = CList::new();
            assert!(clist.is_empty(0));
            clist.push(0, 0);
            assert_eq!(clist.pop_head(0), Some(0));
            assert_eq!(clist.pop_head(0), None);
        }

        #[test]
        fn test_clist_push_already_in_list() {
            let mut clist: CList<8, 32> = CList::new();
            assert!(clist.is_empty(0));
            clist.push(0, 0);
            clist.push(0, 0);
            assert_eq!(clist.pop_head(0), Some(0));
            assert_eq!(clist.pop_head(0), None);
            assert!(clist.is_empty(0));
        }

        #[test]
        fn test_clist_push_two() {
            let mut clist: CList<8, 32> = CList::new();
            assert!(clist.is_empty(0));
            clist.push(0, 0);
            clist.push(1, 0);
            assert_eq!(clist.pop_head(0), Some(0));
            assert_eq!(clist.pop_head(0), Some(1));
            assert_eq!(clist.pop_head(0), None);
            assert!(clist.is_empty(0));
        }

        #[test]
        fn test_clist_push_all() {
            const N: usize = 255;
            let mut clist: CList<8, N> = CList::new();
            assert!(clist.is_empty(0));
            for i in 0..(N - 1) {
                println!("pushing {}", i);
                clist.push(i as u8, 0);
            }
            for i in 0..(N - 1) {
                println!("{}", i);
                assert_eq!(clist.pop_head(0), Some(i as u8));
            }
            assert_eq!(clist.pop_head(0), None);
            assert!(clist.is_empty(0));
        }

        #[test]
        fn test_clist_advance() {
            let mut clist: CList<8, 32> = CList::new();
            assert!(clist.is_empty(0));
            clist.push(0, 0);
            clist.push(1, 0);
            clist.advance(0);
            assert_eq!(clist.pop_head(0), Some(1));
            assert_eq!(clist.pop_head(0), Some(0));
            assert_eq!(clist.pop_head(0), None);
            assert!(clist.is_empty(0));
        }

        #[test]
        fn test_clist_peek_head() {
            let mut clist: CList<8, 32> = CList::new();
            assert!(clist.is_empty(0));
            clist.push(0, 0);
            clist.push(1, 0);
            assert_eq!(clist.peek_head(0), Some(0));
            assert_eq!(clist.peek_head(0), Some(0));
            assert_eq!(clist.pop_head(0), Some(0));
            assert_eq!(clist.peek_head(0), Some(1));
            assert_eq!(clist.pop_head(0), Some(1));
            assert_eq!(clist.peek_head(0), None);
            assert_eq!(clist.peek_head(0), None);
            assert_eq!(clist.pop_head(0), None);
            assert!(clist.is_empty(0));
        }
    }
}
