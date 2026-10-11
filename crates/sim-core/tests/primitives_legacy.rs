#[path = "vm_host.rs"]
mod support;
use sim_core::ids::ObjectId;
use sim_core::primitives::legacy::*;
use sim_core::vm::*;
use support::*;
fn token(kind: i32, guid: u32, count: u16) -> Ts1InventoryItem {
    Ts1InventoryItem {
        token_type: kind,
        guid,
        count,
    }
}
fn op(mode: u8, kind: u8, flags: u8, flags2: u8, guid: u32) -> [u8; 8] {
    let mut bytes = [mode, kind, flags, flags2, 0, 0, 0, 0];
    bytes[4..].copy_from_slice(&guid.to_le_bytes());
    bytes
}

#[test]
fn ts1_budget_rejects_negative_old_budget_but_allows_negative_new_budget_in_check_trees() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    thread.mode = VmMode::Ts1;
    thread.is_check = true;
    host.family_budget = Some(100);
    assert_eq!(
        family_budget(&thread, &mut host, [0, 0, 200, 0, 0, 0, 0, 0]).unwrap(),
        PrimitiveExit::GotoTrue
    );
    assert_eq!(host.family_budget, Some(-100));
    assert_eq!(host.budget_writes, vec![-100]);
    assert_eq!(
        family_budget(&thread, &mut host, [0, 0, 10, 0, 2, 0, 0, 0]).unwrap(),
        PrimitiveExit::GotoFalse
    );
    assert_eq!(host.family_budget, Some(-100));
}

#[test]
fn ts1_budget_none_just_test_and_big_wrapping_amount_match_source() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    thread.temp_xl[0] = i32::MIN;
    assert_eq!(
        family_budget(&thread, &mut host, [3, 42, 0, 0, 0, 0, 0, 0]).unwrap(),
        PrimitiveExit::GotoTrue
    );
    assert_eq!(host.family_budget, None);
    assert!(host.budget_writes.is_empty());
    host.family_budget = Some(0);
    family_budget(&thread, &mut host, [3, 42, 0, 0, 1 | 2, 0, 0, 0]).unwrap();
    assert_eq!(host.family_budget, Some(0));
    family_budget(&thread, &mut host, [3, 42, 0, 0, 2, 0, 0, 0]).unwrap();
    assert_eq!(host.family_budget, Some(i32::MIN));
    assert!(matches!(
        family_budget(&thread, &mut host, [3, 255, 0, 0, 0, 0, 0, 0]),
        Err(VmFault::UnknownScope(255))
    ));
}

#[test]
fn inventory_add_wraps_unsigned_count_and_remove_by_guid_checks_quantity_and_wildcard_type() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    host.memory
        .insert((ObjectId(1), EntityField::PersonData, 31), 44);
    thread.temps[0] = -1;
    inventory(&mut thread, &mut host, op(0, 3, 2, 0, 99)).unwrap();
    assert_eq!(host.ts1_inventory.read(44), Some(vec![token(3, 99, 65535)]));
    thread.temps[0] = 3;
    inventory(&mut thread, &mut host, op(0, 3, 2, 0, 99)).unwrap();
    assert_eq!(host.ts1_inventory.read(44), Some(vec![token(3, 99, 2)]));
    assert_eq!(
        inventory(&mut thread, &mut host, op(1, 0, 2, 0, 99)).unwrap(),
        PrimitiveExit::GotoFalse
    );
    thread.temps[0] = -1;
    assert_eq!(
        inventory(&mut thread, &mut host, op(1, 0, 2, 0, 99)).unwrap(),
        PrimitiveExit::GotoTrue
    );
    assert_eq!(host.ts1_inventory.read(44), Some(vec![]));
}

#[test]
fn remove_at_index_does_not_check_quantity_and_updates_index_only_when_requested() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    host.ts1_inventory.write(0, vec![token(3, 99, 3)]).unwrap();
    thread.temps[0] = 10;
    thread.temps[1] = 0;
    assert_eq!(
        inventory(&mut thread, &mut host, op(2, 0, 2 | 128, 1, 99)).unwrap(),
        PrimitiveExit::GotoTrue
    );
    assert_eq!(host.ts1_inventory.read(0), Some(vec![token(3, 99, 65529)]));
    assert_eq!(thread.temps[1], -1);
    thread.temps[0] = -1;
    thread.temps[1] = 0;
    inventory(&mut thread, &mut host, op(2, 0, 2 | 128, 1, 99)).unwrap();
    assert_eq!(host.ts1_inventory.read(0), Some(vec![]));
    assert_eq!(thread.temps[1], -1);
}

#[test]
fn find_token_branches_on_signed_count_before_aliased_index_register_write() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    host.ts1_inventory
        .write(0, vec![token(1, 7, 1), token(3, 99, 32768)])
        .unwrap();
    assert_eq!(
        inventory(&mut thread, &mut host, op(3, 0, 16 | 8, 2, 99)).unwrap(),
        PrimitiveExit::GotoFalse
    );
    assert_eq!(thread.temps[2], 1);
    assert_eq!(
        inventory(&mut thread, &mut host, op(3, 3, 8, 0, 99)).unwrap(),
        PrimitiveExit::GotoFalse
    );
    assert_eq!(thread.temps[2], i16::MIN);
}

#[test]
fn set_to_next_exact_type_counts_all_matches_and_uses_original_start_despite_aliases() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    host.ts1_inventory
        .write(
            0,
            vec![token(3, 7, 30000), token(0, 8, 9), token(3, 9, 30000)],
        )
        .unwrap();
    thread.temps[1] = -1;
    assert_eq!(
        inventory(&mut thread, &mut host, op(4, 3, 128 | 8, 1, 1)).unwrap(),
        PrimitiveExit::GotoTrue
    );
    assert_eq!((thread.temps[1], thread.temps[2]), (0, -5536));
    inventory(&mut thread, &mut host, op(4, 3, 128 | 8, 1, 1)).unwrap();
    assert_eq!(thread.temps[1], 2);
    assert_eq!(
        inventory(&mut thread, &mut host, op(4, 3, 128 | 8, 1, 1)).unwrap(),
        PrimitiveExit::GotoFalse
    );
    assert_eq!(thread.temps[2], -5536);
    thread.temps[0] = -1;
    assert_eq!(
        inventory(&mut thread, &mut host, op(4, 0, 128, 0, 1)).unwrap(),
        PrimitiveExit::GotoTrue
    );
    assert_eq!(thread.temps[0], 1); // Type0 is exact for this operation.
}

#[test]
fn legacy_follow_tokens_preserve_source_lookup_append_guid_mismatch() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    thread.temps[0] = 44;
    inventory(&mut thread, &mut host, op(5, 0, 0, 0, 1)).unwrap();
    thread.temps[0] = 45;
    inventory(&mut thread, &mut host, op(5, 0, 0, 0, 1)).unwrap();
    assert_eq!(
        host.ts1_inventory.read(0),
        Some(vec![token(2, 10, 44), token(2, 10, 45)])
    );
    host.ts1_inventory.write(0, vec![token(2, 0, 99)]).unwrap();
    inventory(&mut thread, &mut host, op(5, 0, 0, 0, 1)).unwrap();
    assert_eq!(host.ts1_inventory.read(0), Some(vec![token(2, 0, 1)]));
}

#[test]
fn ignored_inventory_mode_still_resolves_zero_guid_and_owner_before_switch() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    write_variable(
        &mut thread,
        &mut host,
        Variable::new(Scope::StackObjectId, 0),
        0,
    )
    .unwrap();
    assert!(matches!(
        inventory(&mut thread, &mut host, op(8, 0, 0, 0, 0)),
        Err(VmFault::MissingEntity(_))
    ));
    assert_eq!(
        inventory(&mut thread, &mut host, op(255, 0, 0, 0, 1)).unwrap(),
        PrimitiveExit::GotoTrue
    );
    assert!(host.ts1_inventory.inventories.is_empty());
    thread.temps[4] = 2;
    assert!(matches!(
        inventory(&mut thread, &mut host, op(0, 0, 0, 32, 1)),
        Err(VmFault::InvalidOperand { opcode: 51, .. })
    ));
}

#[test]
fn inventory_caps_and_checked_source_sum_are_independent_and_checkpointable() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    host.ts1_inventory
        .write(0, vec![token(3, 1, u16::MAX); MAX_TS1_INVENTORY_ITEMS])
        .unwrap();
    thread.temps[0] = 7;
    assert!(matches!(
        inventory(&mut thread, &mut host, op(4, 3, 0, 0, 1)),
        Err(VmFault::Arithmetic(_))
    ));
    assert_eq!(thread.temps[0], 7);
    let before = host.ts1_inventory.clone();
    assert!(host.ts1_inventory.write(1, vec![token(1, 1, 1)]).is_err());
    assert_eq!(host.ts1_inventory, before);
    let restored: Ts1InventoryBook =
        bincode::deserialize(&bincode::serialize(&host.ts1_inventory).unwrap()).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored, host.ts1_inventory);
}
