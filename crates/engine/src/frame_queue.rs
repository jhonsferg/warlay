//! A small bounded queue for real-time frames (voice, shared desktop audio).
//!
//! Real-time audio must stay close to live: when the consumer falls behind,
//! the *oldest* frames are the ones worth dropping, never the newest. A plain
//! bounded channel does the opposite (it refuses new items once full), which
//! turns a stall into seconds of stale audio.

use std::collections::VecDeque;
use std::sync::Mutex;

use uuid::Uuid;

/// A frame and the member who sent it.
pub(crate) type Frame = (Uuid, Vec<u8>);

pub(crate) struct FrameQueue {
    items: Mutex<VecDeque<Frame>>,
    capacity: usize,
}

impl FrameQueue {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            items: Mutex::new(VecDeque::with_capacity(capacity)),
            capacity: capacity.max(1),
        }
    }

    /// Queues a frame, dropping the oldest one when the queue is full.
    /// Returns true when an older frame had to be dropped.
    pub(crate) fn push(&self, frame: Frame) -> bool {
        let mut items = self.items.lock().unwrap_or_else(|e| e.into_inner());
        let dropped = items.len() >= self.capacity;
        if dropped {
            items.pop_front();
        }
        items.push_back(frame);
        dropped
    }

    /// Takes everything queued so far, oldest first.
    pub(crate) fn drain(&self) -> Vec<Frame> {
        let mut items = self.items.lock().unwrap_or_else(|e| e.into_inner());
        items.drain(..).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_order_and_empties_on_drain() {
        let q = FrameQueue::new(4);
        let a = Uuid::new_v4();
        assert!(!q.push((a, vec![1])));
        assert!(!q.push((a, vec![2])));
        assert_eq!(q.drain(), vec![(a, vec![1]), (a, vec![2])]);
        assert!(q.drain().is_empty());
    }

    #[test]
    fn a_full_queue_drops_the_oldest_frames_not_the_newest() {
        let q = FrameQueue::new(2);
        let a = Uuid::new_v4();
        assert!(!q.push((a, vec![0])));
        assert!(!q.push((a, vec![1])));
        assert!(q.push((a, vec![2])));
        assert!(q.push((a, vec![3])));
        assert_eq!(q.drain(), vec![(a, vec![2]), (a, vec![3])]);
    }
}
