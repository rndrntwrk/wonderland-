//! FreeSO VMContext.NextRandom, with no operating-system entropy.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimRng {
    state: u64,
}

impl SimRng {
    pub const fn new(state: u64) -> Self {
        Self { state }
    }

    pub const fn state(&self) -> u64 {
        self.state
    }

    /// A zero bound leaves the state unchanged. Every nonzero bound, including
    /// one, performs all three XOR shifts. The stored state is not multiplied.
    pub fn next(&mut self, max: u64) -> u64 {
        if max == 0 {
            return 0;
        }
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        self.state.wrapping_mul(2685821657736338717) % max
    }

    /// VMScheduler.RunTick's post-execution, pre-deletion RNG adjustment.
    /// The caller invokes this once, including a tick with no runnable objects.
    pub fn mix_entity_count(&mut self, count: usize) {
        self.state = self.state.wrapping_add(count as u64);
    }
}
