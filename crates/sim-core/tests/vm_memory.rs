#[path = "vm_host.rs"]
mod support;
use sim_core::ids::ObjectId;
use sim_core::vm::*;
use support::*;

#[test]
fn scopes_keep_caller_callee_stack_owner_and_indirections_distinct() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    host.add(3, 77, 0, 0);
    let context = &mut thread.top_mut().unwrap().context;
    context.callee = reference(2);
    context.stack_object = ObjectId(3);
    context.stack_object_ref = Some(reference(3));
    context.code_owner = 999;
    thread.temps[1] = 4;
    thread.top_mut().unwrap().args[2] = 5;
    write_variable(
        &mut thread,
        &mut host,
        Variable::new(Scope::MyObjectAttributes, 4),
        10,
    )
    .unwrap();
    write_variable(
        &mut thread,
        &mut host,
        Variable::new(Scope::StackObjectAttributeByTemp, 1),
        20,
    )
    .unwrap();
    write_variable(
        &mut thread,
        &mut host,
        Variable::new(Scope::StackObjectAttributeByParameter, 2),
        30,
    )
    .unwrap();
    assert_eq!(host.memory[&(ObjectId(1), EntityField::Attribute, 4)], 10);
    assert_eq!(host.memory[&(ObjectId(3), EntityField::Attribute, 4)], 20);
    assert_eq!(host.memory[&(ObjectId(3), EntityField::Attribute, 5)], 30);
    write_variable(
        &mut thread,
        &mut host,
        Variable::new(Scope::LocalByTemp, 1),
        72,
    )
    .unwrap();
    assert_eq!(
        read_variable(&thread, &host, Variable::new(Scope::Local, 4)).unwrap(),
        72
    );
    assert!(host.memory.keys().all(|(id, _, _)| *id != ObjectId(2)));
}

#[test]
fn short_xl_width_and_invalid_indirection_are_checked() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    write_big_variable(
        &mut thread,
        &mut host,
        Variable::new(Scope::Temps, 0),
        65535,
    )
    .unwrap();
    assert_eq!(thread.temps[0], -1);
    write_big_variable(
        &mut thread,
        &mut host,
        Variable::new(Scope::TempXl, 0),
        65535,
    )
    .unwrap();
    assert_eq!(thread.temp_xl[0], 65535);
    assert!(read_variable(&thread, &host, Variable::new(Scope::TempXl, 0)).is_err());
    assert!(read_variable(&thread, &host, Variable::new(Scope::TempByTemp, 0)).is_err());
    assert!(write_variable(&mut thread, &mut host, Variable::new(Scope::Temps, 20), 0).is_err());
    assert!(read_variable(
        &thread,
        &host,
        Variable {
            scope: 400,
            data: 0
        }
    )
    .is_err());
    assert!(matches!(
        read_variable(&thread, &host, Variable::new(Scope::TargetObject, 0)),
        Err(VmFault::DeprecatedScope(5))
    ));
    assert!(!write_variable(&mut thread, &mut host, Variable::new(Scope::Tuning, 0), 99).unwrap());
}

#[test]
fn separate_check_stack_temps_use_entity_bank_while_normal_temps_keep_the_clone() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    thread.temps[3] = 42;
    host.memory.insert((ObjectId(1), EntityField::Temp, 3), 7);
    host.entity_temps_alias = false;
    assert_eq!(
        read_variable(&thread, &host, Variable::new(Scope::StackObjectTemp, 3)).unwrap(),
        7
    );
    assert!(write_variable(
        &mut thread,
        &mut host,
        Variable::new(Scope::StackObjectTemp, 3),
        99
    )
    .unwrap());
    assert_eq!(host.memory[&(ObjectId(1), EntityField::Temp, 3)], 99);
    assert_eq!(
        read_variable(&thread, &host, Variable::new(Scope::Temps, 3)).unwrap(),
        42
    );
    host.entity_temps_alias = true;
    assert_eq!(
        read_variable(&thread, &host, Variable::new(Scope::StackObjectTemp, 3)).unwrap(),
        42
    );
    assert!(write_variable(
        &mut thread,
        &mut host,
        Variable::new(Scope::StackObjectTemp, 3),
        -8
    )
    .unwrap());
    assert_eq!(thread.temps[3], -8);
    assert_eq!(host.memory[&(ObjectId(1), EntityField::Temp, 3)], 99);
}

#[test]
fn cached_stack_generation_does_not_rebind_until_stack_id_is_assigned() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    write_variable(
        &mut thread,
        &mut host,
        Variable::new(Scope::StackObjectId, 0),
        2,
    )
    .unwrap();
    host.entities
        .get_mut(&ObjectId(2))
        .unwrap()
        .reference
        .generation = 2;
    assert_eq!(
        read_variable(&thread, &host, Variable::new(Scope::StackObjectId, 0)).unwrap(),
        2
    );
    assert!(matches!(
        read_variable(
            &thread,
            &host,
            Variable::new(Scope::StackObjectAttributes, 0)
        ),
        Err(VmFault::StaleEntity(_))
    ));
    write_variable(
        &mut thread,
        &mut host,
        Variable::new(Scope::StackObjectId, 0),
        2,
    )
    .unwrap();
    assert_eq!(
        read_variable(
            &thread,
            &host,
            Variable::new(Scope::StackObjectAttributes, 0)
        )
        .unwrap(),
        0
    );
    write_variable(
        &mut thread,
        &mut host,
        Variable::new(Scope::StackObjectId, 0),
        -8,
    )
    .unwrap();
    assert_eq!(
        read_variable(&thread, &host, Variable::new(Scope::StackObjectId, 0)).unwrap(),
        -8
    );
    assert_eq!(thread.top().unwrap().context.stack_object_ref, None);
}

#[test]
fn list_reads_and_tuning_table_scope_and_pid_halves_match_source() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    host.lists.insert(ObjectId(1), vec![1, 2, 3]);
    thread.temps[0] = 1;
    for (index, expected) in [(0, 1), (1, 3), (2, 3), (3, 2)] {
        assert_eq!(
            read_variable(&thread, &host, Variable::new(Scope::MyList, index)).unwrap(),
            expected
        );
    }
    assert_eq!(
        read_variable(&thread, &host, Variable::new(Scope::Tuning, 1)).unwrap(),
        4097
    );
    assert_eq!(
        read_variable(&thread, &host, Variable::new(Scope::Tuning, 64 * 128 + 2)).unwrap(),
        8194
    );
    assert_eq!(
        read_variable(&thread, &host, Variable::new(Scope::Tuning, 128 * 128 + 3)).unwrap(),
        259
    );
    assert_eq!(
        read_variable(&thread, &host, Variable::new(Scope::MyAvatarId, 0)).unwrap(),
        1
    );
    assert_eq!(
        read_variable(&thread, &host, Variable::new(Scope::MyAvatarId, 1)).unwrap(),
        0x1234
    );
    assert_eq!(
        read_variable(&thread, &host, Variable::new(Scope::MyTypeAttr, 0)).unwrap(),
        0
    );
    assert!(write_variable(
        &mut thread,
        &mut host,
        Variable::new(Scope::MyTypeAttr, 0),
        3
    )
    .unwrap());
    assert_eq!(
        read_variable(&thread, &host, Variable::new(Scope::FeatureEnableLevel, 0)).unwrap(),
        1
    );
}
