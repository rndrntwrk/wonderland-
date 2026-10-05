#[path = "vm_host.rs"]
mod support;
use sim_core::avatars::outfits::OutfitReference;
use sim_core::ids::ObjectId;
use sim_core::primitives::presentation::*;
use sim_core::vm::*;
use support::*;
fn set_stack(thread: &mut VmThread, host: &mut Host, id: i16) {
    write_variable(thread, host, Variable::new(Scope::StackObjectId, 0), id).unwrap();
}

#[test]
fn refresh_only_updates_game_object_graphics_but_light_and_room_need_a_target() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    refresh(&thread, &mut host, [0; 8]).unwrap();
    assert!(host.presentations.is_empty());
    set_stack(&mut thread, &mut host, 2);
    refresh(&thread, &mut host, [1, 0, 0, 0, 0, 0, 0, 0]).unwrap();
    assert_eq!(
        host.presentations,
        vec![PresentationRequest::Refresh {
            target: reference(2),
            kind: RefreshKind::Graphic
        }]
    );
    refresh(&thread, &mut host, [0, 0, 2, 0, 0, 0, 0, 0]).unwrap();
    assert_eq!(
        host.presentations[1],
        PresentationRequest::Refresh {
            target: reference(1),
            kind: RefreshKind::RoomScore
        }
    );
    set_stack(&mut thread, &mut host, 0);
    assert!(refresh(&thread, &mut host, [1, 0, 1, 0, 0, 0, 0, 0]).is_err());
    assert_eq!(
        refresh(&thread, &mut host, [255; 8]).unwrap(),
        PrimitiveExit::GotoTrue
    );
}

#[test]
fn show_string_skips_game_objects_and_resolves_one_based_string_and_history_flag() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    host.dialog_string = Some("Hello".into());
    set_stack(&mut thread, &mut host, 2);
    show_string(&thread, &mut host, [44, 1, 2, 0, 1, 0, 0, 0]).unwrap();
    assert!(host.string_lookups.borrow().is_empty());
    set_stack(&mut thread, &mut host, 1);
    show_string(&thread, &mut host, [44, 1, 2, 0, 1, 0, 0, 0]).unwrap();
    assert_eq!(
        host.presentations,
        vec![PresentationRequest::ShowString {
            target: reference(1),
            message: "Hello".into(),
            history: false
        }]
    );
    let lookup = &host.string_lookups.borrow()[0];
    assert_eq!(
        (lookup.source, lookup.table, lookup.index),
        (StringSource::CodeOwner, 300, 1)
    );
}

#[test]
fn missing_string_table_succeeds_with_null_target_but_resolved_message_requires_avatar() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    set_stack(&mut thread, &mut host, 0);
    assert_eq!(
        show_string(&thread, &mut host, [0; 8]).unwrap(),
        PrimitiveExit::GotoTrue
    );
    host.dialog_string = Some("message".into());
    assert!(matches!(
        show_string(&thread, &mut host, [0; 8]),
        Err(VmFault::MissingEntity(_))
    ));
}

#[test]
fn headline_operand_preserves_signed_index_wrap_and_algorithmic_local_selection() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    set_stack(&mut thread, &mut host, 2);
    thread.temps[0] = 100;
    balloon(&thread, &mut host, [1, 0, 100, 4, 30, 0, 2, 16]).unwrap();
    assert!(
        matches!(host.presentations[0],PresentationRequest::Balloon{target,index:-56,duration:30,clear:false,icon:None,..} if target==reference(2))
    );
    thread.top_mut().unwrap().locals[3] = 1;
    balloon(&thread, &mut host, [6, 0, 2, 7, 255, 255, 0, 16]).unwrap();
    assert!(
        matches!(host.presentations[1],PresentationRequest::Balloon{target,index:2,duration:-1,icon:Some(icon),..} if target==reference(1)&&icon==reference(1))
    );
    balloon(&thread, &mut host, [255, 255, 255, 7, 0, 0, 0, 0]).unwrap();
    assert!(matches!(
        host.presentations[2],
        PresentationRequest::Balloon {
            clear: true,
            preserve_money_on_zero_duration: true,
            icon: None,
            ..
        }
    ));
}

#[test]
fn action_strings_collect_raw_stack_parameter_and_survive_checkpoint_validation() {
    let (store, mut thread, mut host) = setup(vec![instruction(255)]);
    thread.is_check = true;
    thread.action_strings = Some(vec![]);
    host.dialog_string = Some("Inspect".into());
    set_stack(&mut thread, &mut host, -7);
    change_action_string(&mut thread, &mut host, [44, 1, 1, 0, 2, 0, 0, 0]).unwrap();
    assert_eq!(
        thread.action_strings,
        Some(vec![ActionString {
            name: "Inspect".into(),
            parameter0: -7
        }])
    );
    assert!(host.presentations.is_empty());
    assert_eq!(
        host.string_lookups.borrow()[0].source,
        StringSource::CalleeSemiGlobal
    );
    let restored: VmThread = bincode::deserialize(&bincode::serialize(&thread).unwrap()).unwrap();
    restored.validate(&store).unwrap();
    thread.action_strings = None;
    change_action_string(&mut thread, &mut host, [44, 1, 2, 0, 2, 0, 0, 0]).unwrap();
    assert_eq!(
        host.presentations,
        vec![PresentationRequest::ActionName {
            caller: reference(1),
            name: "Inspect".into()
        }]
    );
}

#[test]
fn suit_temp_lookup_uses_resolved_byte_but_body_and_decoration_use_original_selector() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    thread.temps[0] = 8;
    host.suit_result = Some(ResolvedSuit::Id(0x123456));
    change_suit(&mut thread, &mut host, [0, 1, 2, 0, 0, 0, 0, 0]).unwrap();
    assert_eq!(host.suit_lookups.borrow()[0].resolved_data, 8);
    assert_eq!(
        host.appearances[0],
        AppearanceOperation::Body {
            target: reference(1),
            outfit: OutfitReference::Id(0x123456),
            current_outfit: 0
        }
    );
    assert_eq!(host.memory[&(ObjectId(1), EntityField::PersonData, 8)], 0);
    thread.temps[8] = 0;
    change_suit(&mut thread, &mut host, [8, 1, 2, 0, 0, 0, 0, 0]).unwrap();
    assert_eq!(host.suit_lookups.borrow()[1].resolved_data, 0);
    assert_eq!(
        host.appearances[1],
        AppearanceOperation::Decoration {
            target: reference(1),
            slot: 8,
            outfit_id: 0x123456,
            remove: false
        }
    );
    assert_eq!(host.memory[&(ObjectId(1), EntityField::PersonData, 8)], 0);
}

#[test]
fn suit_default_update_skips_temp_selector_and_accessory_names_remain_exact() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    thread.temps[0] = -1;
    host.suit_result = Some(ResolvedSuit::Reference(OutfitReference::Name(
        "legacy body".into(),
    )));
    change_suit(&mut thread, &mut host, [255, 2, 2 | 4, 0, 0, 0, 0, 0]).unwrap();
    let lookup = host.suit_lookups.borrow()[0].clone();
    assert!(lookup.default_update);
    assert_eq!(lookup.update_index, -1);
    assert_eq!(
        host.appearances[0],
        AppearanceOperation::DefaultDaywear {
            target: reference(1),
            outfit: OutfitReference::Name("legacy body".into())
        }
    );
    assert!(!host
        .memory
        .contains_key(&(ObjectId(1), EntityField::PersonData, 8)));
    host.suit_result = Some(ResolvedSuit::Accessory("FormalSleeve.APR".into()));
    change_suit(&mut thread, &mut host, [1, 0, 1, 0, 0, 0, 0, 0]).unwrap();
    assert_eq!(
        host.appearances[1],
        AppearanceOperation::AccessoryName {
            target: reference(1),
            appearance: "FormalSleeve.APR".into(),
            remove: true
        }
    );
}

#[test]
fn suit_missing_resource_is_source_success_but_caller_cast_and_string_bounds_fail_explicitly() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    assert_eq!(
        change_suit(&mut thread, &mut host, [0; 8]).unwrap(),
        PrimitiveExit::GotoTrue
    );
    thread.top_mut().unwrap().context.caller = reference(2);
    assert!(matches!(
        change_suit(&mut thread, &mut host, [0; 8]),
        Err(VmFault::InvalidOperand { opcode: 6, .. })
    ));
    thread.top_mut().unwrap().context.caller = reference(1);
    host.dialog_string = Some("x".repeat(65537));
    assert!(change_action_string(&mut thread, &mut host, [0; 8]).is_err());
    assert!(host.presentations.is_empty());
}
