#[path = "vm_host.rs"]
mod support;
use sim_core::ids::ObjectId;
use sim_core::primitives::{
    arithmetic::{expression, random_number, ExpressionOperand},
    primitive_info, registry, CoverageStatus,
};
use sim_core::vm::*;
use support::*;

fn expr(lhs: Variable, operator: u8, rhs: Variable) -> [u8; 8] {
    ExpressionOperand {
        lhs,
        rhs,
        operator,
        is_signed: 0,
    }
    .encode()
}
fn literal(value: i16) -> Variable {
    Variable::new(Scope::Literal, value)
}
fn temp(index: i16) -> Variable {
    Variable::new(Scope::Temps, index)
}

#[test]
fn expression_mutation_order_keeps_i32_lhs_and_reads_rhs_after_narrowing() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    thread.temps[0] = 32767;
    assert_eq!(
        expression(&mut thread, &mut host, expr(temp(0), 11, temp(0))).unwrap(),
        PrimitiveExit::GotoFalse
    );
    assert_eq!(thread.temps[0], -32768);
    thread.temps[0] = -32768;
    assert_eq!(
        expression(&mut thread, &mut host, expr(temp(0), 17, temp(0))).unwrap(),
        PrimitiveExit::GotoFalse
    );
    assert_eq!(thread.temps[0], 32767);
    thread.temp_xl[0] = i32::MAX;
    assert_eq!(
        expression(
            &mut thread,
            &mut host,
            expr(Variable::new(Scope::TempXl, 0), 11, literal(0))
        )
        .unwrap(),
        PrimitiveExit::GotoTrue
    );
    assert_eq!(thread.temp_xl[0], i32::MIN);
}

#[test]
fn expression_division_modulo_overflow_flags_and_setter_status_match_source() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    for (start, operator, rhs, expected) in [
        (-7, 12, 3, 2),
        (7, 12, -3, -2),
        (5, 12, 0, 5),
        (5, 7, 0, -1),
        (32767, 3, 1, -32768),
    ] {
        thread.temps[0] = start;
        assert_eq!(
            expression(
                &mut thread,
                &mut host,
                expr(temp(0), operator, literal(rhs))
            )
            .unwrap(),
            PrimitiveExit::GotoTrue
        );
        assert_eq!(thread.temps[0], expected);
    }
    thread.temp_xl[0] = i32::MIN;
    assert!(matches!(
        expression(
            &mut thread,
            &mut host,
            expr(Variable::new(Scope::TempXl, 0), 7, literal(-1))
        ),
        Err(VmFault::Arithmetic(_))
    ));
    thread.temp_xl[0] = 0;
    expression(
        &mut thread,
        &mut host,
        expr(Variable::new(Scope::TempXl, 0), 9, literal(32)),
    )
    .unwrap();
    assert_eq!(thread.temp_xl[0], i32::MIN);
    assert_eq!(
        expression(
            &mut thread,
            &mut host,
            expr(Variable::new(Scope::TempXl, 0), 8, literal(32))
        )
        .unwrap(),
        PrimitiveExit::GotoFalse
    );
    assert_eq!(
        expression(&mut thread, &mut host, expr(literal(5), 5, literal(2))).unwrap(),
        PrimitiveExit::GotoFalse
    );
    assert_eq!(
        expression(&mut thread, &mut host, expr(literal(5), 3, literal(2))).unwrap(),
        PrimitiveExit::GotoTrue
    );
    assert_eq!(
        expression(&mut thread, &mut host, expr(literal(5), 9, literal(2))).unwrap(),
        PrimitiveExit::GotoFalse
    );
}

#[test]
fn expression_all_comparators_and_ts1_boolean_aliases() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    for (op, expected) in [
        (0, false),
        (1, true),
        (2, false),
        (14, false),
        (15, true),
        (16, true),
    ] {
        assert_eq!(
            expression(&mut thread, &mut host, expr(literal(-2), op, literal(3))).unwrap(),
            PrimitiveExit::branch(expected)
        );
    }
    thread.mode = VmMode::Ts1;
    thread.temps[0] = 5;
    expression(&mut thread, &mut host, expr(temp(0), 18, literal(2))).unwrap();
    assert_eq!(thread.temps[0], 7);
    expression(&mut thread, &mut host, expr(temp(0), 19, literal(3))).unwrap();
    assert_eq!(thread.temps[0], 4);
    expression(&mut thread, &mut host, expr(temp(0), 20, literal(-1))).unwrap();
    assert_eq!(thread.temps[0], 0);
    assert!(expression(&mut thread, &mut host, expr(temp(0), 21, literal(0))).is_err());
}

#[test]
fn expression_floor_compatibility_compares_caller_against_callee_offset() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    thread.top_mut().unwrap().context.callee = reference(2);
    host.entities.get_mut(&ObjectId(1)).unwrap().position.level = 4;
    host.entities.get_mut(&ObjectId(2)).unwrap().position.level = 5;
    host.entities.get_mut(&ObjectId(2)).unwrap().level_offset = 2;
    assert_eq!(
        expression(
            &mut thread,
            &mut host,
            expr(Variable::new(Scope::MyObject, 29), 14, literal(1024))
        )
        .unwrap(),
        PrimitiveExit::GotoTrue
    );
    assert_eq!(
        expression(
            &mut thread,
            &mut host,
            expr(Variable::new(Scope::MyObject, 29), 1, literal(1024))
        )
        .unwrap(),
        PrimitiveExit::GotoFalse
    );
}

#[test]
fn list_push_pop_order_empty_false_and_mutation_despite_readonly_destination() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    expression(
        &mut thread,
        &mut host,
        expr(Variable::new(Scope::MyList, 1), 18, literal(10)),
    )
    .unwrap();
    expression(
        &mut thread,
        &mut host,
        expr(Variable::new(Scope::MyList, 0), 18, literal(20)),
    )
    .unwrap();
    assert_eq!(host.lists[&ObjectId(1)], vec![20, 10]);
    expression(
        &mut thread,
        &mut host,
        expr(temp(0), 19, Variable::new(Scope::MyList, 1)),
    )
    .unwrap();
    assert_eq!(thread.temps[0], 10);
    assert_eq!(
        expression(
            &mut thread,
            &mut host,
            expr(literal(9), 19, Variable::new(Scope::MyList, 0))
        )
        .unwrap(),
        PrimitiveExit::GotoTrue
    );
    assert!(host.lists[&ObjectId(1)].is_empty());
    assert_eq!(
        expression(
            &mut thread,
            &mut host,
            expr(temp(0), 19, Variable::new(Scope::MyList, 0))
        )
        .unwrap(),
        PrimitiveExit::GotoFalse
    );
    assert!(expression(
        &mut thread,
        &mut host,
        expr(Variable::new(Scope::MyList, 2), 18, literal(1))
    )
    .is_err());
}

#[test]
fn random_range_is_unsigned_short_and_rng_runs_for_zero_and_one() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    for range in [-1i16, 0, 1] {
        let mut bytes = [0; 8];
        bytes[2..4].copy_from_slice(&8u16.to_le_bytes());
        bytes[4..6].copy_from_slice(&range.to_le_bytes());
        bytes[6..8].copy_from_slice(&7u16.to_le_bytes());
        random_number(&mut thread, &mut host, bytes).unwrap();
    }
    assert_eq!(host.random_bounds, vec![65535, 0, 1]);
}

#[test]
fn fresh_mode_registries_exhaustively_match_source_inventory() {
    assert_eq!(registry(VmMode::Tso).len(), 50);
    assert_eq!(registry(VmMode::Ts1).len(), 53);
    assert_eq!(
        primitive_info(VmMode::Tso, 30).unwrap().status,
        CoverageStatus::SourceNoOp
    );
    assert_eq!(
        primitive_info(VmMode::Ts1, 30).unwrap().handler,
        "VMGosubFoundAction"
    );
    assert_eq!(
        primitive_info(VmMode::Tso, 49).unwrap().operand,
        "VMAnimateSimOperand"
    );
    assert_eq!(
        primitive_info(VmMode::Tso, 24).unwrap().operand,
        "VMOldRelationshipOperand"
    );
    for opcode in 0..=255 {
        let tso = primitive_info(VmMode::Tso, opcode);
        let ts1 = primitive_info(VmMode::Ts1, opcode);
        assert_eq!(
            tso.is_some(),
            [
                0, 1, 2, 4, 5, 6, 7, 8, 9, 11, 12, 13, 14, 15, 16, 17, 18, 20, 21, 22, 23, 24, 25,
                26, 27, 28, 29, 30, 31, 32, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48,
                49, 50, 62, 63, 65, 67
            ]
            .contains(&opcode)
        );
        assert_eq!(
            ts1.is_some(),
            tso.is_some() || [3, 19, 51].contains(&opcode)
        );
    }
}
