use std::collections::{BTreeMap, HashMap};

pub const DEFAULT_TARGET_DEPTH: usize = 3; // ~60ms at 20ms/frame
pub const MAX_HOLE_TICKS: u32 = 2; // skip missing frame after 2 ticks (40ms)

#[derive(Debug)]
pub struct PeerJitterBuffer {
    target_depth: usize,
    max_hole_ticks: u32,
    frames: BTreeMap<u32, Vec<u8>>,
    next_seq: Option<u32>,
    started: bool,
    hole_tick_counter: u32,
}

impl PeerJitterBuffer {
    pub fn new(target_depth: usize) -> Self {
        Self {
            target_depth,
            max_hole_ticks: MAX_HOLE_TICKS,
            frames: BTreeMap::new(),
            next_seq: None,
            started: false,
            hole_tick_counter: 0,
        }
    }

    pub fn push(&mut self, seq: u32, payload: Vec<u8>) {
        if let Some(next) = self.next_seq {
            // Drop frames older than next_seq if we already established sequence flow
            if seq < next {
                return;
            }
        }
        self.frames.insert(seq, payload);
    }

    pub fn pop(&mut self) -> Option<Vec<u8>> {
        if !self.started {
            if self.frames.len() >= self.target_depth {
                self.started = true;
                self.next_seq = self.frames.keys().next().copied();
            } else {
                return None;
            }
        }

        let cur_seq = match self.next_seq {
            Some(seq) => seq,
            None => {
                if let Some(&min_seq) = self.frames.keys().next() {
                    self.next_seq = Some(min_seq);
                    min_seq
                } else {
                    self.started = false;
                    return None;
                }
            }
        };

        if let Some(payload) = self.frames.remove(&cur_seq) {
            self.next_seq = Some(cur_seq.wrapping_add(1));
            self.hole_tick_counter = 0;
            return Some(payload);
        }

        // Current frame sequence is missing (hole)
        self.hole_tick_counter += 1;
        let has_future_frames = self.frames.keys().any(|&k| k > cur_seq);

        if has_future_frames
            && (self.hole_tick_counter >= self.max_hole_ticks || self.frames.len() >= self.target_depth * 2)
        {
            // Skip missing frame and move next_seq forward
            let next = cur_seq.wrapping_add(1);
            self.next_seq = Some(next);
            self.hole_tick_counter = 0;
            return self.pop();
        }

        if self.frames.is_empty() {
            self.started = false;
            self.next_seq = None;
            self.hole_tick_counter = 0;
        }

        None
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}

pub struct JitterBufferManager {
    target_depth: usize,
    buffers: HashMap<String, PeerJitterBuffer>,
}

impl JitterBufferManager {
    pub fn new(target_depth: usize) -> Self {
        Self {
            target_depth,
            buffers: HashMap::new(),
        }
    }

    pub fn push_frame(&mut self, peer_id: &str, seq: u32, payload: Vec<u8>) {
        let buffer = self
            .buffers
            .entry(peer_id.to_string())
            .or_insert_with(|| PeerJitterBuffer::new(self.target_depth));
        buffer.push(seq, payload);
    }

    pub fn pop_frame_for_peer(&mut self, peer_id: &str) -> Option<Vec<u8>> {
        if let Some(buffer) = self.buffers.get_mut(peer_id) {
            buffer.pop()
        } else {
            None
        }
    }

    pub fn active_peers(&self) -> Vec<String> {
        self.buffers.keys().cloned().collect()
    }

    pub fn remove_peer(&mut self, peer_id: &str) {
        self.buffers.remove(peer_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jitter_buffer_depth_and_ordering() {
        let mut jb = PeerJitterBuffer::new(3);

        // Push out of order frames
        jb.push(2, vec![2]);
        jb.push(0, vec![0]);
        jb.push(1, vec![1]);

        // Depth threshold (3) is reached, popping should return sequence 0, 1, 2 in order
        assert_eq!(jb.pop(), Some(vec![0]));
        assert_eq!(jb.pop(), Some(vec![1]));
        assert_eq!(jb.pop(), Some(vec![2]));
        assert_eq!(jb.pop(), None);
    }

    #[test]
    fn test_jitter_buffer_waits_for_target_depth() {
        let mut jb = PeerJitterBuffer::new(3);

        jb.push(0, vec![0]);
        jb.push(1, vec![1]);

        // Depth is 2 (< 3), pop should return None (buffering)
        assert_eq!(jb.pop(), None);

        // Push 3rd frame
        jb.push(2, vec![2]);

        // Now depth is 3, pop should start delivering 0
        assert_eq!(jb.pop(), Some(vec![0]));
    }

    #[test]
    fn test_jitter_buffer_hole_skipping() {
        let mut jb = PeerJitterBuffer::new(3);

        // Send 0, skip 1, send 2, 3
        jb.push(0, vec![0]);
        jb.push(2, vec![2]);
        jb.push(3, vec![3]);

        assert_eq!(jb.pop(), Some(vec![0]));

        // Next expected is 1, which is missing.
        // Tick 1: frame 1 missing, returns None while waiting for hole timeout
        assert_eq!(jb.pop(), None);

        // Tick 2: max_hole_ticks reached (2), missing frame 1 is skipped, pops frame 2
        assert_eq!(jb.pop(), Some(vec![2]));
        assert_eq!(jb.pop(), Some(vec![3]));
    }
}
