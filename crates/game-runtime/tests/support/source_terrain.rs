//! Authored source-contract data for boundary tests, not original asset qualification.
#![allow(dead_code)]
use wonderland_game_runtime::{
    AcceptedCommand, EntityRef, Facing, GameRuntime, InteractionAccess, LotModel, PersistentId,
    PrincipalKey, RuntimeConfig, RuntimeEvent, RuntimeRole, SpawnSpec, TilePos, VmMode,
    sim_core::{
        avatars::{motives::TsoMotiveTuning, state::TEMPLATE_PERSON},
        interactions::{
            ActionFlags, InteractionDefinition, InteractionKey, InteractionScope, PermissionFlags,
            RoutineBinding,
        },
        runtime::goto_adapter::SOURCE_GOTO_GUID,
        state::{ContentSet, InteractionTable, ObjectDefinition, TuningSet},
        vm::{RoutineKey, RoutineScope, RoutineStore, VmInstruction, VmRoutine},
    },
};
pub fn extend(
    routines: &mut RoutineStore,
    objects: &mut Vec<ObjectDefinition>,
) -> (u32, InteractionTable) {
    let key = |id| RoutineKey {
        scope: RoutineScope::Private(SOURCE_GOTO_GUID),
        id,
    };
    for (id, instructions) in [
        // Source Init sets placement flags; Main completes; action reads Param0.
        (
            4096,
            vec![
                VmInstruction::new(2, 1, 255, [42, 0, 3, 0, 0, 5, 3, 7]),
                VmInstruction::new(2, 254, 255, [4, 0, 1, 0, 0, 5, 3, 7]),
            ],
        ),
        (4097, vec![VmInstruction::new(15, 254, 255, [0; 8])]),
        (
            4098,
            vec![VmInstruction::new(2, 254, 255, [0, 0, 0, 0, 0, 5, 8, 9])],
        ),
    ] {
        routines
            .insert(key(id), VmRoutine::new(id, 0, 4, instructions).unwrap())
            .unwrap();
    }
    let mut goto = ObjectDefinition::new(SOURCE_GOTO_GUID, 4);
    goto.object_data[8] = 4;
    goto.entry_points.extend([(0, key(4096)), (1, key(4097))]);
    objects.push(goto);
    (
        SOURCE_GOTO_GUID,
        InteractionTable {
            local_table_present: true,
            definitions: vec![InteractionDefinition {
                key: InteractionKey {
                    tta_index: 7,
                    scope: InteractionScope::Local,
                },
                action: RoutineBinding {
                    routine_id: 4098,
                    code_owner_guid: SOURCE_GOTO_GUID,
                },
                check: None,
                flags: ActionFlags(ActionFlags::ALLOW_VISITORS | ActionFlags::ALLOW_CONSECUTIVE),
                permissions: PermissionFlags::default(),
                label: Some("Authored GoTo boundary action".into()),
            }],
        },
    )
}
pub fn content() -> ContentSet {
    let mut routines = RoutineStore::new();
    let mut person = ObjectDefinition::new(TEMPLATE_PERSON, 4);
    person.object_data[4] = 1;
    person.object_data[42] = person.placement_rules.flags as i16;
    let mut objects = vec![person];
    let terrain = extend(&mut routines, &mut objects);
    ContentSet::new(
        routines,
        objects,
        vec![],
        TuningSet {
            tso_motives: Some(TsoMotiveTuning {
                flat_sim: [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0],
                category_weights: [[1000; 7]; 11],
            }),
            ..Default::default()
        },
    )
    .unwrap()
    .with_interaction_tables(vec![terrain])
    .unwrap()
}
pub fn runtime() -> (GameRuntime, EntityRef) {
    let mut game = GameRuntime::new(
        content(),
        LotModel::new(8, 8, 1).unwrap(),
        RuntimeConfig::new(VmMode::Tso, 11, 7, 123),
        RuntimeRole::Authority,
    )
    .unwrap();
    let outcome = game
        .advance(vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: TEMPLATE_PERSON,
            position: TilePos::new(3, 3, 1).center(),
            facing: Facing::NORTH,
            persistent_id: PersistentId(7),
            avatar: true,
        })])
        .unwrap();
    let actor = outcome
        .events
        .iter()
        .find_map(|e| match e {
            RuntimeEvent::Spawned(actor) => Some(*actor),
            _ => None,
        })
        .unwrap();
    game.advance(vec![AcceptedCommand::SetInteractionAuthority {
        actor,
        access: Some(InteractionAccess {
            principal: PrincipalKey(42),
            allow_hidden: false,
        }),
    }])
    .unwrap();
    (game, actor)
}
