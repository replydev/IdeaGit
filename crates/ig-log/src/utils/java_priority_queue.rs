//! `java.util.PriorityQueue` with a comparator: a binary min-heap with Java's exact sift order, so
//! elements that compare equal are polled in the same order as upstream.
//!
//! IdeaGit helper (no upstream file): linear BEK polls graph edges that tie on their down node,
//! and which one comes first changes the result.

use std::cmp::Ordering;

pub struct JavaPriorityQueue<T, C> {
    queue: Vec<T>,
    comparator: C,
}

impl<T: Clone, C: Fn(&T, &T) -> Ordering> JavaPriorityQueue<T, C> {
    pub fn new(comparator: C) -> JavaPriorityQueue<T, C> {
        JavaPriorityQueue {
            queue: Vec::new(),
            comparator,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// `offer`: appends and sifts up.
    pub fn add(&mut self, e: T) {
        let k = self.queue.len();
        self.queue.push(e.clone());
        self.sift_up(k, e);
    }

    /// `AbstractQueue.addAll`: adds the elements one by one.
    pub fn add_all(&mut self, elements: impl IntoIterator<Item = T>) {
        for e in elements {
            self.add(e);
        }
    }

    /// `poll`: removes the head, moves the last element to the top and sifts it down.
    pub fn poll(&mut self) -> Option<T> {
        if self.queue.is_empty() {
            return None;
        }
        let x = self.queue.pop().expect("not empty");
        if self.queue.is_empty() {
            return Some(x);
        }
        let result = self.queue[0].clone();
        self.sift_down(0, x);
        Some(result)
    }

    fn sift_up(&mut self, mut k: usize, x: T) {
        while k > 0 {
            let parent = (k - 1) >> 1;
            if (self.comparator)(&x, &self.queue[parent]) != Ordering::Less {
                break;
            }
            self.queue[k] = self.queue[parent].clone();
            k = parent;
        }
        self.queue[k] = x;
    }

    fn sift_down(&mut self, mut k: usize, x: T) {
        let n = self.queue.len();
        let half = n >> 1;
        while k < half {
            let mut child = 2 * k + 1;
            let right = child + 1;
            if right < n
                && (self.comparator)(&self.queue[child], &self.queue[right]) == Ordering::Greater
            {
                child = right;
            }
            if (self.comparator)(&x, &self.queue[child]) != Ordering::Greater {
                break;
            }
            self.queue[k] = self.queue[child].clone();
            k = child;
        }
        self.queue[k] = x;
    }
}
