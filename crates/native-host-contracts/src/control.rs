//! Private staged guard state only. No admission or mutation is authorized here.
use crate::DecimalId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawControlStateV2", into = "RawControlStateV2")]
pub struct ControlStateV2 {
    revision: DecimalId,
}
impl ControlStateV2 {
    pub fn new(revision: DecimalId) -> Self {
        Self { revision }
    }
    pub fn revision(&self) -> DecimalId {
        self.revision
    }
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Mode {
    ControlledV2,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawControlStateV2 {
    schema_version: u32,
    mode: Mode,
    revision: DecimalId,
}
impl TryFrom<RawControlStateV2> for ControlStateV2 {
    type Error = &'static str;
    fn try_from(raw: RawControlStateV2) -> Result<Self, Self::Error> {
        if raw.schema_version != 2 {
            return Err("unsupported controlled lot state");
        }
        Ok(Self::new(raw.revision))
    }
}
impl From<ControlStateV2> for RawControlStateV2 {
    fn from(state: ControlStateV2) -> Self {
        Self {
            schema_version: 2,
            mode: Mode::ControlledV2,
            revision: state.revision,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    #[test]
    fn controlled_state_golden_vectors_are_strict_and_exact() {
        let vectors: Vec<Value> = serde_json::from_str(include_str!(
            "../../../fixtures/native/control-state-v2.json"
        ))
        .unwrap();
        for case in vectors {
            let state = serde_json::from_value::<ControlStateV2>(case["value"].clone());
            assert_eq!(
                state.is_ok(),
                case["valid"].as_bool().unwrap(),
                "{}",
                case["name"]
            );
            if let Ok(state) = state {
                assert_eq!(serde_json::to_value(state).unwrap(), case["value"]);
            }
        }
    }
    #[test]
    fn max_revision_round_trips_without_narrowing() {
        let state = ControlStateV2::new(DecimalId(u64::MAX));
        let recovered: ControlStateV2 =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        assert_eq!(recovered.revision(), DecimalId(u64::MAX));
    }
}
