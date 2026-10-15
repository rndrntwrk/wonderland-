//! Cross-language contract qualification only, not an actor/admission provider.
use serde_json::{Value, json};
use wonderland_native_host_contracts::ControlStateV2;
fn main() {
    let vectors: Vec<Value> = serde_json::from_str(include_str!(
        "../../../fixtures/native/control-state-v2.json"
    ))
    .unwrap();
    let results: Vec<Value> = vectors
        .iter()
        .map(|case| {
            let parsed = serde_json::from_value::<ControlStateV2>(case["value"].clone());
            assert_eq!(
                parsed.is_ok(),
                case["valid"].as_bool().unwrap(),
                "{}",
                case["name"]
            );
            let canonical = parsed
                .ok()
                .map(|state| serde_json::to_value(state).unwrap().to_string());
            json!({"name":case["name"],"accepted":canonical.is_some(),"canonical":canonical})
        })
        .collect();
    println!("{}", serde_json::to_string(&results).unwrap());
}
