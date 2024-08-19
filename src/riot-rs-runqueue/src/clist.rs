//! This module implements an array of `N_QUEUES` circular linked lists over an
//! array of size `N_THREADS`.
//! The array is used for "next" pointers, so each integer value in the array
//! corresponds to one element, which can only be in one of the lists.
#[derive(Debug, Copy, Clone)]
pub struct CList<const N_QUEUES: usize, const N_THREADS: usize> {
    tail: [u8; N_QUEUES],
    next_idxs: [u8; N_THREADS],
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
        if self.next_idxs[n as usize] == Self::sentinel() {
            if self.tail[rq as usize] == Self::sentinel() {
                // rq is empty, link both tail and n.next to n
                self.tail[rq as usize] = n;
                self.next_idxs[n as usize] = n;
            } else {
                // rq has an entry already, so
                // 1. n.next = old_tail.next ("first" in list)
                self.next_idxs[n as usize] = self.next_idxs[self.tail[rq as usize] as usize];
                // 2. old_tail.next = n
                self.next_idxs[self.tail[rq as usize] as usize] = n;
                // 3. tail = n
                self.tail[rq as usize] = n;
            }
        }
    }

    /// Delete a thread from the runqueue.
    pub fn del(&mut self, n: u8, rq: u8) {
        if self.next_idxs[n as usize] == Self::sentinel() {
            // Thread is not in rq, do nothing.
            return;
        }

        if self.next_idxs[n as usize] == n {
            // `n` should always be the tail in this case, but better be
            // safe and double-check.
            if self.tail[rq as usize] == n {
                // `n` bites itself, so there's only one entry.
                // Clear tail.
                self.tail[rq as usize] = Self::sentinel();
            }
        } else {
            let next = self.next_idxs[n as usize];

            // Find previous in list and update its next-idx.
            let prev = self
                .next_idxs
                .iter()
                .position(|next_idx| *next_idx == n)
                .expect("List is circular.");
            self.next_idxs[prev] = next as u8;

            // Update tail if the thread was the tail.
            if self.tail[rq as usize] == n {
                self.tail[rq as usize] = prev as u8;
            }
        }

        // Clear thread's value.
        self.next_idxs[n as usize] = Self::sentinel();
    }

    pub fn pop_head(&mut self, rq: u8) -> Option<u8> {
        if self.tail[rq as usize] == Self::sentinel() {
            // rq is empty, do nothing
            None
        } else {
            let head = self.next_idxs[self.tail[rq as usize] as usize];
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
    }

    pub fn peek_head(&self, rq: u8) -> Option<u8> {
        if self.tail[rq as usize] == Self::sentinel() {
            None
        } else {
            Some(self.next_idxs[self.tail[rq as usize] as usize])
        }
    }

    pub fn advance(&mut self, rq: u8) -> bool {
        if self.tail[rq as usize] == Self::sentinel() {
            return false;
        }
        self.tail[rq as usize] = self.next_idxs[self.tail[rq as usize] as usize];
        true
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
