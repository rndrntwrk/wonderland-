//! Frozen accepted-command layout from a6c3b73f66620f4eefd92444ca9059de90ba11b3.
//! Test fixture only. Do not extend it when appending new runtime commands.
use serde::{Deserialize, Serialize};
use sim_core::{
    effects::EffectResolved,
    ids::EntityRef,
    runtime::{interaction_adapter, SpawnSpec},
    vm::{FrameContext, MemoryAddress, RoutineKey, Ts1InventoryItem},
    world::{self, RouteGoal},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
// Keep accepted command payloads inline to preserve the public replay API and layout.
#[allow(clippy::large_enum_variant)]
pub enum LegacyAcceptedCommand {
    Spawn(SpawnSpec),
    Delete {
        entity: EntityRef,
    },
    Interrupt {
        entity: EntityRef,
    },
    StartBehavior {
        entity: EntityRef,
        routine: RoutineKey,
        context: FrameContext,
        args: Vec<i16>,
        replace: bool,
    },
    WriteMemory {
        address: MemoryAddress,
        value: i16,
    },
    SetInteractionProjection {
        entity: EntityRef,
        queued_users: BTreeSet<EntityRef>,
        active_advertisements: Option<BTreeMap<(u8, u16), i16>>,
    },
    SetEveryFrame {
        entity: EntityRef,
        enabled: bool,
    },
    SetObjectStatus {
        entity: EntityRef,
        disabled_flags: u8,
        broken: bool,
    },
    BeginRoute {
        entity: EntityRef,
        target: Option<EntityRef>,
        goals: Vec<RouteGoal>,
    },
    RouteCallback {
        continuation_id: u64,
        route_id: u64,
        token: u64,
        outcome: world::routing::CallbackOutcome,
    },
    DefineSlot(world::SlotDefinition),
    ReserveSlot(world::ReservationRequest),
    OccupySlot(world::ReservationToken),
    ReleaseSlot(world::ReservationToken),
    EffectResolved(EffectResolved),
    Disconnect {
        entity: EntityRef,
        session_epoch: u64,
    },
    Reconnect {
        entity: EntityRef,
        session_epoch: u64,
    },
    Reset {
        entity: EntityRef,
    },
    SetTs1FamilyBudget(Option<i32>),
    SetTs1Inventory {
        neighbor: i16,
        items: Vec<Ts1InventoryItem>,
    },
    /// Trusted authority grant. Never construct from an unauthenticated UI payload.
    SetInteractionAuthority {
        actor: EntityRef,
        access: Option<interaction_adapter::InteractionAccess>,
    },
    QueueInteraction(sim_core::interactions::InteractionIntent),
    CancelInteraction(sim_core::interactions::CancelIntent),
    /// Prepared from authoritative source permissions, prices and content.
    BeginBuild {
        preview: world::build::BuildPreview,
        authority: world::build::BuildAuthority,
    },
    /// Server-only durable receipt lane. A transport write is not this receipt.
    CompleteBuild(world::build::ServerBuildConfirmation),
    /// Trusted synchronous source callback; never construct from public UI input.
    /// Appended to preserve every existing accepted-command discriminant.
    ExecuteNamedEntryPoint {
        entity: EntityRef,
        name: String,
        args: Vec<i16>,
    },
    /// Opt-in source EvaluateCheck behavior for expression-only named routines.
    /// Only global-register bounds exceptions are currently qualified.
    /// Other faults still reject the whole tick. This is not avatar admission.
    /// Appended so legacy accepted command tags and behavior remain unchanged.
    ExecuteSourceExpressionCallback {
        entity: EntityRef,
        name: String,
        args: Vec<i16>,
    },
}
