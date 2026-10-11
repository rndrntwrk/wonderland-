// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//! All behavior in this file is SYNTHETIC FIXTURE ONLY. No BHAV or object runs.
#![allow(dead_code)]

use sim_core::interactions as wonderland_interactions_check;
use std::collections::BTreeSet;
use wonderland_interactions_check::adapters::*;
use wonderland_interactions_check::*;

pub fn actor_key() -> EntityKey {
    EntityKey {
        slot: 1,
        generation: 4,
    }
}
pub fn target_key() -> EntityKey {
    EntityKey {
        slot: 2,
        generation: 9,
    }
}
pub fn actor_version() -> EntityVersion {
    EntityVersion {
        key: actor_key(),
        revision: 10,
    }
}
pub fn target_version() -> EntityVersion {
    EntityVersion {
        key: target_key(),
        revision: 20,
    }
}

pub fn actor() -> ActorFacts {
    ActorFacts {
        version: actor_version(),
        is_avatar: true,
        species: Species::Human,
        permission: AvatarPermission::Visitor,
        carrying: false,
        ghost: false,
        owns_target: false,
        ts1_ungreeted_visitor: true,
        age: 25,
    }
}

pub fn target() -> TargetFacts {
    TargetFacts {
        version: target_version(),
        is_game_object: true,
        broken: false,
        disabled: false,
    }
}

pub fn key(index: u32) -> InteractionKey {
    InteractionKey {
        tta_index: index,
        scope: InteractionScope::Local,
    }
}

pub fn definition(index: u32) -> InteractionDefinition {
    InteractionDefinition {
        key: key(index),
        action: RoutineBinding {
            routine_id: 4096,
            code_owner_guid: 0x12345678,
        },
        check: Some(RoutineBinding {
            routine_id: 4097,
            code_owner_guid: 0x12345678,
        }),
        flags: ActionFlags(ActionFlags::ALLOW_VISITORS),
        permissions: PermissionFlags(0x1e),
        label: Some(format!("Fixture {index}")),
    }
}

pub fn snapshot(mode: LegacyMode, definitions: Vec<InteractionDefinition>) -> InteractionSnapshot {
    let limits = InteractionLimits::default();
    let mut state = CheckState::default();
    state.rng_seed = 0x12345678;
    state.temp_registers = [10; 20];
    state.temp_xl = [50, 60];
    state.set_provider_state(&[7, 8, 9], &limits).unwrap();
    state.set_advertisement(99, 100, &limits).unwrap();
    let mut snapshot =
        InteractionSnapshot::new(mode, 100, actor(), target(), state, &limits).unwrap();
    for definition in definitions {
        snapshot.add_definition(definition, &limits).unwrap();
    }
    snapshot
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixtureWorld {
    pub live: InteractionSnapshot,
    pub revision: u64,
    pub live_actor: Option<EntityVersion>,
    pub live_target: Option<EntityVersion>,
    pub allowed: bool,
}

impl FixtureWorld {
    pub fn new(definitions: Vec<InteractionDefinition>) -> Self {
        Self {
            live: snapshot(LegacyMode::Tso, definitions),
            revision: 100,
            live_actor: Some(actor_version()),
            live_target: Some(target_version()),
            allowed: true,
        }
    }

    pub fn query(&self) -> OfferQuery {
        OfferQuery {
            principal: PrincipalKey(55),
            seen: self.live.stamp(),
            options: QueryOptions::default(),
        }
    }

    pub fn intent(
        &self,
        queue: &ActionQueue,
        sequence: u64,
        index: u32,
        param0: i16,
    ) -> InteractionIntent {
        InteractionIntent {
            principal: PrincipalKey(55),
            seen: self.live.stamp(),
            queue_revision: queue.revision(),
            command_sequence: sequence,
            interaction: key(index),
            param0,
        }
    }

    pub fn cancel(&self, queue: &ActionQueue, sequence: u64, action: ActionId) -> CancelIntent {
        CancelIntent {
            principal: PrincipalKey(55),
            world_revision: self.revision,
            actor: actor_version(),
            queue_revision: queue.revision(),
            command_sequence: sequence,
            action,
        }
    }
}

impl WorldProvider for FixtureWorld {
    fn revision(&self) -> u64 {
        self.revision
    }
    fn entity_version(&self, key: EntityKey) -> Option<EntityVersion> {
        [self.live_actor, self.live_target]
            .into_iter()
            .flatten()
            .find(|version| version.key == key)
    }
    fn authorize(&self, principal: PrincipalKey, actor: EntityKey, _: AuthorityOperation) -> bool {
        self.allowed && principal == PrincipalKey(55) && actor == actor_key()
    }
    fn snapshot(
        &self,
        _: EntityKey,
        _: EntityKey,
        _: &InteractionLimits,
    ) -> Result<InteractionSnapshot> {
        Ok(self.live.clone())
    }
}

#[derive(Clone, Copy, Debug)]
pub enum FixtureBehavior {
    Advertise,
    Variants,
    HideFirst,
    RequireUnoccupied,
    Reject,
    Yield,
    Abort,
    Error,
    IgnoreVariantBound,
    IgnoreBudget,
}

pub struct FixtureChecks {
    pub behavior: FixtureBehavior,
    pub expected_args: [i16; 4],
    pub expected_origin: Option<CheckOrigin>,
}

impl Default for FixtureChecks {
    fn default() -> Self {
        Self {
            behavior: FixtureBehavior::Advertise,
            expected_args: [0; 4],
            expected_origin: None,
        }
    }
}

impl CheckTreeProvider for FixtureChecks {
    fn evaluate(
        &self,
        request: CheckRequest<'_>,
        state: &mut CheckState,
        output: &mut CheckOutput,
        budget: &mut CheckBudget,
    ) -> Result<CheckExit> {
        assert_eq!(request.args, self.expected_args, "source CheckAction args");
        if let Some(expected) = self.expected_origin {
            assert_eq!(request.origin, expected);
        }
        assert_eq!(request.actor, actor_key());
        assert_eq!(request.target, target_key());
        budget.spend(1)?;
        state.random_below(1000);
        state.temp_registers[3] += 1;
        state.temp_xl[1] += 2;
        if !state.provider_state().is_empty() {
            state.provider_state_mut()[0] += 1;
        }
        state.set_advertisement(7, 15, &InteractionLimits::default())?;
        state.set_advertisement(2, 3, &InteractionLimits::default())?;
        match self.behavior {
            FixtureBehavior::Advertise => {}
            FixtureBehavior::Variants => {
                output.push_variant(Some("Fixture dynamic label"), 7)?;
                output.push_variant(None, -2)?;
            }
            FixtureBehavior::HideFirst => {
                if request.definition.key.tta_index == 1 {
                    state.hide_interaction = true;
                }
            }
            FixtureBehavior::RequireUnoccupied => {
                if state.target_occupied {
                    return Ok(CheckExit::ReturnFalse);
                }
            }
            FixtureBehavior::Reject => return Ok(CheckExit::ReturnFalse),
            FixtureBehavior::Yield => return Ok(CheckExit::Yielded),
            FixtureBehavior::Abort => return Ok(CheckExit::Aborted),
            FixtureBehavior::Error => {
                return Err(Error::RuntimeFailure("fixture interpreter error"))
            }
            FixtureBehavior::IgnoreVariantBound => {
                for _ in 0..3 {
                    let _ = output.push_variant(Some("bounded fixture"), 1);
                }
            }
            FixtureBehavior::IgnoreBudget => {
                let _ = budget.spend(u64::MAX);
            }
        }
        Ok(CheckExit::ReturnTrue)
    }
}

#[derive(Default)]
pub struct FixtureRuntime {
    pub dead: BTreeSet<EntityKey>,
    pub reject_check: BTreeSet<u32>,
    pub refuse_start: BTreeSet<u32>,
    pub check_errors: BTreeSet<u32>,
    pub start_errors: BTreeSet<u32>,
    pub checked: Vec<u32>,
    pub frames: Vec<u32>,
}

impl QueueRuntime for FixtureRuntime {
    fn target_is_alive(&self, target: EntityKey) -> bool {
        !self.dead.contains(&target)
    }
    fn check_action(&mut self, action: &ActionInvocation) -> Result<bool> {
        let key = action.definition.key.tta_index;
        self.checked.push(key);
        if self.check_errors.contains(&key) {
            return Err(Error::RuntimeFailure("fixture check error"));
        }
        Ok(!self.reject_check.contains(&key))
    }
    fn start_action(&mut self, action: &ActionInvocation) -> Result<bool> {
        let key = action.definition.key.tta_index;
        if self.start_errors.contains(&key) {
            return Err(Error::RuntimeFailure("fixture start error"));
        }
        if self.refuse_start.contains(&key) {
            return Ok(false);
        }
        self.frames.push(key);
        Ok(true)
    }
}

pub fn invocation(index: u32, priority: i16, mode: QueueMode, flags: u32) -> ActionInvocation {
    let mut definition = definition(index);
    definition.flags.0 |= flags;
    ActionInvocation {
        actor: actor_key(),
        target: target_key(),
        stack_object: target_key(),
        icon_owner: None,
        definition,
        args: [0; 4],
        priority,
        mode,
        callback: None,
        interaction_result: -1,
        result_check_counter: 0,
    }
}

pub fn indices(queue: &ActionQueue) -> Vec<u32> {
    queue
        .entries()
        .iter()
        .map(|entry| entry.invocation.definition.key.tta_index)
        .collect()
}

pub fn queue(mode: LegacyMode) -> ActionQueue {
    ActionQueue::new(actor_key(), mode, InteractionLimits::default()).unwrap()
}
