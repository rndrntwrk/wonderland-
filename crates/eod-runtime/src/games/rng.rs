// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Native SplitMix64 stream, explicitly not System.Random sequence parity.
use super::{Reader, Writer};
use crate::Error;

#[derive(Clone)]
pub(crate) struct NativeRng {
    state: u64,
    draws: u64,
}
impl NativeRng {
    pub(crate) fn new(seed: u64) -> Self {
        Self {
            state: seed,
            draws: 0,
        }
    }
    pub(crate) fn below(&mut self, upper: usize) -> Result<usize, Error> {
        if upper == 0 || upper > 4096 {
            return Err(Error::InvalidPluginInput);
        }
        let bound = upper as u64;
        let threshold = bound.wrapping_neg() % bound;
        // Every call has bounded work, even for a corrupted or hostile test seed.
        for _ in 0..32 {
            // MAX is reserved as invalid by the private checkpoint schema.
            // Never commit a draw that would make successful state unrestorable.
            self.draws = self
                .draws
                .checked_add(1)
                .filter(|draws| *draws != u64::MAX)
                .ok_or(Error::CounterExhausted)?;
            self.state = self.state.wrapping_add(0x9e3779b97f4a7c15);
            let mut word = self.state;
            word = (word ^ (word >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            word = (word ^ (word >> 27)).wrapping_mul(0x94d049bb133111eb);
            word ^= word >> 31;
            if word >= threshold {
                return Ok((word % bound) as usize);
            }
        }
        Err(Error::InvalidPluginInput)
    }
    pub(crate) fn save(&self, writer: &mut Writer) {
        writer.u64(self.state);
        writer.u64(self.draws);
    }
    pub(crate) fn restore(reader: &mut Reader<'_>) -> Result<Self, Error> {
        let result = Self {
            state: reader.u64()?,
            draws: reader.u64()?,
        };
        if result.draws == u64::MAX {
            return Err(Error::InvalidCheckpoint);
        }
        Ok(result)
    }
}
