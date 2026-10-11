use crate::{AudioError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use wonderland_render_core::EntityRef;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CueId {
    pub lot_id: u64,
    pub timeline: u64,
    pub tick: u64,
    pub event_ordinal: u32,
    pub nested_ordinal: u32,
    pub owner: Option<EntityRef>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CueAction {
    Play { event: String, looped: bool },
    StopOwner,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioCue {
    pub id: CueId,
    pub action: CueAction,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CueAdmission {
    New,
    Duplicate,
    Retired,
}
/// Bounded causal identities. Delivery/reconnect epochs deliberately do not occur
/// in `CueId`: callers validate transport authority before presenting a cue.
#[derive(Debug)]
pub struct CueLedger {
    lot: u64,
    timeline: u64,
    capacity: usize,
    retired: Option<u64>,
    seen: BTreeSet<CueId>,
}
impl CueLedger {
    pub fn new(lot: u64, timeline: u64, capacity: usize) -> Result<Self> {
        if capacity == 0 {
            return Err(AudioError::Invalid("cue capacity"));
        }
        Ok(Self {
            lot,
            timeline,
            capacity,
            retired: None,
            seen: BTreeSet::new(),
        })
    }
    /// Validate without recording, for transactions that may still be rejected.
    pub fn check(&self, id: &CueId) -> Result<CueAdmission> {
        if id.lot_id != self.lot || id.timeline != self.timeline {
            return Err(AudioError::Stale);
        }
        if id.owner.is_some_and(|x| x.generation == 0) {
            return Err(AudioError::Invalid("owner generation"));
        }
        if self.retired.is_some_and(|t| id.tick <= t) {
            return Ok(CueAdmission::Retired);
        }
        if self.seen.contains(id) {
            return Ok(CueAdmission::Duplicate);
        }
        if self.seen.len() == self.capacity {
            return Err(AudioError::Limit("unretired cues"));
        }
        Ok(CueAdmission::New)
    }
    pub fn admit(&mut self, id: &CueId) -> Result<CueAdmission> {
        let admission = self.check(id)?;
        if admission == CueAdmission::New {
            self.seen.insert(id.clone());
        }
        Ok(admission)
    }
    /// Only advance after the accepted timeline has reconciled through `tick`.
    /// An out-of-order delivery above this watermark is still admitted.
    pub fn retire_through(&mut self, tick: u64) -> Result<()> {
        if self.retired.is_some_and(|old| tick < old) {
            return Err(AudioError::Stale);
        }
        self.retired = Some(tick);
        self.seen.retain(|id| id.tick > tick);
        Ok(())
    }
    pub fn len(&self) -> usize {
        self.seen.len()
    }
    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}
