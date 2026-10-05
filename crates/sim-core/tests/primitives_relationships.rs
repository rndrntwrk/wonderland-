#[path = "vm_host.rs"]
mod support;
use sim_core::ids::ObjectId;
use sim_core::primitives::relationships::*;
use sim_core::vm::*;
use support::*;

fn operand(column: u8, mode: u8, flags: u8, local: u8, scope: Scope, data: i16) -> [u8; 8] {
    let mut bytes = [column, mode, flags, local, 0, 0, 0, 0];
    bytes[4..6].copy_from_slice(&(scope as u16).to_le_bytes());
    bytes[6..8].copy_from_slice(&data.to_le_bytes());
    bytes
}
fn set_stack(thread: &mut VmThread, host: &mut Host, id: i16) {
    write_variable(thread, host, Variable::new(Scope::StackObjectId, 0), id).unwrap();
}
fn persistent_key(owner: i16, target: i16) -> RelationshipKey {
    RelationshipKey {
        owner: RelationshipOwner::Entity(reference(owner)),
        target: RelationshipTarget::Persistent(target as u32 + 0x12340000),
    }
}

#[test]
fn old_and_new_operands_have_distinct_offsets_and_target_registers() {
    let old = RelationshipOperand::decode(24, [2, 9, 3, 2, 64, 200, 255, 255]);
    assert_eq!(old.variable, Variable::new(Scope::Parameters, 3));
    assert_eq!(
        (old.relationship_variable, old.mode, old.set_mode, old.local),
        (9, 2, 2, 0)
    );
    let new = RelationshipOperand::decode(26, operand(9, 2, 32 | 64, 5, Scope::Temps, 7));
    assert_eq!(new.variable, Variable::new(Scope::Temps, 7));
    assert_eq!(
        (new.relationship_variable, new.mode, new.set_mode, new.local),
        (9, 2, 2, 5)
    );
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    host.add(3, 3, 0, 0);
    set_stack(&mut thread, &mut host, 2);
    thread.top_mut().unwrap().args[0] = 1;
    thread.top_mut().unwrap().args[3] = 40;
    thread.top_mut().unwrap().locals[5] = 3;
    thread.temps[7] = 60;
    relationship(&mut thread, &mut host, 24, [1, 0, 3, 2, 0, 0, 0, 0]).unwrap();
    relationship(
        &mut thread,
        &mut host,
        26,
        operand(0, 2, 4, 5, Scope::Temps, 7),
    )
    .unwrap();
    assert_eq!(
        host.relationships.read(persistent_key(2, 1)),
        Some(vec![40])
    );
    assert_eq!(
        host.relationships.read(persistent_key(2, 3)),
        Some(vec![60])
    );
}

#[test]
fn persistent_matrix_creation_clamps_and_marks_before_column_failure() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    set_stack(&mut thread, &mut host, 2);
    thread.temps[0] = 500;
    assert_eq!(
        relationship(
            &mut thread,
            &mut host,
            26,
            operand(2, 0, 4, 0, Scope::Temps, 0)
        )
        .unwrap(),
        PrimitiveExit::GotoTrue
    );
    let key = persistent_key(1, 2);
    assert_eq!(host.relationships.read(key), Some(vec![0, 0, 100]));
    assert!(host
        .relationships
        .changed_persistent
        .contains(&(reference(1), 0x12340002)));
    host.relationships.changed_persistent.clear();
    assert_eq!(
        relationship(
            &mut thread,
            &mut host,
            26,
            operand(7, 0, 4 | 1, 0, Scope::Temps, 0)
        )
        .unwrap(),
        PrimitiveExit::GotoFalse
    );
    assert!(host
        .relationships
        .changed_persistent
        .contains(&(reference(1), 0x12340002)));
    assert_eq!(host.relationships.read(key).unwrap().len(), 3);
    host.relationships.matrices.clear();
    host.relationships.changed_persistent.clear();
    assert_eq!(
        relationship(
            &mut thread,
            &mut host,
            26,
            operand(7, 0, 4 | 1, 0, Scope::Temps, 0)
        )
        .unwrap(),
        PrimitiveExit::GotoFalse
    );
    assert!(
        host.relationships.matrices.is_empty() && host.relationships.changed_persistent.is_empty()
    );
}

#[test]
fn increment_scales_in_f32_then_narrows_then_wraps_before_clamping() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    set_stack(&mut thread, &mut host, 2);
    host.relationships
        .write(persistent_key(1, 2), vec![32760])
        .unwrap();
    host.relationship_scale = 2.0;
    thread.temps[0] = 20;
    relationship(
        &mut thread,
        &mut host,
        26,
        operand(0, 0, 32 | 64, 0, Scope::Temps, 0),
    )
    .unwrap();
    assert_eq!(
        host.relationships.read(persistent_key(1, 2)),
        Some(vec![-32736])
    );
    host.relationships
        .write(persistent_key(1, 2), vec![32760])
        .unwrap();
    relationship(
        &mut thread,
        &mut host,
        26,
        operand(0, 0, 32, 0, Scope::Temps, 0),
    )
    .unwrap();
    assert_eq!(
        host.relationships.read(persistent_key(1, 2)),
        Some(vec![-100])
    );
    host.relationship_scale = 1.5;
    thread.temps[0] = -3;
    relationship(
        &mut thread,
        &mut host,
        26,
        operand(0, 0, 32 | 64, 0, Scope::Temps, 0),
    )
    .unwrap();
    assert_eq!(
        host.relationships.read(persistent_key(1, 2)),
        Some(vec![-104])
    );
}

#[test]
fn reads_create_local_reverse_bookkeeping_and_ignore_failed_variable_setters() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    set_stack(&mut thread, &mut host, 2);
    let key = RelationshipKey {
        owner: RelationshipOwner::Entity(reference(1)),
        target: RelationshipTarget::Local(reference(2)),
    };
    assert_eq!(
        relationship(
            &mut thread,
            &mut host,
            26,
            operand(3, 0, 128, 0, Scope::Literal, 5)
        )
        .unwrap(),
        PrimitiveExit::GotoTrue
    );
    assert_eq!(host.relationships.read(key), Some(vec![0; 4]));
    assert_eq!(
        host.relationships.local_reverse[&reference(2)]
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        vec![reference(1)]
    );
    assert!(host.relationships.changed_persistent.is_empty());
    let encoded = bincode::serialize(&host.relationships).unwrap();
    let decoded: RelationshipBook = bincode::deserialize(&encoded).unwrap();
    assert_eq!(decoded, host.relationships);
    decoded.validate().unwrap();
}

#[test]
fn null_targets_succeed_but_ts1_neighbor_mode_uses_raw_neighbor_ids() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    set_stack(&mut thread, &mut host, 999);
    let bytes = [0, 0, 0, 0, 255, 255, 255, 255];
    assert_eq!(
        relationship(&mut thread, &mut host, 26, bytes).unwrap(),
        PrimitiveExit::GotoTrue
    );
    assert!(host.relationships.matrices.is_empty());
    thread.mode = VmMode::Ts1;
    host.memory
        .insert((ObjectId(1), EntityField::PersonData, 31), 42);
    thread.temps[0] = 80;
    relationship(
        &mut thread,
        &mut host,
        26,
        operand(0, 0, 2 | 4, 0, Scope::Temps, 0),
    )
    .unwrap();
    let key = RelationshipKey {
        owner: RelationshipOwner::Neighbor(42),
        target: RelationshipTarget::Neighbor(999),
    };
    assert_eq!(host.relationships.read(key), Some(vec![80]));
    thread.top_mut().unwrap().context.caller = reference(2);
    assert!(matches!(
        relationship(
            &mut thread,
            &mut host,
            26,
            operand(0, 0, 2, 0, Scope::Temps, 0)
        ),
        Err(VmFault::InvalidOperand { .. })
    ));
}

#[test]
fn relationship_book_caps_are_atomic_and_historical_supersets_remain_valid() {
    let owner = reference(1);
    let mut book = RelationshipBook::default();
    for target in 1..=MAX_RELATIONSHIP_MATRICES as u32 {
        book.write(
            RelationshipKey {
                owner: RelationshipOwner::Entity(owner),
                target: RelationshipTarget::Persistent(target),
            },
            vec![],
        )
        .unwrap();
    }
    let extra = RelationshipKey {
        owner: RelationshipOwner::Entity(owner),
        target: RelationshipTarget::Persistent(MAX_RELATIONSHIP_MATRICES as u32 + 1),
    };
    assert!(book.write(extra, vec![]).is_err());
    assert_eq!(book.matrices.len(), MAX_RELATIONSHIP_MATRICES);
    book.changed_persistent = (1..=MAX_RELATIONSHIP_BOOKKEEPING as u32)
        .map(|target| (owner, target))
        .collect();
    assert!(book.mark(extra, true).is_err());
    assert_eq!(book.changed_persistent.len(), MAX_RELATIONSHIP_BOOKKEEPING);
    book.matrices.clear();
    book.validate().unwrap(); // Source GenericTSOCall clears matrices alone.
    book.local_reverse.insert(
        reference(2),
        (1..=MAX_RELATIONSHIP_BOOKKEEPING as u32)
            .map(|generation| sim_core::ids::EntityRef {
                object_id: ObjectId(1),
                generation,
            })
            .collect(),
    );
    let local = RelationshipKey {
        owner: RelationshipOwner::Entity(reference(3)),
        target: RelationshipTarget::Local(reference(2)),
    };
    assert!(book.mark(local, false).is_err());
    assert_eq!(
        book.local_reverse[&reference(2)].len(),
        MAX_RELATIONSHIP_BOOKKEEPING
    );
    book.validate().unwrap();
    book.local_reverse
        .get_mut(&reference(2))
        .unwrap()
        .insert(sim_core::ids::EntityRef {
            object_id: ObjectId(0),
            generation: 1,
        });
    assert!(book.validate().is_err());
}
