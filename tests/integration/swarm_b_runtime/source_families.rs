//! Characterization at the B import/real-runtime boundary. These are unmodified
//! source routines in a declared harness, not authored replacements for gameplay.
#[path = "support/families.rs"]
mod families;

use families::{FamilyHarness, SOURCES};
use wonderland_content_runtime_bridge::sim_core::vm::VmMode;

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn source_chair_room_impact_runs_in_real_accepted_ticks() {
    for mode in [VmMode::Ts1, VmMode::Tso] {
        let harness = FamilyHarness::load(&root(), &SOURCES[0], mode).unwrap();
        let result = harness.execute(4122, false).unwrap();
        assert_eq!(result["query"]["instructions"], 2);
        assert_eq!(result["query"]["stop"]["Completed"], "ReturnTrue");
        assert_eq!(result["accepted"]["object"]["object_data"][7], 30);
        assert_eq!(result["accepted"]["stop"]["Completed"], "ReturnTrue");
        assert_eq!(result["accepted"]["authority_matches"], true);
        assert_eq!(result["query"]["snapshot_unchanged"], true);
    }
}

#[test]
fn source_bed_sleep_begin_and_end_write_the_actual_avatar_motive() {
    for mode in [VmMode::Ts1, VmMode::Tso] {
        let harness = FamilyHarness::load(&root(), &SOURCES[1], mode).unwrap();
        let begin = harness.execute(4114, true).unwrap();
        assert_eq!(begin["query"]["instructions"], 3);
        assert_eq!(begin["accepted"]["actor"]["motives"][11], -40);
        // VMAvatar.Tick increments TickCounter after the behavior resets it.
        assert_eq!(begin["accepted"]["actor"]["person_data"][27], 1);
        assert_eq!(begin["accepted"]["actor"]["person_data"][28], 0);
        let end = harness.execute_tail(&[(4114, true), (4123, true)]).unwrap();
        assert_eq!(end["actor"]["motives"][11], 0);
        assert_eq!(end["ticks"].as_array().unwrap().len(), 2);
        assert_eq!(end["authority_matches"], true);
        assert_eq!(end["replica_external_dispatches"], 0);
    }
}

#[test]
fn source_appliance_random_query_discards_rng_and_accepted_replay_keeps_it() {
    for mode in [VmMode::Ts1, VmMode::Tso] {
        let harness = FamilyHarness::load(&root(), &SOURCES[2], mode).unwrap();
        let result = harness.execute(4102, false).unwrap();
        // Source VMContext RNG: initial 123 + two spawned entities = 125;
        // XOR-shift state = 4,194,304,098; multiplied value modulo four = 2.
        assert_eq!(result["query"]["temps"][0], 2);
        assert_eq!(result["query"]["instructions"], 3);
        assert_eq!(result["query"]["snapshot_unchanged"], true);
        assert_eq!(result["accepted"]["rng_after"], 4_194_304_100_u64);
        assert_eq!(result["accepted"]["object"]["attributes"][1], 0);
        assert_eq!(result["accepted"]["authority_matches"], true);
    }
}

#[test]
fn source_entries_keep_missing_global_calls_and_replay_their_real_error() {
    for source in &SOURCES {
        let harness = FamilyHarness::load(&root(), source, VmMode::Tso).unwrap();
        let result = harness.execute(4096, false).unwrap();
        assert!(
            result["query"]["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["MissingRoutine"]["id"] == 280),
            "{}",
            source.name
        );
        assert_eq!(result["query"]["stop"]["Completed"], "Error");
        assert_eq!(result["accepted"]["authority_matches"], true);
        assert!(!harness.unresolved_calls().is_empty());
    }
}

#[test]
fn every_family_routine_is_evaluated_without_inventing_missing_dependencies() {
    let result = families::run(&root()).unwrap();
    let family_rows = result["families"].as_array().unwrap();
    assert_eq!(family_rows.len(), 6); // Three sources in both actual VM dialects.
    for family in family_rows {
        let ids = family["routine_count"].as_u64().unwrap();
        assert!(ids > 20);
        assert_eq!(
            family["executions"].as_array().unwrap().len() as u64,
            ids * 2
        );
        assert_eq!(family["complete_gameplay"], false);
        for execution in family["executions"].as_array().unwrap() {
            assert_eq!(execution["query"]["snapshot_unchanged"], true);
            assert_eq!(execution["accepted"]["authority_matches"], true);
            assert_eq!(execution["accepted"]["replica_external_dispatches"], 0);
        }
    }
}
