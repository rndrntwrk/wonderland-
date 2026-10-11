#[path = "vm_host.rs"]
mod support;
use sim_core::ids::ObjectId;
use sim_core::primitives::entities::*;
use sim_core::vm::*;
use support::*;

fn next(search: u8, scope: Scope, data: u8, guid: u32) -> [u8; 8] {
    let mut bytes = [0; 8];
    bytes[..4].copy_from_slice(&guid.to_le_bytes());
    bytes[4] = 128 | search;
    bytes[5] = scope as u8;
    bytes[7] = data;
    bytes
}
fn set_stack(thread: &mut VmThread, host: &mut Host, id: i16) {
    write_variable(thread, host, Variable::new(Scope::StackObjectId, 0), id).unwrap();
}

#[test]
fn iteration_sorts_host_ids_is_resumable_and_uses_explicit_target_variable() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    host.add(8, 8, 0, 0);
    host.add(5, 5, 0, 0);
    let op = next(0, Scope::Temps, 3, 0);
    thread.temps[3] = 0;
    for expected in [1, 2, 5, 8] {
        assert_eq!(
            set_to_next(&mut thread, &mut host, op).unwrap(),
            PrimitiveExit::GotoTrue
        );
        assert_eq!(thread.temps[3], expected);
    }
    assert_eq!(
        set_to_next(&mut thread, &mut host, op).unwrap(),
        PrimitiveExit::GotoFalse
    );
    assert_eq!(thread.temps[3], 8);
    assert_eq!(thread.top().unwrap().context.stack_object, ObjectId(1));
    thread.temps[3] = 2;
    host.entities.remove(&ObjectId(5));
    assert_eq!(
        set_to_next(&mut thread, &mut host, op).unwrap(),
        PrimitiveExit::GotoTrue
    );
    assert_eq!(thread.temps[3], 8);
    // The old encoding ignores target bytes until flag128 has been set.
    set_stack(&mut thread, &mut host, 0);
    let mut old = op;
    old[4] = 0;
    set_to_next(&mut thread, &mut host, old).unwrap();
    assert_eq!(thread.top().unwrap().context.stack_object, ObjectId(1));
}

#[test]
fn multipart_and_same_position_iteration_wrap_to_smallest_member() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    host.add(8, 8, 0, 0);
    host.add(5, 5, 0, 0);
    for id in [1, 5, 8] {
        let e = host.entities.get_mut(&ObjectId(id)).unwrap();
        e.multi_tile = true;
        e.group = vec![ObjectId(8), ObjectId(1), ObjectId(5)];
    }
    set_stack(&mut thread, &mut host, 8);
    assert_eq!(
        set_to_next(&mut thread, &mut host, next(3, Scope::StackObjectId, 0, 0)).unwrap(),
        PrimitiveExit::GotoTrue
    );
    assert_eq!(thread.top().unwrap().context.stack_object, ObjectId(1));
    set_stack(&mut thread, &mut host, 8);
    assert_eq!(
        set_to_next(&mut thread, &mut host, next(8, Scope::StackObjectId, 0, 0)).unwrap(),
        PrimitiveExit::GotoTrue
    );
    assert_eq!(thread.top().unwrap().context.stack_object, ObjectId(1));
    set_stack(&mut thread, &mut host, 0);
    assert_eq!(
        set_to_next(&mut thread, &mut host, next(8, Scope::StackObjectId, 0, 0)).unwrap(),
        PrimitiveExit::GotoFalse
    );
}

#[test]
fn same_position_and_no_duplicate_checks_use_tiles_instead_of_exact_subtile_coordinates() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    host.add(5, 5, 15, 15);
    set_stack(&mut thread, &mut host, 1);
    assert_eq!(
        set_to_next(&mut thread, &mut host, next(8, Scope::StackObjectId, 0, 0)).unwrap(),
        PrimitiveExit::GotoTrue
    );
    assert_eq!(thread.top().unwrap().context.stack_object, ObjectId(5));
    let mut operand = [0; 8];
    operand[..4].copy_from_slice(&5u32.to_le_bytes());
    operand[4] = 1;
    operand[5] = 1;
    assert_eq!(
        create(&mut thread, &mut host, operand).unwrap(),
        PrimitiveExit::GotoFalse
    );
    assert!(host.operations.is_empty());
}

#[test]
fn zero_argument_frames_fail_parameter_operands_with_bounds_instead_of_panicking() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    thread.top_mut().unwrap().args.clear();
    assert!(matches!(
        set_to_next(&mut thread, &mut host, next(6, Scope::Temps, 0, 0)),
        Err(VmFault::Bounds { .. })
    ));
    assert!(matches!(
        create(&mut thread, &mut host, [99, 0, 0, 0, 7, 0, 0, 0]),
        Err(VmFault::Bounds { .. })
    ));
    assert!(host.operations.is_empty());
}

#[test]
fn failed_creation_placement_deletes_the_entire_group() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    host.create_out_of_world = true;
    assert_eq!(
        create(&mut thread, &mut host, [99, 0, 0, 0, 1, 0, 0, 0]).unwrap(),
        PrimitiveExit::GotoFalse
    );
    assert!(matches!(
        host.operations.last(),
        Some(EntityOperation::Delete {
            cleanup_all: true,
            return_immediately: false,
            ..
        })
    ));
}

#[test]
fn drop_and_create_host_directions_are_canonical_notches_for_cardinal_and_diagonal_facing() {
    for (notch, (dx, dy)) in [
        (0, (0, -16)),
        (1, (16, -16)),
        (2, (16, 0)),
        (3, (16, 16)),
        (4, (0, 16)),
        (5, (-16, 16)),
        (6, (-16, 0)),
        (7, (-16, -16)),
    ] {
        let (_, mut thread, mut host) = setup(vec![instruction(255)]);
        host.entities.get_mut(&ObjectId(1)).unwrap().direction = notch;
        host.memory.insert((ObjectId(1), EntityField::Slot, 0), 2);
        assert_eq!(
            drop_object(&mut thread, &mut host).unwrap(),
            PrimitiveExit::GotoTrue
        );
        assert!(
            matches!(host.operations[0],EntityOperation::ChangePosition{position,direction,..} if position==VmPosition{x:8+dx,y:8+dy,level:1}&&direction==notch)
        );
    }
    for (notch, position) in [
        (0, (0, -16)),
        (2, (16, 0)),
        (4, (0, 16)),
        (6, (-16, 0)),
        (1, (0, 0)),
    ] {
        let (_, mut thread, mut host) = setup(vec![instruction(255)]);
        host.entities.get_mut(&ObjectId(1)).unwrap().direction = notch;
        let mut bytes = [0; 8];
        bytes[..4].copy_from_slice(&99u32.to_le_bytes());
        create(&mut thread, &mut host, bytes).unwrap();
        assert!(
            matches!(host.operations[0],EntityOperation::Create{position:actual,direction,..} if (actual.x,actual.y)==position&&direction==notch)
        );
    }
}

#[test]
fn object_type_matches_master_guid_and_missing_id_returns_error() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    host.entities.get_mut(&ObjectId(2)).unwrap().master_guid = Some(0x11223344);
    set_stack(&mut thread, &mut host, 2);
    let mut bytes = [0; 8];
    bytes[..4].copy_from_slice(&0x11223344u32.to_le_bytes());
    bytes[6] = 10;
    assert_eq!(
        test_object_type(&mut thread, &mut host, bytes).unwrap(),
        PrimitiveExit::GotoTrue
    );
    set_stack(&mut thread, &mut host, 99);
    assert_eq!(
        test_object_type(&mut thread, &mut host, bytes).unwrap(),
        PrimitiveExit::Error
    );
}

#[test]
fn adjacency_search_is_north_east_south_west_and_chooses_lowest_id_on_tile() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    host.entities.get_mut(&ObjectId(1)).unwrap().position = VmPosition {
        x: 40,
        y: 40,
        level: 1,
    };
    host.add(9, 9, 40, 24);
    host.add(6, 6, 40, 24);
    host.add(7, 7, 56, 40);
    host.add(8, 8, 24, 40);
    thread.top_mut().unwrap().locals[0] = 1;
    set_stack(&mut thread, &mut host, 0);
    for expected in [6, 7, 8] {
        assert_eq!(
            set_to_next(&mut thread, &mut host, next(9, Scope::StackObjectId, 0, 0)).unwrap(),
            PrimitiveExit::GotoTrue
        );
        assert_eq!(
            thread.top().unwrap().context.stack_object,
            ObjectId(expected)
        );
    }
    assert_eq!(
        set_to_next(&mut thread, &mut host, next(9, Scope::StackObjectId, 0, 0)).unwrap(),
        PrimitiveExit::GotoFalse
    );
}

#[test]
fn deletion_is_host_scheduled_preserves_stack_cache_and_self_delete_yields() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    set_stack(&mut thread, &mut host, 2);
    assert_eq!(
        remove(&mut thread, &mut host, [1, 0, 3, 0, 0, 0, 0, 0]).unwrap(),
        PrimitiveExit::GotoTrue
    );
    assert!(host.entities.contains_key(&ObjectId(2)));
    assert_eq!(
        thread.top().unwrap().context.stack_object_ref,
        Some(reference(2))
    );
    host.commit_deletions();
    assert!(stack_entity(&thread, &host).is_err());
    assert_eq!(
        remove(&mut thread, &mut host, [0; 8]).unwrap(),
        PrimitiveExit::GotoTrueNextTick
    );
    assert!(matches!(
        host.operations[0],
        EntityOperation::Delete {
            cleanup_all: true,
            return_immediately: true,
            ..
        }
    ));
}

#[test]
fn drop_onto_refuses_occupied_destination_and_drop_search_order_matches_source() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    host.add(3, 3, 0, 0);
    set_stack(&mut thread, &mut host, 2);
    host.memory.insert((ObjectId(1), EntityField::Slot, 0), 3);
    host.memory.insert((ObjectId(2), EntityField::Slot, 0), 1);
    assert_eq!(
        drop_onto(&mut thread, &mut host, [0; 8]).unwrap(),
        PrimitiveExit::GotoFalse
    );
    assert!(host.operations.is_empty());
    host.memory.insert((ObjectId(2), EntityField::Slot, 0), 0);
    assert_eq!(
        drop_onto(&mut thread, &mut host, [0; 8]).unwrap(),
        PrimitiveExit::GotoTrue
    );
    host.operations.clear();
    host.failed_positions = 3;
    assert_eq!(
        drop_object(&mut thread, &mut host).unwrap(),
        PrimitiveExit::GotoTrue
    );
    let positions = host
        .operations
        .iter()
        .filter_map(|op| {
            if let EntityOperation::ChangePosition { position, .. } = op {
                Some((position.x, position.y))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(positions, vec![(8, -8), (-8, -8), (24, -8), (-8, 8)]);
}

#[test]
fn create_rejects_authority_flags_before_mutation_and_passes_source_main_ids() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    set_stack(&mut thread, &mut host, 2);
    thread.temps[0] = 22;
    let mut op = [0; 8];
    op[..4].copy_from_slice(&99u32.to_le_bytes());
    op[4] = 6;
    op[5] = 128;
    assert!(matches!(
        create(&mut thread, &mut host, op),
        Err(VmFault::UnsupportedPrimitive { opcode: 42, .. })
    ));
    assert!(host.operations.is_empty());
    op[5] = 2 | 16;
    assert_eq!(
        create(&mut thread, &mut host, op).unwrap(),
        PrimitiveExit::GotoTrue
    );
    assert_eq!(thread.top().unwrap().context.stack_object, ObjectId(3));
    assert!(matches!(
        host.operations[0],
        EntityOperation::Create {
            guid: 99,
            main_parameter: ObjectId(2),
            main_stack_object: ObjectId(22),
            ..
        }
    ));
    op[5] = 1;
    assert_eq!(
        create(&mut thread, &mut host, op).unwrap(),
        PrimitiveExit::GotoFalse
    );
}

#[test]
fn distance_levels_direction_and_fixed_coordinates_use_source_units() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    set_stack(&mut thread, &mut host, 2);
    host.entities.get_mut(&ObjectId(2)).unwrap().position = VmPosition {
        x: 48,
        y: 64,
        level: 3,
    };
    distance(&mut thread, &mut host, [0; 8]).unwrap();
    assert_eq!(thread.temps[0], 45);
    thread.mode = VmMode::Ts1;
    distance(&mut thread, &mut host, [0; 8]).unwrap();
    assert_eq!(thread.temps[0], 40);
    host.entities.get_mut(&ObjectId(2)).unwrap().position = VmPosition {
        x: 16,
        y: 0,
        level: 1,
    };
    direction(&mut thread, &mut host, [0, 0, 8, 0, 0, 0, 0, 0]).unwrap();
    assert_eq!(thread.temps[0], 2);
    host.entities.get_mut(&ObjectId(2)).unwrap().position = VmPosition {
        x: -17,
        y: 31,
        level: 2,
    };
    terrain_info(&mut thread, &mut host, [2, 0, 0, 0, 0, 0, 0, 0]).unwrap();
    assert_eq!(&thread.temps[..5], &[-2, -1, 1, 15, 2]);
}
