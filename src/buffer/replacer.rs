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
                // millisecond when a thread marked it as pinned.
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
        None // Out of memory? 
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
