#[path = "vm_host.rs"]
mod support;
use sim_core::ids::ObjectId;
use sim_core::primitives::fire::*;
use sim_core::vm::*;
use support::*;
fn stack(thread: &mut VmThread, host: &mut Host, id: i16) {
    write_variable(thread, host, Variable::new(Scope::StackObjectId, 0), id).unwrap();
}
fn object_value(host: &mut Host, id: i16, index: u16, value: i16) {
    host.memory
        .insert((ObjectId(id), EntityField::ObjectData, index), value);
}

#[test]
fn fire_enabled_chance_pool_and_percent_updates_follow_source_order() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    stack(&mut thread, &mut host, 2);
    host.fire.enabled = false;
    assert_eq!(
        burn(&mut thread, &mut host, [1, 0, 0, 0, 0, 0, 0, 0]).unwrap(),
        PrimitiveExit::GotoFalse
    );
    assert!(host.random_bounds.is_empty());
    thread.mode = VmMode::Ts1;
    host.fire.percent = 0;
    assert_eq!(
        burn(&mut thread, &mut host, [1, 0, 0, 0, 0, 0, 0, 0]).unwrap(),
        PrimitiveExit::GotoFalse
    );
    assert_eq!(host.random_bounds, vec![10000]);
    assert!(host.fire_percent_writes.is_empty());
    host.fire.percent = 20000;
    host.pool = true;
    assert_eq!(
        burn(&mut thread, &mut host, [1, 0, 0, 0, 0, 0, 0, 0]).unwrap(),
        PrimitiveExit::GotoFalse
    );
    assert_eq!(host.random_bounds.len(), 2);
    assert!(host.fire_percent_writes.is_empty());
    host.pool = false;
    host.entities.get_mut(&ObjectId(2)).unwrap().position = VmPosition {
        x: i16::MIN,
        y: i16::MIN,
        level: 1,
    };
    assert_eq!(
        burn(&mut thread, &mut host, [2, 0, 0, 0, 0, 0, 0, 0]).unwrap(),
        PrimitiveExit::GotoFalse
    );
    assert_eq!(host.fire_percent_writes, vec![19500]);
    assert!(host.operations.is_empty());
}

#[test]
fn multipart_fire_spread_visits_tiles_once_and_centers_source_fire_objects() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    host.entities.get_mut(&ObjectId(1)).unwrap().position = VmPosition {
        x: 8,
        y: 8,
        level: 1,
    };
    host.entities.get_mut(&ObjectId(2)).unwrap().position = VmPosition {
        x: 24,
        y: 8,
        level: 1,
    };
    host.add(3, 3, 40, 8);
    host.add(4, 4, 47, 15);
    host.entities.get_mut(&ObjectId(2)).unwrap().group =
        vec![ObjectId(2), ObjectId(3), ObjectId(4)];
    for id in 2..=4 {
        object_value(&mut host, id, 40, 32);
    }
    stack(&mut thread, &mut host, 2);
    assert_eq!(
        burn(&mut thread, &mut host, [0; 8]).unwrap(),
        PrimitiveExit::GotoTrue
    );
    assert!(host.random_bounds.is_empty());
    let fires = host
        .operations
        .iter()
        .filter_map(|op| {
            if let EntityOperation::Create {
                guid,
                position,
                direction,
                ..
            } = op
            {
                Some((*guid, *position, *direction))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        fires,
        vec![
            (
                FIRE_GUID,
                VmPosition {
                    x: 24,
                    y: 8,
                    level: 1
                },
                0
            ),
            (
                FIRE_GUID,
                VmPosition {
                    x: 40,
                    y: 8,
                    level: 1
                },
                0
            )
        ]
    );
    for id in 2..=4 {
        assert_eq!(
            host.memory[&(ObjectId(id), EntityField::ObjectData, 8)] & 512,
            512
        );
    }
    assert_eq!(
        burn(&mut thread, &mut host, [0; 8]).unwrap(),
        PrimitiveExit::GotoFalse
    );
}

#[test]
fn burn_busy_flag_is_ignored_and_ghosts_fireproof_and_other_floors_are_not_marked() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    host.entities.get_mut(&ObjectId(2)).unwrap().position = VmPosition {
        x: 24,
        y: 8,
        level: 1,
    };
    host.add(3, 3, 24, 8);
    host.entities.get_mut(&ObjectId(3)).unwrap().is_avatar = true;
    host.memory
        .insert((ObjectId(3), EntityField::PersonData, 68), 1);
    host.add(4, 4, 24, 8);
    object_value(&mut host, 4, 8, 2048);
    host.add(5, 5, 40, 8);
    host.entities.get_mut(&ObjectId(5)).unwrap().position.level = 2;
    host.entities
        .get_mut(&ObjectId(4))
        .unwrap()
        .group
        .push(ObjectId(5));
    host.function_states.insert(
        reference(2),
        FunctionEntityState {
            in_use: true,
            ..FunctionEntityState::default()
        },
    );
    for id in 2..=5 {
        object_value(&mut host, id, 40, 32);
    }
    stack(&mut thread, &mut host, 2);
    assert_eq!(
        burn(&mut thread, &mut host, [0, 255, 0, 0, 0, 0, 0, 0]).unwrap(),
        PrimitiveExit::GotoTrue
    );
    for id in 2..=5 {
        assert_eq!(
            host.memory
                .get(&(ObjectId(id), EntityField::ObjectData, 8))
                .copied()
                .unwrap_or(0)
                & 512,
            0
        );
    }
    assert_eq!(
        host.operations
            .iter()
            .filter(|op| matches!(op, EntityOperation::Create { .. }))
            .count(),
        1
    );
}

#[test]
fn front_position_uses_canonical_cardinals_and_missing_fire_definition_still_returns_true() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    host.entities.get_mut(&ObjectId(2)).unwrap().position = VmPosition {
        x: 40,
        y: 40,
        level: 1,
    };
    host.entities.get_mut(&ObjectId(2)).unwrap().direction = 6;
    stack(&mut thread, &mut host, 2);
    host.failed_creations = 1;
    assert_eq!(
        burn(&mut thread, &mut host, [1, 0, 0, 0, 0, 0, 0, 0]).unwrap(),
        PrimitiveExit::GotoTrue
    );
    assert!(
        matches!(host.operations[0],EntityOperation::Create{guid,position:VmPosition{x:24,y:40,level:1},direction:0,..}if guid==FIRE_GUID)
    );
    assert_eq!(host.fire.percent, 19500);
}

#[test]
fn first_fire_creation_observes_existing_tile_list_but_not_detached_empty_fallback() {
    for (mode, should_burn) in [(1, false), (0, true)] {
        let (_, mut thread, mut host) = setup(vec![instruction(255)]);
        host.entities.get_mut(&ObjectId(2)).unwrap().position = VmPosition {
            x: 40,
            y: 40,
            level: 1,
        };
        stack(&mut thread, &mut host, 2);
        host.new_object_burnable = true;
        burn(&mut thread, &mut host, [mode, 0, 0, 0, 0, 0, 0, 0]).unwrap();
        let burning = host
            .memory
            .get(&(ObjectId(3), EntityField::ObjectData, 8))
            .copied()
            .unwrap_or(0)
            & 512
            != 0;
        assert_eq!(burning, should_burn);
    }
}

#[test]
fn malformed_burnable_tile_reports_bounds_after_source_burning_flag_mutation() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    host.entities.get_mut(&ObjectId(2)).unwrap().position = VmPosition {
        x: -8,
        y: 8,
        level: 1,
    };
    object_value(&mut host, 2, 40, 32);
    stack(&mut thread, &mut host, 2);
    assert!(
        matches!(burn(&mut thread,&mut host,[0;8]),Err(VmFault::Bounds{area,..})if area=="fire tile")
    );
    assert_eq!(
        host.memory[&(ObjectId(2), EntityField::ObjectData, 8)] & 512,
        512
    );
    host.fire.width = 0;
    assert!(matches!(
        burn(&mut thread, &mut host, [0; 8]),
        Err(VmFault::InvalidContent(_))
    ));
}
