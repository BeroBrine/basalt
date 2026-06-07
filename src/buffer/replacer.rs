use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

const STATE_PINNED: u8 = 0; // the page is pinned , clock hand ignores this
const STATE_HOT: u8 = 1; // unpinned but recently used
const STATE_COLD: u8 = 2; // sent for eviction

// using trait dispatches allows for the db engine to hotplug replacement policies during the
// runtime.
pub trait Replacer: Send + Sync {
    fn victim(&self) -> Option<usize>;
    fn pin(&self, frame_id: usize);
    fn unpin(&self, frame_id: usize);
    fn size(&self) -> usize;
}
pub struct ClockReplacer {
    // making the states atomic for concurrency control.
    states: Vec<AtomicU8>,
    clock_hand: AtomicUsize,
    capacity: usize,
}

impl ClockReplacer {
    pub fn new(capacity: usize) -> Self {
        let mut states: Vec<AtomicU8> = Vec::new();
        // initializes all frames as empty , which is functionally equivalent to pinned
        for _ in 0..capacity {
            states.push(AtomicU8::new(STATE_PINNED));
        }

        ClockReplacer {
            states,
            clock_hand: AtomicUsize::new(0),
            capacity,
        }
    }
}

impl Replacer for ClockReplacer {
    fn victim(&self) -> Option<usize> {
        let no_of_sweeps = 2 * self.capacity; // 2 sweeps are proven to be optimal in clock replacement.
        let mut sweeps = 0;

        while sweeps < no_of_sweeps {
            // read the state of the current frame and atomically increment the clock hand
            let clock_hand = self.clock_hand.fetch_add(1, Ordering::Relaxed) % self.capacity;
            let current_frame_state = self.states[clock_hand].load(Ordering::Relaxed);
            if current_frame_state == STATE_HOT {
                // downgrade to cold state as it was recently used
                // compare exchange is used so that we dont accidentaly overwrite at the last
                // millisecond when a thread marks it as pinned.
                let _ = self.states[clock_hand].compare_exchange(
                    current_frame_state,
                    STATE_COLD,
                    Ordering::Release,
                    Ordering::Relaxed,
                );
            }
            // found a cold frame and thread was able to pin it. evict the frame.
            else if current_frame_state == STATE_COLD
                && self.states[clock_hand]
                    .compare_exchange(
                        current_frame_state,
                        STATE_PINNED,
                        Ordering::Release,
                        Ordering::Relaxed,
                    )
                    .is_ok()
            {
                // clock hand represents the current frame. we need to return this frame to the
                // buffer manager
                return Some(clock_hand);
            }
            // if compare exchange failed , another thread stole this operation therefore we
            // move to the next frame

            sweeps += 1;
        }
        None // Out of memory? yahoooooooooooo 
    }

    fn unpin(&self, frame_id: usize) {
        self.states[frame_id].store(STATE_HOT, Ordering::Release);
    }

    fn pin(&self, frame_id: usize) {
        self.states[frame_id].store(STATE_PINNED, Ordering::Release);
    }

    fn size(&self) -> usize {
        // count frames that are not pinned.
        self.states
            .iter()
            .filter(|x| x.load(Ordering::Relaxed) != STATE_PINNED)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clock_replacer_initialization() {
        let replacer = ClockReplacer::new(10);
        assert_eq!(replacer.size(), 0);
    }

    #[test]
    fn test_clock_replacer_unpin() {
        let replacer = ClockReplacer::new(10);
        replacer.unpin(1);
        replacer.unpin(2);
        replacer.unpin(3);
        assert_eq!(replacer.size(), 3);
    }

    #[test]
    fn test_clock_replacer_pin() {
        let replacer = ClockReplacer::new(10);
        replacer.unpin(1);
        replacer.unpin(2);
        replacer.unpin(3);
        assert_eq!(replacer.size(), 3);
        replacer.pin(2);
        assert_eq!(replacer.size(), 2);
    }

    #[test]
    fn test_clock_replacer_victim() {
        let replacer = ClockReplacer::new(3);
        replacer.unpin(0);
        replacer.unpin(1);
        replacer.unpin(2);
        assert_eq!(replacer.size(), 3);

        // First sweep: all HOT -> COLD
        // After first sweep, hand is at 0 again (if it started at 0)
        // Actually fetch_add happens before returning the value, so hand is incremented.
        
        // Let's trace:
        // victim() starts
        // clock_hand = 0, state[0] HOT -> COLD, hand = 1
        // clock_hand = 1, state[1] HOT -> COLD, hand = 2
        // clock_hand = 2, state[2] HOT -> COLD, hand = 3 (mod 3 = 0)
        // sweeps = 3. no_of_sweeps = 6.
        // clock_hand = 0, state[0] is COLD, returns Some(0), state[0] pinned.
        
        assert_eq!(replacer.victim(), Some(0));
        assert_eq!(replacer.size(), 2);

        assert_eq!(replacer.victim(), Some(1));
        assert_eq!(replacer.size(), 1);

        assert_eq!(replacer.victim(), Some(2));
        assert_eq!(replacer.size(), 0);

        assert_eq!(replacer.victim(), None);
    }

    #[test]
    fn test_clock_replacer_complex() {
        let replacer = ClockReplacer::new(10);

        for i in 0..10 {
            replacer.unpin(i);
        }
        assert_eq!(replacer.size(), 10);

        replacer.pin(0);
        replacer.pin(4);
        replacer.pin(8);
        assert_eq!(replacer.size(), 7);

        // Victim should skip pinned ones
        // hand starts at 0. 
        // 0 is PINNED, skip. hand=1
        // 1 is HOT->COLD, hand=2
        // 2 is HOT->COLD, hand=3
        // 3 is HOT->COLD, hand=4
        // 4 is PINNED, skip. hand=5
        // 5 is HOT->COLD, hand=6
        // 6 is HOT->COLD, hand=7
        // 7 is HOT->COLD, hand=8
        // 8 is PINNED, skip. hand=9
        // 9 is HOT->COLD, hand=0
        
        // Second sweep:
        // 0 PINNED, skip
        // 1 COLD -> PINNED, return Some(1)
        assert_eq!(replacer.victim(), Some(1));
        assert_eq!(replacer.victim(), Some(2));
        assert_eq!(replacer.victim(), Some(3));
        assert_eq!(replacer.victim(), Some(5));
        assert_eq!(replacer.victim(), Some(6));
        assert_eq!(replacer.victim(), Some(7));
        assert_eq!(replacer.victim(), Some(9));
        assert_eq!(replacer.victim(), None);
    }
}
