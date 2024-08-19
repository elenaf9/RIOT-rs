#![cfg_attr(not(test), no_std)]
#![feature(lint_reasons)]
#![feature(min_specialization)]

mod runqueue;
pub use runqueue::{CoreId, GlobalRunqueue, RunQueue, RunqueueId, ThreadId};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rq_basic() {
        let mut runqueue: RunQueue<8, 32> = RunQueue::new();

        runqueue.add(ThreadId::new(0), RunqueueId::new(0));
        runqueue.add(ThreadId::new(1), RunqueueId::new(0));
        runqueue.add(ThreadId::new(2), RunqueueId::new(0));

        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(0)));

        runqueue.advance(ThreadId::new(0), RunqueueId::new(0));

        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(1)));
        runqueue.advance(ThreadId::new(1), RunqueueId::new(0));

        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(2)));
        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(2)));

        runqueue.advance(ThreadId::new(2), RunqueueId::new(0));
        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(0)));

        runqueue.advance(ThreadId::new(0), RunqueueId::new(0));
        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(1)));

        runqueue.advance(ThreadId::new(1), RunqueueId::new(0));
        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(2)));
    }

    #[test]
    fn test_rq_all32() {
        let mut runqueue: RunQueue<8, 32> = RunQueue::new();

        for i in 0..=31 {
            runqueue.add(ThreadId::new(i), RunqueueId::new(0));
        }

        for i in 0..=31 {
            assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(i)));
            runqueue.advance(ThreadId::new(i), RunqueueId::new(0));
        }

        for i in 0..=31 {
            assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(i)));
            runqueue.advance(ThreadId::new(i), RunqueueId::new(0));
        }
    }

    #[test]
    fn test_rq_basic_twoprio() {
        let mut runqueue: RunQueue<8, 32> = RunQueue::new();

        runqueue.add(ThreadId::new(0), RunqueueId::new(0));
        runqueue.add(ThreadId::new(1), RunqueueId::new(0));
        runqueue.add(ThreadId::new(3), RunqueueId::new(0));

        runqueue.add(ThreadId::new(2), RunqueueId::new(1));
        runqueue.add(ThreadId::new(4), RunqueueId::new(1));

        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(2)));
        runqueue.del(ThreadId::new(2), RunqueueId::new(1));
        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(4)));
        runqueue.del(ThreadId::new(4), RunqueueId::new(1));
        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(0)));
        runqueue.del(ThreadId::new(0), RunqueueId::new(0));
        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(1)));
        runqueue.del(ThreadId::new(1), RunqueueId::new(0));
        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(3)));
        runqueue.del(ThreadId::new(3), RunqueueId::new(0));
        assert_eq!(runqueue.get_next(CoreId::new(0)), None);
    }
    #[test]
    fn test_push_twice() {
        let mut runqueue: RunQueue<8, 32> = RunQueue::new();

        runqueue.add(ThreadId::new(0), RunqueueId::new(0));
        runqueue.add(ThreadId::new(1), RunqueueId::new(0));

        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(0)));
        runqueue.del(ThreadId::new(0), RunqueueId::new(0));
        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(1)));

        runqueue.add(ThreadId::new(0), RunqueueId::new(0));

        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(1)));

        runqueue.advance(ThreadId::new(1), RunqueueId::new(0));
        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(0)));
    }

    #[test]
    fn multicore_basic() {
        let mut runqueue: RunQueue<8, 32, 4> = RunQueue::new();

        // First thread should get allocated to core 0.
        assert_eq!(
            runqueue.add(ThreadId::new(0), RunqueueId::new(0)),
            Some(CoreId::new(0))
        );
        // Second thread should get allocated to core 1.
        assert_eq!(
            runqueue.add(ThreadId::new(1), RunqueueId::new(0)),
            Some(CoreId::new(1))
        );

        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(0)));
        assert_eq!(runqueue.get_next(CoreId::new(1)), Some(ThreadId::new(1)));
        assert!(runqueue.get_next(CoreId::new(2)).is_none());

        // Advancing a runqueue shouldn't change any allocations
        // if all threads in the queue are already running.
        assert_eq!(runqueue.advance(ThreadId::new(0), RunqueueId::new(0)), None);
        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(0)));
        assert_eq!(runqueue.get_next(CoreId::new(1)), Some(ThreadId::new(1)));
        assert!(runqueue.get_next(CoreId::new(2)).is_none());

        // Restores original order.
        assert_eq!(runqueue.advance(ThreadId::new(1), RunqueueId::new(0)), None);

        // Add more threads, which should be allocated to free
        // cores.
        assert_eq!(
            runqueue.add(ThreadId::new(2), RunqueueId::new(0)),
            Some(CoreId::new(2))
        );
        assert_eq!(
            runqueue.add(ThreadId::new(3), RunqueueId::new(0)),
            Some(CoreId::new(3))
        );
        assert_eq!(runqueue.add(ThreadId::new(4), RunqueueId::new(0)), None);
        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(0)));
        assert_eq!(runqueue.get_next(CoreId::new(1)), Some(ThreadId::new(1)));
        assert_eq!(runqueue.get_next(CoreId::new(2)), Some(ThreadId::new(2)));
        assert_eq!(runqueue.get_next(CoreId::new(3)), Some(ThreadId::new(3)));

        // Advancing the runqueue now should change the mapping
        // on core 0, since the previous head was running there.
        assert_eq!(
            runqueue.advance(ThreadId::new(0), RunqueueId::new(0)),
            Some(CoreId::new(0))
        );
        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(4)));
        // Other allocations shouldn't change.
        assert_eq!(runqueue.get_next(CoreId::new(1)), Some(ThreadId::new(1)));
        assert_eq!(runqueue.get_next(CoreId::new(2)), Some(ThreadId::new(2)));
        assert_eq!(runqueue.get_next(CoreId::new(3)), Some(ThreadId::new(3)));

        // Adding or deleting waiting threads shouldn't change
        // any allocations.
        assert_eq!(runqueue.del(ThreadId::new(0), RunqueueId::new(0)), None);
        assert_eq!(runqueue.add(ThreadId::new(5), RunqueueId::new(0)), None);

        // Deleting a running thread should allocate the waiting
        // thread to the now free core.
        assert_eq!(
            runqueue.del(ThreadId::new(2), RunqueueId::new(0)),
            Some(CoreId::new(2))
        );
        assert_eq!(runqueue.get_next(CoreId::new(2)), Some(ThreadId::new(5)));
        // Other allocations shouldn't change.
        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(4)));
        assert_eq!(runqueue.get_next(CoreId::new(1)), Some(ThreadId::new(1)));
        assert_eq!(runqueue.get_next(CoreId::new(3)), Some(ThreadId::new(3)));
    }

    #[test]
    fn multicore_multiqueue() {
        let mut runqueue: RunQueue<8, 32, 4> = RunQueue::new();

        assert_eq!(
            runqueue.add(ThreadId::new(0), RunqueueId::new(2)),
            Some(CoreId::new(0))
        );
        assert_eq!(
            runqueue.add(ThreadId::new(1), RunqueueId::new(2)),
            Some(CoreId::new(1))
        );
        assert_eq!(
            runqueue.add(ThreadId::new(2), RunqueueId::new(1)),
            Some(CoreId::new(2))
        );
        assert_eq!(
            runqueue.add(ThreadId::new(3), RunqueueId::new(0)),
            Some(CoreId::new(3))
        );
        assert_eq!(runqueue.add(ThreadId::new(4), RunqueueId::new(0)), None);

        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(0)));
        assert_eq!(runqueue.get_next(CoreId::new(1)), Some(ThreadId::new(1)));
        assert_eq!(runqueue.get_next(CoreId::new(2)), Some(ThreadId::new(2)));
        assert_eq!(runqueue.get_next(CoreId::new(3)), Some(ThreadId::new(3)));

        // Advancing highest priority queue shouldn't change anything
        // because there are more cores than threads in this priority's queue.
        assert_eq!(runqueue.advance(ThreadId::new(0), RunqueueId::new(2)), None);
        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(0)));
        assert_eq!(runqueue.get_next(CoreId::new(1)), Some(ThreadId::new(1)));
        assert_eq!(runqueue.get_next(CoreId::new(2)), Some(ThreadId::new(2)));
        assert_eq!(runqueue.get_next(CoreId::new(3)), Some(ThreadId::new(3)));

        // Advancing lowest priority queue should change allocations
        // since there are two threads in this priority's queue,
        // but only one available core for them.

        // Core 3 was newly allocated.
        assert_eq!(
            runqueue.advance(ThreadId::new(3), RunqueueId::new(0)),
            Some(CoreId::new(3))
        );
        assert_eq!(runqueue.get_next(CoreId::new(3)), Some(ThreadId::new(4)));
        // Other allocations didn't change.
        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(0)));
        assert_eq!(runqueue.get_next(CoreId::new(1)), Some(ThreadId::new(1)));
        assert_eq!(runqueue.get_next(CoreId::new(2)), Some(ThreadId::new(2)));

        // Restores original order.
        runqueue.advance(ThreadId::new(4), RunqueueId::new(0));

        // Delete one high-priority thread.
        // The waiting low-priority thread should be allocated
        // to the newly freed core.

        // Core 0 was newly allocated.
        assert_eq!(
            runqueue.del(ThreadId::new(0), RunqueueId::new(2)),
            Some(CoreId::new(0))
        );
        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(4)));
        // Other allocations didn't change.
        assert_eq!(runqueue.get_next(CoreId::new(1)), Some(ThreadId::new(1)));
        assert_eq!(runqueue.get_next(CoreId::new(2)), Some(ThreadId::new(2)));
        assert_eq!(runqueue.get_next(CoreId::new(3)), Some(ThreadId::new(3)));

        // Add one medium-priority thread.
        // The low-priority thread furthest back in its priority queue
        // should be preempted.

        // Core 0 was newly allocated.
        assert_eq!(
            runqueue.add(ThreadId::new(5), RunqueueId::new(1)),
            Some(CoreId::new(0))
        );
        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(5)));
        // Other allocations didn't change.
        assert_eq!(runqueue.get_next(CoreId::new(1)), Some(ThreadId::new(1)));
        assert_eq!(runqueue.get_next(CoreId::new(2)), Some(ThreadId::new(2)));
        assert_eq!(runqueue.get_next(CoreId::new(3)), Some(ThreadId::new(3)));
    }

    #[test]
    fn multicore_invalid_core() {
        let mut runqueue: RunQueue<8, 32, 2> = RunQueue::new();
        assert_eq!(
            runqueue.add(ThreadId::new(0), RunqueueId::new(2)),
            Some(CoreId::new(0))
        );
        assert_eq!(
            runqueue.add(ThreadId::new(1), RunqueueId::new(2)),
            Some(CoreId::new(1))
        );
        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(0)));
        assert_eq!(runqueue.get_next(CoreId::new(1)), Some(ThreadId::new(1)));
        // Querying for n > `N_CORES` shouldn't cause a panic.
        assert_eq!(runqueue.get_next(CoreId::new(2)), None)
    }

    #[test]
    fn multicore_advance() {
        let mut runqueue: RunQueue<8, 32, 4> = RunQueue::new();
        assert_eq!(
            runqueue.add(ThreadId::new(0), RunqueueId::new(0)),
            Some(CoreId::new(0))
        );
        assert_eq!(
            runqueue.add(ThreadId::new(1), RunqueueId::new(0)),
            Some(CoreId::new(1))
        );
        assert_eq!(
            runqueue.add(ThreadId::new(2), RunqueueId::new(0)),
            Some(CoreId::new(2))
        );
        assert_eq!(
            runqueue.add(ThreadId::new(3), RunqueueId::new(0)),
            Some(CoreId::new(3))
        );
        assert_eq!(runqueue.add(ThreadId::new(4), RunqueueId::new(0)), None);
        assert_eq!(runqueue.add(ThreadId::new(5), RunqueueId::new(0)), None);

        // Advance head.
        assert_eq!(
            runqueue.advance(ThreadId::new(0), RunqueueId::new(0)),
            Some(CoreId::new(0))
        );
        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(4)));
        // Other allocations didn't change.
        assert_eq!(runqueue.get_next(CoreId::new(1)), Some(ThreadId::new(1)));
        assert_eq!(runqueue.get_next(CoreId::new(2)), Some(ThreadId::new(2)));
        assert_eq!(runqueue.get_next(CoreId::new(3)), Some(ThreadId::new(3)));

        // Advance from a thread that is not head.
        assert_eq!(
            runqueue.advance(ThreadId::new(2), RunqueueId::new(0)),
            Some(CoreId::new(2))
        );
        assert_eq!(runqueue.get_next(CoreId::new(2)), Some(ThreadId::new(5)));
        // Other allocations didn't change.
        assert_eq!(runqueue.get_next(CoreId::new(0)), Some(ThreadId::new(4)));
        assert_eq!(runqueue.get_next(CoreId::new(1)), Some(ThreadId::new(1)));
        assert_eq!(runqueue.get_next(CoreId::new(3)), Some(ThreadId::new(3)));
    }
}
