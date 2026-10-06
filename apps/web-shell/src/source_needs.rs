//! The original VM's eight displayed motives in one shared presentation order.
//!
//! `TSOClient/tso.simantics/Model/VMMotive.cs` defines the serialized indices.
//! Account projections retain signed source values; each visual meter chooses
//! its own display range without changing or inventing the received state.

pub const SOURCE_NEED_LABELS: [&str; 8] = [
    "Energy", "Comfort", "Hunger", "Hygiene", "Bladder", "Room", "Social", "Fun",
];

const SOURCE_MOTIVE_INDICES: [usize; 8] = [5, 6, 7, 8, 9, 13, 14, 15];

pub fn source_needs(motives: &[i16]) -> Option<[i16; 8]> {
    let mut values = [0; 8];
    for (value, index) in values.iter_mut().zip(SOURCE_MOTIVE_INDICES) {
        *value = *motives.get(index)?;
    }
    Some(values)
}
