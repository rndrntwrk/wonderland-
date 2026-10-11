use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One flattened resource record, in motion/time-property/item encounter order.
/// The source's unused `OrderBy` does not sort this sequence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeProperty {
    pub time_ms: u32,
    pub properties: BTreeMap<String, String>,
}

impl TimeProperty {
    pub fn xevt(time_ms: u32, event: i16) -> Self {
        Self {
            time_ms,
            properties: BTreeMap::from([("xevt".into(), event.to_string())]),
        }
    }
}

/// Presentation cues are outputs, never acknowledgements required by the VM.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnimationCue {
    Xevt { animation: usize, code: i16 },
    RightHand(i16),
    LeftHand(i16),
    Sound(String),
    Dress(String),
    Undress(String),
}
