use crate::{digest, Probe, ReplayReport, SnapshotRecord, TickRecord};
use bincode::Options;
use serde::Serialize;
use sim_core::{
    avatars::{
        events::{AnimationCue, TimeProperty},
        motives::Motive,
        timeline::AnimationMetadata,
    },
    effects::{EffectDispatch, EffectResolved, EffectValue},
    ids::{EntityRef, ObjectId, PersistentId},
    primitives::arithmetic::ExpressionOperand,
    runtime::{
        encode_effect_resolution, AcceptedCommand, AcceptedTick, RuntimeConfig, RuntimeEvent,
        RuntimeRole, SimRuntime, SpawnSpec, TickOutcome,
    },
    state::{AnimationKey, ContentSet, ContinuationKind, ObjectDefinition, RoutingSlot, TuningSet},
    vm::*,
    world::{
        routing::{CallbackOutcome, RouteCallback, RouteCallbackKind, RoutePhase},
        slots::SlotSearch,
        Facing, LotModel, LotPosition, Portal, PortalId, RouteContinuation, RouteFailCode, TilePos,
    },
};
use std::{collections::BTreeMap, fmt::Debug};

type Result<T> = std::result::Result<T, String>;
const LOT: u64 = 0x5341_5245_504c_4159;
const BEHAVIOR_GUID: u32 = 100;
const AVATAR_GUID: u32 = 200;
const EFFECT_GUID: u32 = 300;
const ROUTE_ACTOR_GUID: u32 = 400;
const ROUTE_PORTAL_GUID: u32 = 401;
const ROUTE_TARGET_GUID: u32 = 402;

fn error(value: impl Debug) -> String {
    format!("{value:?}")
}
fn require(condition: bool, message: impl Into<String>) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn canonical<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_little_endian()
        .with_limit(64 * 1024 * 1024)
        .serialize(value)
        .map_err(error)
}
fn key(guid: u32, id: u16) -> RoutineKey {
    RoutineKey {
        scope: RoutineScope::Private(guid),
        id,
    }
}
fn variable(scope: Scope, data: i16) -> Variable {
    Variable::new(scope, data)
}
fn expression(lhs: Variable, op: u8, rhs: Variable, next: u8) -> VmInstruction {
    VmInstruction::new(
        2,
        next,
        255,
        ExpressionOperand {
            lhs,
            rhs,
            operator: op,
            is_signed: 0,
        }
        .encode(),
    )
}
fn add_attribute(index: i16, value: i16, next: u8) -> VmInstruction {
    expression(
        variable(Scope::MyObjectAttributes, index),
        3,
        variable(Scope::Literal, value),
        next,
    )
}
fn random(temp: i16, bound: i16, next: u8) -> VmInstruction {
    let mut operand = [0; 8];
    for (offset, value) in [
        (0, temp as u16),
        (2, Scope::Temps as u16),
        (4, bound as u16),
        (6, Scope::Literal as u16),
    ] {
        operand[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    VmInstruction::new(8, next, 255, operand)
}
fn call(id: u16, arguments: [i16; 4], next: u8) -> VmInstruction {
    let mut operand = [0; 8];
    for (at, value) in arguments.into_iter().enumerate() {
        operand[at * 2..at * 2 + 2].copy_from_slice(&value.to_le_bytes());
    }
    VmInstruction::new(id, next, 255, operand)
}
fn insert(
    store: &mut RoutineStore,
    key: RoutineKey,
    instructions: Vec<VmInstruction>,
) -> Result<()> {
    store
        .insert(
            key,
            VmRoutine::new(key.id, 2, 4, instructions).map_err(error)?,
        )
        .map_err(error)
}
fn spawn(guid: u32, x: i16, avatar: bool) -> AcceptedCommand {
    AcceptedCommand::Spawn(SpawnSpec {
        guid,
        position: TilePos::new(x, 3, 1).center(),
        facing: Facing::NORTH,
        persistent_id: PersistentId(if avatar { 1000 + x as u32 } else { 0 }),
        avatar,
    })
}
fn write(entity: EntityRef, field: EntityField, index: u16, value: i16) -> AcceptedCommand {
    AcceptedCommand::WriteMemory {
        address: MemoryAddress::Entity {
            entity,
            field,
            index,
        },
        value,
    }
}
fn start(entity: EntityRef, guid: u32) -> AcceptedCommand {
    AcceptedCommand::StartBehavior {
        entity,
        routine: key(guid, 4096),
        context: FrameContext::for_entity(entity, guid),
        args: vec![3, 0, 0, 0],
        replace: true,
    }
}

/// Each accepted command stream is also replayed through a live replica and a
/// fresh runtime restored from the last post-tick snapshot. Role is never hashed.
struct Driver {
    authority: SimRuntime,
    replica: SimRuntime,
    restored: Option<SimRuntime>,
    report: ReplayReport,
    duplicate_ticks: u32,
    replayed_ticks: u32,
}
impl Driver {
    fn new(scenario: u32, seed: u64, mode: VmMode, content: ContentSet) -> Result<Self> {
        let make = |role| {
            SimRuntime::new(
                content.clone(),
                LotModel::new(16, 16, 1).map_err(error)?,
                RuntimeConfig::new(mode, LOT, 1, seed),
                role,
            )
            .map_err(error)
        };
        Ok(Self {
            authority: make(RuntimeRole::Authority)?,
            replica: make(RuntimeRole::Replica)?,
            restored: None,
            report: ReplayReport {
                scenario,
                seed,
                ticks: vec![],
                snapshots: vec![],
                probes: vec![],
            },
            duplicate_ticks: 0,
            replayed_ticks: 0,
        })
    }
    fn entity(&self, id: i16) -> Result<EntityRef> {
        self.authority
            .state()
            .entities
            .get(&ObjectId(id))
            .map(|entity| entity.info.reference)
            .ok_or_else(|| format!("fixture entity {id} missing"))
    }
    /// Trusted normalized topology may reference entities supplied by an initial
    /// state load. Its full snapshot is recorded before the first replay tick.
    fn from_initialized(scenario: u32, seed: u64, authority: SimRuntime) -> Result<Self> {
        let replica = SimRuntime::from_state(
            authority.state().clone(),
            authority.content().clone(),
            RuntimeRole::Replica,
        )
        .map_err(error)?;
        Ok(Self {
            authority,
            replica,
            restored: None,
            report: ReplayReport {
                scenario,
                seed,
                ticks: vec![],
                snapshots: vec![],
                probes: vec![],
            },
            duplicate_ticks: 0,
            replayed_ticks: 0,
        })
    }
    fn step(&mut self, commands: Vec<AcceptedCommand>) -> Result<TickOutcome> {
        let input = self.authority.next_tick(commands).map_err(error)?;
        let outcome = self.authority.step(&input).map_err(error)?;
        let replica = self.replica.step(&input).map_err(error)?;
        require(
            outcome.state_hash == replica.state_hash
                && self.authority.state() == self.replica.state(),
            format!("authority/replica state differs at tick {}", input.tick),
        )?;
        require(
            outcome.instructions == replica.instructions && outcome.events == replica.events,
            "authority/replica VM output differs",
        )?;
        require(
            replica.effects.is_empty(),
            "replica dispatched an external effect",
        )?;
        for event in &outcome.events {
            if let RuntimeEvent::ThreadFault { fault, .. } = event {
                return Err(format!(
                    "fixture thread fault at tick {}: {fault}",
                    input.tick
                ));
            }
        }
        if let Some(restored) = &mut self.restored {
            let replayed = restored.step(&input).map_err(error)?;
            require(
                outcome.state_hash == replayed.state_hash
                    && self.authority.state() == restored.state(),
                format!("snapshot replay differs at tick {}", input.tick),
            )?;
            require(
                outcome.instructions == replayed.instructions
                    && outcome.events == replayed.events
                    && outcome.effects == replayed.effects,
                "snapshot replay output differs",
            )?;
            self.replayed_ticks += 1;
        }
        if input.tick % 13 == 0 {
            Self::duplicate(&mut self.authority, &input, outcome.state_hash)?;
            Self::duplicate(&mut self.replica, &input, outcome.state_hash)?;
            if let Some(restored) = &mut self.restored {
                Self::duplicate(restored, &input, outcome.state_hash)?;
            }
            self.duplicate_ticks += 1;
        }
        self.report.ticks.push(TickRecord {
            tick: input.tick,
            epoch: input.epoch,
            rng: self.authority.state().rng.state(),
            instructions: outcome.instructions,
            accepted_hash: digest(&canonical(&input)?),
            state_hash: outcome.state_hash,
        });
        Ok(outcome)
    }
    fn duplicate(
        runtime: &mut SimRuntime,
        input: &AcceptedTick,
        expected_hash: [u8; 32],
    ) -> Result<()> {
        let before = runtime.snapshot().map_err(error)?;
        let duplicate = runtime.step(input).map_err(error)?;
        require(
            duplicate.duplicate
                && duplicate.instructions == 0
                && duplicate.state_hash == expected_hash
                && duplicate.events.is_empty()
                && duplicate.effects.is_empty(),
            "accepted tick duplicate had side effects",
        )?;
        require(
            runtime.snapshot().map_err(error)? == before,
            "duplicate changed canonical snapshot",
        )
    }
    fn checkpoint(&mut self, label: &str) -> Result<()> {
        let bytes = self.authority.snapshot().map_err(error)?;
        require(
            bytes == self.replica.snapshot().map_err(error)?,
            "replica snapshot bytes differ",
        )?;
        let state = self.authority.state();
        let config = RuntimeConfig::new(
            state.mode,
            state.lot_id,
            state.authority_epoch,
            self.report.seed,
        );
        let mut restored = SimRuntime::new(
            self.authority.content().clone(),
            LotModel::new(16, 16, 1).map_err(error)?,
            config,
            RuntimeRole::Authority,
        )
        .map_err(error)?;
        restored.restore(&bytes).map_err(error)?;
        require(
            restored.state() == self.authority.state()
                && restored.snapshot().map_err(error)? == bytes,
            "post-tick snapshot did not restore canonically",
        )?;
        self.report.snapshots.push(SnapshotRecord {
            label: label.into(),
            tick: state.completed_tick,
            epoch: state.authority_epoch,
            state_hash: self.authority.state_hash().map_err(error)?,
            bytes,
        });
        self.restored = Some(restored);
        Ok(())
    }
    fn adopt(&mut self, epoch: u64) -> Result<()> {
        self.authority
            .adopt_epoch(epoch, RuntimeRole::Authority)
            .map_err(error)?;
        self.replica
            .adopt_epoch(epoch, RuntimeRole::Replica)
            .map_err(error)?;
        if let Some(restored) = &mut self.restored {
            restored
                .adopt_epoch(epoch, RuntimeRole::Authority)
                .map_err(error)?;
        }
        require(
            self.authority.state() == self.replica.state(),
            "takeover changed replica state differently",
        )
    }
    fn reject(&mut self, input: &AcceptedTick, expected: &str) -> Result<()> {
        fn one(runtime: &mut SimRuntime, input: &AcceptedTick, expected: &str) -> Result<()> {
            let before = runtime.snapshot().map_err(error)?;
            let failure = runtime
                .step(input)
                .err()
                .ok_or("invalid accepted delivery succeeded")?;
            require(
                failure.to_string().contains(expected),
                format!("expected rejection {expected}, got {failure}"),
            )?;
            require(
                before == runtime.snapshot().map_err(error)?,
                "rejection partially changed state",
            )
        }
        one(&mut self.authority, input, expected)?;
        one(&mut self.replica, input, expected)?;
        if let Some(restored) = &mut self.restored {
            one(restored, input, expected)?;
        }
        Ok(())
    }
    fn reject_commands(&mut self, commands: Vec<AcceptedCommand>, expected: &str) -> Result<()> {
        let input = self.authority.next_tick(commands).map_err(error)?;
        self.reject(&input, expected)
    }
    fn probe(&mut self, name: &str, value: i64) {
        self.report.probes.push(Probe {
            name: name.into(),
            value,
        });
    }
    fn finish(mut self) -> Result<ReplayReport> {
        require(
            self.replayed_ticks > 0,
            "scenario never continued a restored snapshot",
        )?;
        require(
            self.duplicate_ticks > 0,
            "scenario never checked an exact accepted tick duplicate",
        )?;
        self.probe(
            "post_snapshot_replayed_ticks",
            i64::from(self.replayed_ticks),
        );
        self.probe("duplicate_tick_noops", i64::from(self.duplicate_ticks));
        Ok(self.report)
    }
}

pub fn run_scenario(scenario: u32, seed: u64) -> Result<ReplayReport> {
    match scenario {
        0 => behavior(seed),
        1 => avatar(seed),
        2 => effects(seed),
        3 => portal_route(seed),
        _ => Err(format!("unknown replay scenario {scenario}")),
    }
}

fn behavior_content() -> Result<ContentSet> {
    let mut routines = RoutineStore::new();
    routines
        .bind_semiglobal(BEHAVIOR_GUID, 900)
        .map_err(error)?;
    insert(
        &mut routines,
        key(BEHAVIOR_GUID, 4096),
        vec![
            add_attribute(0, 1, 1),
            random(0, 32767, 2),
            expression(
                variable(Scope::MyObjectAttributes, 1),
                5,
                variable(Scope::Temps, 0),
                3,
            ),
            call(4097, [3, 7, 0, 0], 4),
            add_attribute(3, 1, 254),
        ],
    )?;
    insert(
        &mut routines,
        key(BEHAVIOR_GUID, 4097),
        vec![
            call(8192, [3, 11, 0, 0], 1),
            VmInstruction::new(0, 2, 255, [0; 8]),
            expression(
                variable(Scope::MyObjectAttributes, 2),
                3,
                variable(Scope::Parameters, 1),
                254,
            ),
        ],
    )?;
    insert(
        &mut routines,
        RoutineKey {
            scope: RoutineScope::SemiGlobal(900),
            id: 8192,
        },
        vec![
            expression(
                variable(Scope::MyObjectAttributes, 4),
                3,
                variable(Scope::Parameters, 1),
                1,
            ),
            call(300, [0; 4], 2),
            VmInstruction::new(0, 3, 255, [0; 8]),
            random(1, 13, 4),
            expression(
                variable(Scope::MyObjectAttributes, 5),
                3,
                variable(Scope::Temps, 1),
                254,
            ),
        ],
    )?;
    insert(
        &mut routines,
        RoutineKey {
            scope: RoutineScope::Global,
            id: 300,
        },
        vec![
            random(2, -1, 1),
            expression(
                variable(Scope::MyObjectAttributes, 6),
                5,
                variable(Scope::Temps, 2),
                2,
            ),
            add_attribute(7, 1, 254),
        ],
    )?;
    let mut object = ObjectDefinition::new(BEHAVIOR_GUID, 8);
    object.entry_points.insert(1, key(BEHAVIOR_GUID, 4096));
    ContentSet::new(routines, vec![object], vec![], TuningSet::default())
}

fn behavior(seed: u64) -> Result<ReplayReport> {
    let mut driver = Driver::new(0, seed, VmMode::Ts1, behavior_content()?)?;
    let mut queries = 0;
    for tick in 1..=96 {
        driver.step(if tick == 1 {
            vec![
                spawn(BEHAVIOR_GUID, 3, false),
                spawn(BEHAVIOR_GUID, 7, false),
            ]
        } else {
            vec![]
        })?;
        if tick == 2 {
            let thread = &driver.authority.state().threads[&ObjectId(1)];
            require(
                thread.frames.len() == 3
                    && matches!(thread.stop, VmStop::Sleeping { until_tick: 4 }),
                "BHAV fixture did not save the intended three-frame sleep",
            )?;
            driver.probe("saved_stack_depth", 3);
            driver.checkpoint("nested-semiglobal-sleep")?;
        }
        if tick == 5 {
            driver.checkpoint("private-caller-sleep")?;
        }
        if tick % 11 == 0 {
            let before = driver.authority.snapshot().map_err(error)?;
            let entity = driver.entity(1)?;
            let result = driver
                .authority
                .query_behavior(
                    entity,
                    RoutineKey {
                        scope: RoutineScope::Global,
                        id: 300,
                    },
                    FrameContext::for_entity(entity, BEHAVIOR_GUID),
                    vec![0; 4],
                    32,
                )
                .map_err(error)?;
            require(
                result.stop == VmStop::Completed(PrimitiveExit::ReturnTrue),
                "query BHAV did not execute",
            )?;
            require(
                driver.authority.snapshot().map_err(error)? == before,
                "query consumed authoritative RNG or wrote state",
            )?;
            queries += 1;
        }
        if tick == 47 {
            driver.checkpoint("repeated-main-midstream")?;
        }
    }
    let attributes = driver.authority.state().entities[&ObjectId(1)]
        .attributes
        .clone();
    require(
        attributes[3] == 13 && attributes[2] == 13 * 7 && attributes[0] == 14,
        format!("BHAV call/sleep semantic counts differ: {attributes:?}"),
    )?;
    require(
        driver.authority.state().entities[&ObjectId(2)].attributes[3] == 13,
        "second scheduled object did not finish the same call count",
    )?;
    driver.probe("completed_main_calls", i64::from(attributes[3]));
    driver.probe("isolated_rng_queries", queries);
    driver.checkpoint("final")?;
    driver.finish()
}

fn avatar_content() -> Result<ContentSet> {
    let mut routines = RoutineStore::new();
    let mut motive = [7, 7, Motive::Hunger as u8, 0, 137, 0, 80, 0];
    motive[4..6].copy_from_slice(&137i16.to_le_bytes());
    insert(
        &mut routines,
        key(AVATAR_GUID, 4096),
        vec![
            add_attribute(0, 1, 1),
            expression(
                variable(Scope::Parameters, 0),
                5,
                variable(Scope::Literal, 3),
                2,
            ),
            VmInstruction::new(0, 3, 255, [0; 8]),
            VmInstruction::new(29, 4, 255, motive),
            VmInstruction::new(44, 7, 5, [1, 0, 0, 0, 1, 32 | 64, 5, 0]),
            expression(
                variable(Scope::MyObjectAttributes, 1),
                3,
                variable(Scope::Local, 0),
                6,
            ),
            add_attribute(2, 1, 4),
            add_attribute(3, 1, 8),
            VmInstruction::new(44, 11, 9, [2, 0, 0, 0, 1, 32 | 2 | 64, 3, 0]),
            expression(
                variable(Scope::MyObjectAttributes, 4),
                3,
                variable(Scope::Local, 0),
                10,
            ),
            add_attribute(5, 1, 8),
            add_attribute(6, 1, 254),
        ],
    )?;
    let mut object = ObjectDefinition::new(AVATAR_GUID, 8);
    object.entry_points.insert(1, key(AVATAR_GUID, 4096));
    let forward = AnimationMetadata {
        resource: "fixture-forward.anim".into(),
        num_frames: 9,
        time_properties: vec![
            TimeProperty::xevt(0, 7),
            TimeProperty::xevt(0, 101),
            TimeProperty::xevt(80, -3),
            TimeProperty::xevt(40, 2),
            TimeProperty {
                time_ms: 100,
                properties: BTreeMap::from([
                    ("righthand".into(), "5".into()),
                    ("dress".into(), "fixture-hat".into()),
                    ("sound".into(), "fixture-sound".into()),
                ]),
            },
            TimeProperty::xevt(600, 88),
        ],
    };
    let reverse = AnimationMetadata {
        resource: "fixture-reverse.anim".into(),
        num_frames: 6,
        time_properties: vec![
            TimeProperty::xevt(0, 5),
            TimeProperty::xevt(40, 6),
            TimeProperty::xevt(160, 7),
        ],
    };
    ContentSet::new(
        routines,
        vec![object],
        vec![
            (
                AnimationKey {
                    owner: AVATAR_GUID,
                    scope: 1,
                    id: 1,
                },
                forward,
            ),
            (
                AnimationKey {
                    owner: AVATAR_GUID,
                    scope: 1,
                    id: 2,
                },
                reverse,
            ),
        ],
        TuningSet::default(),
    )?
    .with_strings(vec![
        (
            (0, 128),
            vec![
                String::new(),
                "fixture-forward".into(),
                "fixture-reverse".into(),
            ],
        ),
        (
            (0, 151),
            vec![
                String::new(),
                "fixture-sit".into(),
                "fixture-kneel".into(),
                "fixture-stand".into(),
            ],
        ),
    ])
}

fn avatar(seed: u64) -> Result<ReplayReport> {
    let mut driver = Driver::new(1, seed, VmMode::Ts1, avatar_content()?)?;
    let mut actual_xevts = 0;
    let mut max_queued_events = 0;
    let mut normal_speed = None;
    let mut hurry_speed = None;
    let mut normal_frame = None;
    let mut hurry_frame = None;
    let mut forward_completions = [0; 2];
    let mut reverse_completions = [0; 2];
    for tick in 1..=181 {
        let commands = if tick == 1 {
            vec![spawn(AVATAR_GUID, 3, true), spawn(AVATAR_GUID, 7, true)]
        } else if tick == 2 {
            let first = driver.entity(1)?;
            let second = driver.entity(2)?;
            vec![
                write(first, EntityField::Motive, 7, -80),
                write(first, EntityField::Motive, 5, -30),
                write(second, EntityField::Motive, 7, -80),
                // VMStackObjectVariable.WalkStyle, deliberately different from PD35.
                write(second, EntityField::ObjectData, 17, 1),
            ]
        } else {
            vec![]
        };
        let outcome = driver.step(commands)?;
        for event in outcome.events {
            if let RuntimeEvent::Avatar { output, .. } = event {
                for cue in output.animation_cues {
                    if let AnimationCue::Xevt { code, .. } = cue {
                        require(code != 88, "future source-order event incorrectly fired after animation completion")?;
                        actual_xevts += 1;
                    }
                }
            }
        }
        for id in [1, 2] {
            let item = &driver.authority.state().entities[&ObjectId(id)];
            let avatar = item.avatar.as_ref().ok_or("missing avatar state")?;
            let index = (id - 1) as usize;
            if item.attributes[3] != forward_completions[index] {
                require(
                    item.attributes[2] == item.attributes[3] * 6
                        && item.attributes[1] == item.attributes[3] * 114,
                    "forward completion must drain exactly [7,101,-3,2,3,4]",
                )?;
                forward_completions[index] = item.attributes[3];
            }
            if item.attributes[6] != reverse_completions[index] {
                require(
                    item.attributes[5] == item.attributes[6] * 3
                        && item.attributes[4] == item.attributes[6] * 18,
                    "reverse completion must drain exactly [7,6,5]",
                )?;
                reverse_completions[index] = item.attributes[6];
            }
            if let Some(animation) = avatar.animations.animations.first() {
                max_queued_events = max_queued_events.max(animation.event_queue.len());
                if !animation.backwards {
                    if id == 1 && normal_speed.is_none() {
                        require(animation.event_queue.iter().copied().collect::<Vec<_>>() == [7, 101],
                            "normal first frame must preserve the source-order future-event barrier")?;
                        normal_speed = Some(animation.speed.to_bits());
                        normal_frame = Some(animation.current_frame.to_bits());
                    }
                    if id == 2 && hurry_speed.is_none() {
                        require(animation.event_queue.iter().copied().collect::<Vec<_>>() == [7, 101, -3, 2],
                            "hurry first frame must queue all four available records in source order")?;
                        hurry_speed = Some(animation.speed.to_bits());
                        hurry_frame = Some(animation.current_frame.to_bits());
                    }
                }
            }
        }
        if tick == 4 {
            driver.checkpoint("animation-start-and-queued-xevts")?;
        }
        if tick == 8 {
            driver.checkpoint("mixed-forward-reverse-continuations")?;
        }
        if tick == 61 {
            driver.checkpoint("first-two-minute-motive-boundary")?;
        }
        if tick == 121 {
            driver.checkpoint("fractional-motive-and-animation-continuations")?;
        }
    }
    require(
        normal_speed == Some(1.2f32.to_bits()) && normal_frame == Some(1.2f32.to_bits()),
        "normal animation did not begin with the source 1.2f32 frame",
    )?;
    require(
        hurry_speed == Some(2.4f32.to_bits()) && hurry_frame == Some(2.4f32.to_bits()),
        "hurryable animation did not read WalkStyle and begin with the source 2.4f32 frame",
    )?;
    require(
        max_queued_events >= 2,
        "animation fixture never queued multiple events in one tick",
    )?;
    let item = &driver.authority.state().entities[&ObjectId(1)];
    let attributes = item.attributes.clone();
    let avatar = item.avatar.as_ref().ok_or("missing avatar")?;
    let hunger = avatar.motives.get(Motive::Hunger);
    require(
        attributes[3] > 0 && attributes[6] > 0 && attributes[2] >= 6 && attributes[5] >= 3,
        "animation event branches or forward/reverse completion never executed",
    )?;
    // Each completed forward clip drains six codes [7,101,-3,2,3,4]; the
    // synthesized tail proves completion was driven by metadata and VM state.
    require(
        attributes[2] >= attributes[3] * 6 && attributes[1] >= attributes[3] * 114,
        "synthesized completion events were lost",
    )?;
    require(
        hunger > -80 && avatar.motives.changes[Motive::Hunger as usize].fractional != 0.0,
        "motive restoration did not retain fractional continuation",
    )?;
    require(
        driver.authority.state().clock.minutes == 6,
        "avatar replay did not cross natural decay clock boundaries",
    )?;
    driver.probe("normal_speed_bits", i64::from(normal_speed.unwrap()));
    driver.probe("hurry_speed_bits", i64::from(hurry_speed.unwrap()));
    driver.probe("normal_first_frame_bits", i64::from(normal_frame.unwrap()));
    driver.probe("hurry_first_frame_bits", i64::from(hurry_frame.unwrap()));
    driver.probe("forward_event_count", i64::from(attributes[2]));
    driver.probe("forward_completed", i64::from(attributes[3]));
    driver.probe("reverse_event_count", i64::from(attributes[5]));
    driver.probe("reverse_completed", i64::from(attributes[6]));
    driver.probe("actual_xevt_cues", actual_xevts);
    driver.probe("max_queued_events", max_queued_events as i64);
    driver.probe("hunger_final", i64::from(hunger));
    driver.checkpoint("final")?;
    driver.finish()
}

fn effect_content() -> Result<ContentSet> {
    let mut routines = RoutineStore::new();
    insert(
        &mut routines,
        key(EFFECT_GUID, 4096),
        vec![
            add_attribute(0, 1, 1),
            VmInstruction::new(25, 2, 5, [0, 0, 50, 0, 0, 0, 0, 9]),
            add_attribute(1, 1, 3),
            expression(
                variable(Scope::MyObjectAttributes, 2),
                5,
                variable(Scope::TempXl, 0),
                4,
            ),
            VmInstruction::new(0, 5, 255, [0; 8]),
            add_attribute(3, 1, 254),
        ],
    )?;
    // No main entry point: every request is initiated by the accepted StartBehavior.
    ContentSet::new(
        routines,
        vec![ObjectDefinition::new(EFFECT_GUID, 4)],
        vec![],
        TuningSet::default(),
    )
}
fn resolve(
    request: &EffectDispatch,
    tick: u64,
    delivery_epoch: u64,
    committed_epoch: u64,
) -> Result<EffectResolved> {
    Ok(EffectResolved {
        operation_id: request.request.operation_id,
        target: request.request.target,
        apply_tick: tick,
        delivery_epoch,
        committed_epoch,
        value: EffectValue::Bytes(
            encode_effect_resolution(&VmResolution {
                response: HostResponse::Complete(PrimitiveExit::GotoTrue),
                writes: vec![RegisterWrite {
                    target: RegisterTarget::TempXl,
                    index: 0,
                    value: 24000,
                }],
            })
            .map_err(error)?,
        ),
    })
}
fn exactly_one(outcome: &TickOutcome) -> Result<EffectDispatch> {
    require(
        outcome.effects.len() == 1,
        "behavior did not issue exactly one durable operation",
    )?;
    Ok(outcome.effects[0].clone())
}
fn verify_retry(driver: &mut Driver, original: &EffectDispatch, epoch: u64) -> Result<()> {
    let before = driver.authority.snapshot().map_err(error)?;
    for _ in 0..3 {
        let pending = driver.authority.pending_effects().map_err(error)?;
        require(
            pending.len() == 1
                && pending[0].request == original.request
                && pending[0].dispatch_epoch == epoch,
            "retry changed the operation ID, issued epoch, target or payload",
        )?;
        require(
            driver.replica.pending_effects().map_err(error)?.is_empty(),
            "replica exposed an external dispatch",
        )?;
        if let Some(restored) = &driver.restored {
            require(
                restored.pending_effects().map_err(error)? == pending,
                "restored pending retry differs",
            )?;
        }
    }
    require(
        driver.authority.snapshot().map_err(error)? == before,
        "reading pending retries changed simulation state",
    )
}
fn effects(seed: u64) -> Result<ReplayReport> {
    let mut driver = Driver::new(2, seed, VmMode::Tso, effect_content()?)?;
    driver.step(vec![
        spawn(EFFECT_GUID, 3, false),
        spawn(EFFECT_GUID, 7, false),
    ])?;
    let first = driver.entity(1)?;
    let old_second = driver.entity(2)?;
    let first_request = exactly_one(&driver.step(vec![start(first, EFFECT_GUID)])?)?;
    require(
        first_request.request.issued_tick == 2 && first_request.request.issued_epoch == 1,
        "first operation did not retain its issuing tick and epoch",
    )?;
    driver.checkpoint("pending-external-vm-continuation")?;
    driver.step(vec![])?;
    driver.adopt(2)?;
    verify_retry(&mut driver, &first_request, 2)?;
    driver.checkpoint("takeover-retries-original-operation")?;
    let mut wrong_epoch = driver.authority.next_tick(vec![]).map_err(error)?;
    wrong_epoch.epoch = 1;
    driver.reject(&wrong_epoch, "WrongEpoch")?;
    driver.reject_commands(
        vec![AcceptedCommand::EffectResolved(resolve(
            &first_request,
            4,
            1,
            1,
        )?)],
        "Epoch",
    )?;
    for _ in 4..=6 {
        driver.step(vec![])?;
    }
    let delivered = resolve(&first_request, 7, 2, 1)?;
    // Duplicate deliveries in the same accepted tick must resume the waiting
    // primitive only once, even before it runs during the entity dispatch.
    driver.step(vec![
        AcceptedCommand::EffectResolved(delivered.clone()),
        AcceptedCommand::EffectResolved(delivered),
    ])?;
    require(
        driver.authority.state().entities[&ObjectId(1)].attributes[1] == 1,
        "same-tick duplicate resumed effect twice",
    )?;
    require(
        driver
            .authority
            .pending_effects()
            .map_err(error)?
            .is_empty(),
        "resolved operation remained pending",
    )?;
    driver.step(vec![AcceptedCommand::EffectResolved(resolve(
        &first_request,
        8,
        2,
        1,
    )?)])?;
    driver.checkpoint("resolved-effect-followed-by-sleep")?;
    driver.step(vec![])?;
    driver.step(vec![])?;
    let cancelled = exactly_one(&driver.step(vec![start(old_second, EFFECT_GUID)])?)?;
    driver.step(vec![AcceptedCommand::Delete { entity: old_second }])?;
    driver.step(vec![spawn(EFFECT_GUID, 7, false)])?;
    let new_second = driver.entity(2)?;
    require(
        new_second.object_id == old_second.object_id
            && new_second.generation == old_second.generation + 1,
        "deleted local object ID was not reused with a fresh generation",
    )?;
    driver.reject_commands(
        vec![AcceptedCommand::EffectResolved(resolve(
            &cancelled, 14, 2, 2,
        )?)],
        "OperationCancelled",
    )?;
    let mut conflict = resolve(&first_request, 14, 2, 1)?;
    conflict.value = EffectValue::Bytes(
        encode_effect_resolution(&VmResolution::complete(PrimitiveExit::GotoFalse))
            .map_err(error)?,
    );
    driver.reject_commands(
        vec![AcceptedCommand::EffectResolved(conflict)],
        "ConflictingDuplicate",
    )?;
    driver.step(vec![])?;
    let third = exactly_one(&driver.step(vec![start(first, EFFECT_GUID)])?)?;
    require(
        third.request.operation_id.nonce() == 3 && cancelled.request.operation_id.nonce() == 2,
        "durable operation IDs did not advance across resolve/cancel/reuse",
    )?;
    driver.checkpoint("second-pending-operation")?;
    driver.adopt(3)?;
    verify_retry(&mut driver, &third, 3)?;
    driver.step(vec![])?;
    driver.reject_commands(
        vec![AcceptedCommand::EffectResolved(resolve(&third, 17, 3, 4)?)],
        "Epoch",
    )?;
    driver.step(vec![])?;
    driver.step(vec![])?;
    driver.step(vec![AcceptedCommand::EffectResolved(resolve(
        &third, 19, 3, 2,
    )?)])?;
    for _ in 20..=24 {
        driver.step(vec![])?;
    }
    let attributes = driver.authority.state().entities[&ObjectId(1)]
        .attributes
        .clone();
    require(
        attributes == [2, 2, 24000, 2],
        format!("effect continuation counts differ: {attributes:?}"),
    )?;
    require(
        driver.authority.state().continuations.is_empty()
            && driver
                .authority
                .pending_effects()
                .map_err(error)?
                .is_empty(),
        "effect scenario left an unexpected pending continuation",
    )?;
    require(
        driver.authority.state().entities[&ObjectId(2)].attributes == [0; 4],
        "late completion changed the replacement object",
    )?;
    driver.probe("applied_effects", 2);
    driver.probe("unique_operations", 3);
    driver.probe("cancelled_operations", 1);
    driver.probe("authority_takeovers", 2);
    driver.probe("rejected_fenced_deliveries", 4);
    driver.probe("duplicate_effect_deliveries", 2);
    driver.probe("replacement_generation", i64::from(new_second.generation));
    driver.probe("xl_response_value", i64::from(attributes[2]));
    driver.checkpoint("final")?;
    driver.finish()
}

fn portal_content() -> Result<ContentSet> {
    let mut routines = RoutineStore::new();
    insert(
        &mut routines,
        key(ROUTE_ACTOR_GUID, 4096),
        vec![
            add_attribute(0, 1, 1),
            // GoToRoutingSlot: literal SLOT7 on the stack object, no failure tree.
            VmInstruction::new(45, 2, 3, [7, 0, 1, 0, 1, 0, 0, 0]),
            add_attribute(1, 1, 254),
            add_attribute(2, 1, 255),
        ],
    )?;
    insert(
        &mut routines,
        key(ROUTE_PORTAL_GUID, 4300),
        vec![expression(
            variable(Scope::Literal, 1),
            2,
            variable(Scope::Literal, 1),
            254,
        )],
    )?;
    let definitions = [ROUTE_ACTOR_GUID, ROUTE_PORTAL_GUID, ROUTE_TARGET_GUID]
        .into_iter()
        .map(|guid| {
            let mut definition = ObjectDefinition::new(guid, 3);
            definition.object_data[4] = 1;
            definition.object_data[42] = 3;
            if guid == ROUTE_ACTOR_GUID {
                definition.slot_count = 3;
            } else {
                definition.object_data[8] = 4; // source zero-extent flag
            }
            if guid == ROUTE_PORTAL_GUID {
                definition
                    .entry_points
                    .insert(15, key(ROUTE_PORTAL_GUID, 4300));
            }
            definition
        })
        .collect();
    ContentSet::new(routines, definitions, vec![], TuningSet::default())
        .map_err(error)?
        .with_routing_slots(vec![(
            (ROUTE_TARGET_GUID, 7),
            RoutingSlot {
                search: SlotSearch {
                    min_proximity: 0,
                    max_proximity: 0,
                    optimal_proximity: 0,
                    directions: 0,
                    resolution: 16,
                    ..SlotSearch::default()
                },
                facing: -3,
                snap_to_direction: false,
                snap_target_slot: None,
            },
        )])
        .map_err(error)
}

fn active_route(driver: &Driver, id: u64) -> Result<&RouteContinuation> {
    let continuation = driver
        .authority
        .state()
        .continuations
        .get(&id)
        .ok_or("route continuation unexpectedly missing")?;
    match &continuation.kind {
        ContinuationKind::Route(route) => Ok(route),
        _ => Err("continuation changed from route to another kind".into()),
    }
}

fn callback_command(id: u64, callback: &RouteCallback, exit: LotPosition) -> AcceptedCommand {
    AcceptedCommand::RouteCallback {
        continuation_id: id,
        route_id: callback.route_id,
        token: callback.token,
        outcome: CallbackOutcome {
            success: true,
            position: Some(exit),
            blocker: None,
        },
    }
}

fn portal_route(seed: u64) -> Result<ReplayReport> {
    let content = portal_content()?;
    let mut lot = LotModel::new(8, 8, 2).map_err(error)?;
    for y in 0..8 {
        for x in 0..8 {
            lot.set_floor(TilePos::new(x, y, 2), 1).map_err(error)?;
        }
    }
    lot.set_object_support(TilePos::new(6, 2, 1), true)
        .map_err(error)?;
    let entry = TilePos::new(3, 2, 1).center();
    let exit = TilePos::new(3, 2, 2).center();
    let goal = TilePos::new(6, 2, 2).center();
    let mut initial = SimRuntime::new(
        content.clone(),
        lot,
        RuntimeConfig::new(VmMode::Ts1, LOT, 1, seed),
        RuntimeRole::Authority,
    )
    .map_err(error)?;
    let spawns = [
        (ROUTE_ACTOR_GUID, TilePos::new(1, 2, 1).center(), true),
        (ROUTE_PORTAL_GUID, entry, false),
        (ROUTE_TARGET_GUID, goal, false),
    ]
    .into_iter()
    .map(|(guid, position, avatar)| {
        AcceptedCommand::Spawn(SpawnSpec {
            guid,
            position,
            facing: Facing::NORTH,
            persistent_id: PersistentId(if avatar { 1001 } else { 0 }),
            avatar,
        })
    })
    .collect();
    let initialization = initial.next_tick(spawns).map_err(error)?;
    let initialized = initial.step(&initialization).map_err(error)?;
    require(
        !initialized
            .events
            .iter()
            .any(|event| matches!(event, RuntimeEvent::ThreadFault { .. })),
        "portal fixture initialization faulted",
    )?;
    let mut state = initial.state().clone();
    let actor = state.entities[&ObjectId(1)].info.reference;
    let portal = state.entities[&ObjectId(2)].info.reference;
    let target = state.entities[&ObjectId(3)].info.reference;
    state
        .world
        .lot
        .upsert_portal(Portal {
            id: PortalId(1),
            entity: portal,
            entry,
            exit,
            bidirectional: true,
            enabled: true,
            cost: 16,
            revision: 0,
        })
        .map_err(error)?;
    // Same trusted initial-state seam as runtime_routes_source. This topology
    // installation is fixture normalization, not an accepted build command.
    let initial = SimRuntime::from_state(state, content, RuntimeRole::Authority).map_err(error)?;
    let mut driver = Driver::from_initialized(3, seed, initial)?;
    driver.checkpoint("normalized-initial-world")?;
    let mut context = FrameContext::for_entity(actor, ROUTE_ACTOR_GUID);
    context.stack_object = target.object_id;
    context.stack_object_ref = Some(target);
    driver.step(vec![AcceptedCommand::StartBehavior {
        entity: actor,
        routine: key(ROUTE_ACTOR_GUID, 4096),
        context,
        args: vec![0; 4],
        replace: true,
    }])?;
    let continuation = driver
        .authority
        .state()
        .continuations
        .values()
        .next()
        .ok_or("VM routing primitive did not create a continuation")?;
    let id = continuation.id;
    require(
        continuation.entity == actor && continuation.resumes_vm,
        "route is not bound to the waiting VM",
    )?;
    let thread = &driver.authority.state().threads[&actor.object_id];
    require(
        thread.frames.len() == 1 && thread.stop == VmStop::Waiting { request_id: id },
        "VM did not suspend its route instruction",
    )?;
    require(
        active_route(&driver, id)?.request().goals.len() == 1
            && active_route(&driver, id)?.request().goals[0].position == goal,
        "literal SLOT did not select the upper-floor goal",
    )?;
    driver.checkpoint("vm-route-pending")?;

    let callback = loop {
        require(
            driver.authority.state().completed_tick < 32,
            "portal callback was not reached within the fixture bound",
        )?;
        let outcome = driver.step(vec![])?;
        let mut callback = None;
        for event in outcome.events {
            match event {
                RuntimeEvent::RouteScript {
                    continuation_id,
                    callback: value,
                } => {
                    require(
                        continuation_id == id,
                        "portal callback changed continuation",
                    )?;
                    require(callback.is_none(), "multiple portal callbacks were emitted")?;
                    callback = Some(value);
                }
                RuntimeEvent::RouteFinished { success, code, .. } => {
                    return Err(format!(
                        "route finished before portal callback: {success}, {code:?}"
                    ));
                }
                _ => {}
            }
        }
        if let Some(callback) = callback {
            break callback;
        }
    };
    require(
        callback.actor == actor
            && callback.target == portal
            && callback.entrypoint == 15
            && callback.route_id == id
            && callback.token == 1
            && matches!(callback.kind,
                RouteCallbackKind::Portal { traversal, entry: from, exit: to }
                if traversal.id == PortalId(1) && !traversal.reverse && from == entry && to == exit),
        "suspended callback lost its portal identity or endpoints",
    )?;
    driver.checkpoint("portal-script-pending")?;
    for _ in 0..3 {
        let outcome = driver.step(vec![])?;
        require(
            !outcome.events.iter().any(|event| {
                matches!(
                    event,
                    RuntimeEvent::RouteScript { .. } | RuntimeEvent::RouteFinished { .. }
                )
            }),
            "waiting portal re-emitted or completed its callback",
        )?;
        let route = active_route(&driver, id)?;
        require(
            route.pending_callback() == Some(&callback)
                && route.phase() == RoutePhase::AwaitingScript
                && driver
                    .authority
                    .state()
                    .world
                    .object(actor)
                    .map(|o| o.position)
                    == Some(entry)
                && driver.authority.state().threads[&actor.object_id].stop
                    == VmStop::Waiting { request_id: id },
            "suspended portal state advanced before an accepted callback",
        )?;
    }
    driver.checkpoint("portal-script-held")?;
    let valid = callback_command(id, &callback, exit);
    let mut wrong_token = callback.clone();
    wrong_token.token += 1;
    driver.reject_commands(
        vec![callback_command(id, &wrong_token, exit)],
        "CallbackMismatch",
    )?;
    let mut wrong_route = callback.clone();
    wrong_route.route_id += 1;
    driver.reject_commands(
        vec![callback_command(id, &wrong_route, exit)],
        "CallbackMismatch",
    )?;
    driver.reject_commands(
        vec![valid.clone(), valid.clone()],
        "CallbackAlreadyCompleted",
    )?;
    driver.step(vec![valid.clone()])?;
    require(
        driver
            .authority
            .state()
            .world
            .object(actor)
            .map(|o| o.position)
            == Some(exit)
            && active_route(&driver, id)?.pending_callback().is_none(),
        "matching accepted callback did not cross to the recorded portal exit",
    )?;
    driver.checkpoint("portal-crossed")?;
    driver.reject_commands(vec![valid.clone()], "CallbackMismatch")?;
    let mut finished_count = 0;
    while driver.authority.state().completed_tick < 65 {
        let outcome = driver.step(vec![])?;
        for event in outcome.events {
            match event {
                RuntimeEvent::RouteFinished {
                    continuation_id,
                    entity,
                    success,
                    code,
                    blocker,
                } => {
                    require(
                        continuation_id == id
                            && entity == actor
                            && success
                            && code == RouteFailCode::Success
                            && blocker.is_none(),
                        "portal route did not finish successfully for its waiting VM",
                    )?;
                    finished_count += 1;
                }
                RuntimeEvent::RouteScript { .. } => {
                    return Err("portal route unexpectedly requested another script".into());
                }
                _ => {}
            }
        }
    }
    let attributes = driver.authority.state().entities[&actor.object_id]
        .attributes
        .clone();
    require(
        finished_count == 1
            && attributes == [1, 1, 0]
            && driver.authority.state().continuations.is_empty()
            && driver
                .authority
                .state()
                .world
                .object(actor)
                .map(|o| o.position)
                == Some(goal)
            && driver.authority.state().threads[&actor.object_id].stop
                == VmStop::Completed(PrimitiveExit::ReturnTrue),
        format!("portal route did not resume the success branch once: {attributes:?}"),
    )?;
    driver.reject_commands(vec![valid], "route continuation missing")?;
    driver.probe("normalized_initial_tick", 1);
    driver.probe("portal_callbacks", 1);
    driver.probe("portal_wait_ticks", 3);
    driver.probe("vm_suspended_frames", 1);
    driver.probe("route_finished_count", finished_count);
    driver.probe("vm_route_success_count", i64::from(attributes[1]));
    driver.probe("vm_route_failure_count", i64::from(attributes[2]));
    driver.probe("rejected_route_callbacks", 5);
    driver.probe("portal_final_level", i64::from(goal.level));
    driver.probe("portal_final_x", i64::from(goal.x));
    driver.probe("portal_final_y", i64::from(goal.y));
    driver.checkpoint("final")?;
    driver.finish()
}
