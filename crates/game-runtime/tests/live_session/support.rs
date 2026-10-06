//! Shared conformance fixtures, not production content or network providers.
//! Original BHAV bytes are embedded unchanged. Metadata and authority are test-only.
use sim_core::interactions::{
    ActionFlags, InteractionDefinition, InteractionKey, InteractionScope, PermissionFlags,
    PrincipalKey, QueryOptions, RoutineBinding,
};
use sim_core::state::{ContentSet, InteractionTable, ObjectDefinition, TuningSet};
use sim_core::vm::{RoutineKey, RoutineScope, RoutineStore, VmMode};
use wonderland_content_runtime_bridge::import_bhav;
use wonderland_game_runtime::live_session::{
    Checkpoint, InteractionSelection, LiveReplica, ReplayLimits, StreamIdentity, TickFrame,
};
use wonderland_game_runtime::{
    AcceptedCommand, EntityRef, Facing, GameRuntime, InteractionAccess, LotModel, PersistentId,
    RuntimeConfig, RuntimeEvent, RuntimeRole, SpawnSpec, TilePos,
};
use wonderland_legacy_formats::{Limits, iff, semantic::decode_strings};

pub fn identity() -> StreamIdentity {
    StreamIdentity {
        browser_epoch: 2,
        source_epoch: 3,
        lot_incarnation: 4,
    }
}

pub fn install(client: &mut LiveReplica, server: &GameRuntime, tail: &[TickFrame]) {
    let bytes = server.snapshot().unwrap();
    client
        .install_checkpoint(
            client.checkpoint_ticket().unwrap(),
            Checkpoint {
                completed_tick: server.sim().state().completed_tick,
                state_hash: server.sim().state_hash().unwrap(),
                bytes: &bytes,
            },
            tail,
        )
        .unwrap();
}

pub const OWNER: u32 = 0x0478_6aed;
pub const PRINCIPAL: PrincipalKey = PrincipalKey(42);

pub fn content(name: &str, id: u16, check: bool) -> ContentSet {
    let bytes: &[u8] = match name {
        "Casino_2-Tile_Bar_CC.iff" => include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../TSOClient/FSO.Content.TSO/Content/Objects/Casino_2-Tile_Bar_CC.iff"
        )),
        "fso_christmas_flag.iff" => include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../TSOClient/FSO.Content.TSO/Content/Objects/fso_christmas_flag.iff"
        )),
        "cursebook_set_permission.iff" => include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../TSOClient/FSO.Content.TSO/Content/Objects/cursebook_set_permission.iff"
        )),
        _ => panic!("unknown test-only source fixture"),
    };
    let file = iff::decode(bytes, &Limits::default()).unwrap();
    let chunk = file
        .chunks
        .iter()
        .find(|c| c.key.kind == *b"BHAV" && c.key.id == id)
        .unwrap();
    let mut routines = RoutineStore::new();
    routines
        .insert(
            RoutineKey {
                scope: RoutineScope::Private(OWNER),
                id,
            },
            import_bhav(chunk, &Limits::default()).unwrap(),
        )
        .unwrap();
    let mut object = ObjectDefinition::new(OWNER, 4);
    object.attributes = vec![10, 20, 30, 40];
    let mut content =
        ContentSet::new(routines, vec![object], vec![], TuningSet::default()).unwrap();
    if let Some(chunk) = file
        .chunks
        .iter()
        .find(|c| c.key.kind == *b"STR#" && c.key.id == 302)
    {
        let strings = decode_strings(&chunk.data, &Limits::default()).unwrap();
        let values = strings.sets[0].iter().map(|s| s.value.text()).collect();
        content = content.with_strings(vec![((OWNER, 302), values)]).unwrap();
    }
    content
        .with_interaction_tables(vec![(
            OWNER,
            InteractionTable {
                local_table_present: true,
                definitions: vec![InteractionDefinition {
                    key: InteractionKey {
                        tta_index: 7,
                        scope: InteractionScope::Local,
                    },
                    action: RoutineBinding {
                        routine_id: id,
                        code_owner_guid: OWNER,
                    },
                    check: check.then_some(RoutineBinding {
                        routine_id: id,
                        code_owner_guid: OWNER,
                    }),
                    flags: ActionFlags(ActionFlags::ALLOW_VISITORS),
                    permissions: PermissionFlags::default(),
                    label: Some("Original routine".into()),
                }],
            },
        )])
        .unwrap()
}

pub fn server_and_client(
    name: &str,
    id: u16,
    check: bool,
) -> (GameRuntime, LiveReplica, EntityRef) {
    pair_with_content(content(name, id, check), false)
}

pub fn pair_with_content(
    content: ContentSet,
    avatar: bool,
) -> (GameRuntime, LiveReplica, EntityRef) {
    let make_runtime = |role| {
        GameRuntime::new(
            content.clone(),
            LotModel::new(8, 8, 1).unwrap(),
            RuntimeConfig::new(VmMode::Ts1, 11, 7, 123),
            role,
        )
        .unwrap()
    };
    let mut server = make_runtime(RuntimeRole::Authority);
    let mut client = LiveReplica::new(
        make_runtime(RuntimeRole::Replica),
        identity(),
        ReplayLimits::default(),
    )
    .unwrap();
    let outcome = server
        .advance(vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: OWNER,
            position: TilePos::new(3, 3, 1).center(),
            facing: Facing::NORTH,
            persistent_id: PersistentId(u32::from(avatar) * 7),
            avatar,
        })])
        .unwrap();
    let actor = outcome
        .events
        .iter()
        .find_map(|event| match event {
            RuntimeEvent::Spawned(actor) => Some(*actor),
            _ => None,
        })
        .unwrap();
    server
        .advance(vec![AcceptedCommand::SetInteractionAuthority {
            actor,
            access: Some(InteractionAccess {
                principal: PRINCIPAL,
                allow_hidden: false,
            }),
        }])
        .unwrap();
    install(&mut client, &server, &[]);
    (server, client, actor)
}

pub fn source_selection(
    client: &LiveReplica,
    actor: EntityRef,
    param0: i16,
) -> InteractionSelection {
    let batch = client
        .offers(
            client.connection(),
            PRINCIPAL,
            actor,
            actor,
            QueryOptions::default(),
        )
        .unwrap();
    let offer = batch
        .offers
        .iter()
        .find(|offer| offer.param0 == param0)
        .unwrap();
    InteractionSelection {
        principal: PRINCIPAL,
        actor,
        target: actor,
        interaction: offer.interaction,
        param0: offer.param0,
        command_sequence: 1,
    }
}
