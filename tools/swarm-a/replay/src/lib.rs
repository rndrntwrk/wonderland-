//! Actual SimRuntime replays shared by a native CLI and an import-free WASM ABI.
//!
//! This fixture supplies synthetic, explicit BHAV/content records. Matching this
//! harness proves cross-target consistency for these cases, not legacy-content
//! completeness or a comparison against the complete C# engine.
#![deny(unsafe_code)]

mod scenarios;

use sha2::{Digest, Sha256};
use std::cell::RefCell;

pub use scenarios::run_scenario;

pub const REPORT_MAGIC: &[u8; 8] = b"SWARMRP1";
pub const SCENARIOS: [(u32, &str, u32); 4] = [
    (0, "bhav-stack-sleep-rng", 96),
    (1, "avatar-animation-motives", 181),
    (2, "effect-fenced-retry", 24),
    (3, "portal-route-callback", 64),
];
pub const SEEDS: [u64; 4] = [0, 1, 123, 0xfedc_ba98_7654_3210];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TickRecord {
    pub tick: u64,
    pub epoch: u64,
    pub rng: u64,
    pub instructions: u32,
    pub accepted_hash: [u8; 32],
    pub state_hash: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotRecord {
    pub label: String,
    pub tick: u64,
    pub epoch: u64,
    pub state_hash: [u8; 32],
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Probe {
    pub name: String,
    pub value: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayReport {
    pub scenario: u32,
    pub seed: u64,
    pub ticks: Vec<TickRecord>,
    pub snapshots: Vec<SnapshotRecord>,
    /// Asserted semantic observations, rather than platform-dependent log text.
    pub probes: Vec<Probe>,
}

impl ReplayReport {
    /// The small outer format is independent of usize and serde enum layouts.
    /// The enclosed snapshots are the unmodified sim-core snapshot bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        fn u32_value(out: &mut Vec<u8>, value: u32) {
            out.extend_from_slice(&value.to_le_bytes());
        }
        fn u64_value(out: &mut Vec<u8>, value: u64) {
            out.extend_from_slice(&value.to_le_bytes());
        }
        fn string(out: &mut Vec<u8>, value: &str) {
            u32_value(out, value.len() as u32);
            out.extend_from_slice(value.as_bytes());
        }
        let mut out = REPORT_MAGIC.to_vec();
        u32_value(&mut out, self.scenario);
        u64_value(&mut out, self.seed);
        u32_value(&mut out, self.ticks.len() as u32);
        for tick in &self.ticks {
            u64_value(&mut out, tick.tick);
            u64_value(&mut out, tick.epoch);
            u64_value(&mut out, tick.rng);
            u32_value(&mut out, tick.instructions);
            out.extend_from_slice(&tick.accepted_hash);
            out.extend_from_slice(&tick.state_hash);
        }
        u32_value(&mut out, self.snapshots.len() as u32);
        for snapshot in &self.snapshots {
            string(&mut out, &snapshot.label);
            u64_value(&mut out, snapshot.tick);
            u64_value(&mut out, snapshot.epoch);
            out.extend_from_slice(&snapshot.state_hash);
            u32_value(&mut out, snapshot.bytes.len() as u32);
            out.extend_from_slice(&digest(&snapshot.bytes));
            out.extend_from_slice(&snapshot.bytes);
        }
        u32_value(&mut out, self.probes.len() as u32);
        for probe in &self.probes {
            string(&mut out, &probe.name);
            out.extend_from_slice(&probe.value.to_le_bytes());
        }
        out
    }
}

pub fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(char::from(DIGITS[usize::from(byte >> 4)]));
        result.push(char::from(DIGITS[usize::from(byte & 15)]));
    }
    result
}

thread_local! {
    static OUTPUT: RefCell<Vec<u8>> = RefCell::new(Vec::new());
}

// Rust 1.75's unsafe_code lint includes externally visible no_mangle names.
// The exceptions are only for those export attributes: the implementation has
// no unsafe block, raw pointer, unchecked read or host-memory callback.
#[allow(unsafe_code)]
#[no_mangle]
pub extern "C" fn replay_run(scenario: u32, seed_lo: u32, seed_hi: u32) -> u32 {
    let seed = u64::from(seed_lo) | (u64::from(seed_hi) << 32);
    let (status, bytes) = match run_scenario(scenario, seed) {
        Ok(report) => (0, report.to_bytes()),
        Err(error) => (1, error.into_bytes()),
    };
    OUTPUT.with(|output| *output.borrow_mut() = bytes);
    status
}

#[allow(unsafe_code)]
#[no_mangle]
pub extern "C" fn replay_len() -> u32 {
    OUTPUT.with(|output| output.borrow().len() as u32)
}

/// 0..=255 is a byte; 256 is a stable bounds-error sentinel.
#[allow(unsafe_code)]
#[no_mangle]
pub extern "C" fn replay_byte(index: u32) -> u32 {
    OUTPUT.with(|output| {
        output
            .borrow()
            .get(index as usize)
            .map_or(256, |byte| u32::from(*byte))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn behavior_fixture_retains_nested_sleep_across_post_tick_restore() {
        let report = run_scenario(0, 123).unwrap();
        assert_eq!(report.ticks.len(), 96);
        assert!(report
            .probes
            .iter()
            .any(|p| p.name == "saved_stack_depth" && p.value == 3));
        assert!(report
            .probes
            .iter()
            .any(|p| p.name == "completed_main_calls" && p.value > 0));
        assert_eq!(report.snapshots.last().unwrap().tick, 96);
    }

    #[test]
    fn animation_fixture_executes_event_branches_hurry_and_fractional_motives() {
        let report = run_scenario(1, 123).unwrap();
        assert_eq!(report.ticks.len(), 181);
        assert!(report
            .probes
            .iter()
            .any(|p| p.name == "forward_event_count" && p.value >= 6));
        assert!(report
            .probes
            .iter()
            .any(|p| p.name == "hurry_speed_bits" && p.value == i64::from(2.4f32.to_bits())));
        assert!(report
            .probes
            .iter()
            .any(|p| p.name == "hunger_final" && p.value > -80));
    }

    #[test]
    fn effects_fixture_retries_original_operation_and_applies_once_after_takeover() {
        let report = run_scenario(2, 123).unwrap();
        assert_eq!(report.ticks.len(), 24);
        assert!(report
            .probes
            .iter()
            .any(|p| p.name == "applied_effects" && p.value == 2));
        assert!(report
            .probes
            .iter()
            .any(|p| p.name == "unique_operations" && p.value == 3));
        assert!(report
            .probes
            .iter()
            .any(|p| p.name == "rejected_fenced_deliveries" && p.value == 4));
    }

    #[test]
    fn portal_fixture_restores_suspended_callback_and_resumes_its_vm_once() {
        let report = run_scenario(3, 123).unwrap();
        assert_eq!(report.ticks.len(), 64);
        assert_eq!(report.ticks.first().unwrap().tick, 2);
        assert_eq!(report.ticks.last().unwrap().tick, 65);
        assert_eq!(report.snapshots.len(), 6);
        assert!(report
            .probes
            .iter()
            .any(|p| p.name == "vm_route_success_count" && p.value == 1));
        assert!(report
            .probes
            .iter()
            .any(|p| p.name == "rejected_route_callbacks" && p.value == 5));
        assert!(report
            .probes
            .iter()
            .any(|p| p.name == "portal_final_level" && p.value == 2));
    }

    #[test]
    fn safe_export_replaces_errors_and_checks_every_byte_index() {
        assert_eq!(replay_run(u32::MAX, 0, 0), 1);
        assert!(replay_len() > 0);
        assert_eq!(replay_byte(replay_len()), 256);
        assert_eq!(replay_byte(u32::MAX), 256);
        assert_eq!(hex(&[0, 15, 16, 255]), "000f10ff");
        assert_eq!(
            digest(b"abc"),
            [
                0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
                0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
                0xf2, 0x00, 0x15, 0xad,
            ]
        );
    }
}
