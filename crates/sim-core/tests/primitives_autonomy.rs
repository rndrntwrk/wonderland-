#[path = "vm_host.rs"]
mod support;
use sim_core::avatars::{
    advertisements::{InteractionCandidate, InteractionVariant, MotiveAdvertisement},
    autonomy::{AutonomyContext, AutonomyTuning, QueuedPriority, ScoreCurve},
    motives::{Motive, MotiveState},
    state::PersonData,
    AvatarPlatform,
};
use sim_core::ids::ObjectId;
use sim_core::primitives::behavior::{find_best_action, gosub_found_action};
use sim_core::vm::*;
use support::*;
fn context() -> AutonomyContext {
    let mut motives = MotiveState::default();
    motives.set(Motive::Hunger, -50);
    AutonomyContext {
        platform: AvatarPlatform::Tso,
        motives,
        person_data: PersonData::default(),
        x: 0,
        y: 0,
        level: 1,
        queued: vec![],
    }
}
fn offers() -> AutonomyOffers {
    let curve = ScoreCurve::new(vec![(-100, -100), (100, 100)]).unwrap();
    let mut candidate = InteractionCandidate::new(reference(2), 0, 300);
    candidate.advertisements.push(MotiveAdvertisement {
        motive: Motive::Hunger,
        minimum: 0,
        delta: 1000,
        personality_modifier: 0,
    });
    candidate.variants = vec![
        InteractionVariant {
            param0: 7,
            motive_ad_changes: None,
        },
        InteractionVariant {
            param0: 9,
            motive_ad_changes: None,
        },
    ];
    AutonomyOffers {
        candidates: vec![candidate],
        tuning: AutonomyTuning {
            adult_curves: std::array::from_fn(|_| curve.clone()),
            child_curves: std::array::from_fn(|_| curve.clone()),
        },
        registers: None,
    }
}

#[test]
fn queued_higher_priority_shortcut_uses_queue_order_before_offers_and_without_rng() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    host.add(3, 3, 0, 0);
    let mut ctx = context();
    ctx.queued = vec![
        QueuedPriority {
            priority: 1,
            target: reference(2),
        },
        QueuedPriority {
            priority: 99,
            target: reference(3),
        },
    ];
    host.autonomy = Some(ctx);
    assert_eq!(
        find_best_action(&mut thread, &mut host).unwrap(),
        PrimitiveExit::GotoTrue
    );
    assert_eq!(thread.top().unwrap().context.stack_object, ObjectId(2));
    assert!(host.offer_requests.is_empty());
    assert!(host.autonomy_enqueued.is_empty());
    assert!(host.random_bounds.is_empty());
}

#[test]
fn checked_offers_use_shared_source_scorer_then_enqueue_autonomous_args_before_stack_write() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    host.autonomy = Some(context());
    host.offers = Some(offers());
    assert_eq!(
        find_best_action(&mut thread, &mut host).unwrap(),
        PrimitiveExit::GotoTrue
    );
    assert_eq!(host.random_bounds, vec![10000]);
    assert_eq!(
        host.autonomy_enqueued,
        vec![AutonomyEnqueue {
            caller: reference(1),
            source: reference(2),
            interaction: 44,
            args: [7, 0, 0, 0],
            priority: 2,
            mode: QueueMode::Normal
        }]
    );
    assert_eq!(
        thread.top().unwrap().context.stack_object_ref,
        Some(reference(2))
    );
}

#[test]
fn missing_selected_action_returns_false_after_check_register_changes_but_keeps_stack() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    host.autonomy = Some(context());
    let mut input = offers();
    input.candidates[0].auto_first = true;
    input.registers = Some(AutonomyRegisters {
        temps: [9; 20],
        temp_xl: [100000, 2],
    });
    host.offers = Some(input);
    host.autonomy_enqueue_result = false;
    assert_eq!(
        find_best_action(&mut thread, &mut host).unwrap(),
        PrimitiveExit::GotoFalse
    );
    assert_eq!(thread.top().unwrap().context.stack_object, ObjectId(1));
    assert_eq!(thread.temps[0], 9);
    assert_eq!(thread.temp_xl[0], 100000);
    assert!(host.random_bounds.is_empty());
    host.offers.as_mut().unwrap().candidates.clear();
    assert_eq!(
        find_best_action(&mut thread, &mut host).unwrap(),
        PrimitiveExit::GotoFalse
    );
    assert!(host.random_bounds.is_empty());
}

#[test]
fn gosub_found_action_retargets_active_callee_and_only_attempts_push_from_main_tree() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    thread.mode = VmMode::Ts1;
    host.interaction_states.insert(
        reference(1),
        InteractionState {
            action_tree: false,
            callee: Some(reference(2)),
        },
    );
    let call = RoutineCall {
        routine: RoutineKey {
            scope: RoutineScope::Global,
            id: 257,
        },
        context: FrameContext::for_entity(reference(1), 1),
        args: vec![0; 4],
        action_tree: true,
        special_result: SpecialResult::Interaction {
            run_immediately: false,
        },
    };
    host.push_results.push_back(Some(call.clone()));
    assert_eq!(
        gosub_found_action(&mut thread, &mut host).unwrap(),
        PrimitiveOutcome::Call(call)
    );
    assert_eq!(host.push_attempts[0].1.stack_object, ObjectId(2));
    thread.top_mut().unwrap().action_tree = true;
    assert_eq!(
        gosub_found_action(&mut thread, &mut host).unwrap(),
        PrimitiveOutcome::Exit(PrimitiveExit::GotoFalse)
    );
    assert_eq!(host.push_attempts.len(), 1);
}
