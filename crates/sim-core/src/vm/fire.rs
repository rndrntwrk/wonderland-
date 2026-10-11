//! Source fire tuning/clock facts supplied by the runtime's deterministic world adapter.
use super::VmFault;
use serde::{Deserialize, Serialize};

pub const MAX_FIRE_TILES: usize = 1_048_576;
pub const MAX_FIRE_GROUP_VISITS: usize = 65_536;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FireState {
    pub enabled: bool,
    pub percent: i32,
    pub width: u16,
    pub height: u16,
}
impl FireState {
    pub fn tile_count(self) -> Result<usize, VmFault> {
        let count = usize::from(self.width) * usize::from(self.height);
        if count == 0 || count > MAX_FIRE_TILES {
            Err(VmFault::InvalidContent("Fire grid exceeds bounds".into()))
        } else {
            Ok(count)
        }
    }
}
