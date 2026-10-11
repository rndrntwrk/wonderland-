// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
#![allow(dead_code)]
//! Compile the exact module nested as Swarm F will nest it, without editing or
//! claiming a shipping sim-core root or a working simulation.
#[path = "../../crates/sim-core/src/interactions/mod.rs"]
mod interactions;

#[test]
fn actual_interactions_source_can_be_nested_under_a_simulation_module() {
    let owner = interactions::EntityKey {
        slot: 1,
        generation: 1,
    };
    let queue = interactions::ActionQueue::new(
        owner,
        interactions::LegacyMode::Tso,
        interactions::InteractionLimits::default(),
    )
    .unwrap();
    assert_eq!(queue.owner(), owner);
    assert!(queue.entries().is_empty());
}
