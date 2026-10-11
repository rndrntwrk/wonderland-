// SPDX-License-Identifier: MPL-2.0
use crate::{Error, ErrorKind, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Limits {
    pub max_input_bytes: usize,
    pub max_entries: usize,
    pub max_resource_bytes: usize,
    pub max_total_decoded_bytes: usize,
    pub max_string_bytes: usize,
    pub max_pixels: usize,
    pub max_vertices: usize,
    pub max_frames: usize,
    pub max_depth: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_input_bytes: 512 * 1024 * 1024,
            max_entries: 100_000,
            max_resource_bytes: 64 * 1024 * 1024,
            max_total_decoded_bytes: 512 * 1024 * 1024,
            max_string_bytes: 1024 * 1024,
            max_pixels: 16 * 1024 * 1024,
            max_vertices: 1024 * 1024,
            max_frames: 100_000,
            max_depth: 128,
        }
    }
}

impl Limits {
    pub fn check_input(&self, bytes: &[u8]) -> Result<()> {
        self.check_count(bytes.len(), self.max_input_bytes, 0, "input bytes")
    }

    pub fn check_count(
        &self,
        count: usize,
        limit: usize,
        offset: usize,
        context: &str,
    ) -> Result<()> {
        if count > limit {
            return Err(Error::new(ErrorKind::LimitExceeded, offset, context));
        }
        Ok(())
    }
}
