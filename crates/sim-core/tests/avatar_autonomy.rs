use sim_core::avatars::advertisements::{
    InteractionCandidate, InteractionVariant, MotiveAdvertisement, OfferStatus,
};
use sim_core::avatars::autonomy::{
    score_candidates, select, select_with_random, AutonomyContext, AutonomySelection,
    AutonomyTuning, QueuedPriority, ScoreCurve,
};
use sim_core::avatars::motives::{Motive, MotiveState};
use sim_core::avatars::state::PersonData;
use sim_core::avatars::AvatarPlatform;
use sim_core::ids::{EntityRef, ObjectId};
use sim_core::rng::SimRng;
use std::collections::BTreeMap;

fn entity(id: i16) -> EntityRef {
    EntityRef {
        object_id: ObjectId(id),
        generation: 1,
    }
}
fn context() -> AutonomyContext {
    let mut motives = MotiveState::default();
    motives.set(Motive::Hunger, -50);
    AutonomyContext {
        platform: AvatarPlatform::Ts1,
        motives,
        person_data: PersonData::default(),
        x: 0,
        y: 0,
        level: 1,
        queued: Vec::new(),
    }
}
fn tuning() -> AutonomyTuning {
    let curve = ScoreCurve::new(vec![(-100, -100), (100, 100)]).unwrap();
    AutonomyTuning {
        adult_curves: std::array::from_fn(|_| curve.clone()),
        child_curves: std::array::from_fn(|_| curve.clone()),
    }
}
fn candidate(id: i16, order: u64) -> InteractionCandidate {
    let mut candidate = InteractionCandidate::new(entity(id), order, 300);
    candidate.advertisements.push(MotiveAdvertisement {
        motive: Motive::Hunger,
        minimum: 0,
        delta: 1000,
        personality_modifier: 0,
    });
    candidate
}

#[test]
fn avatar_autonomy_host_rng_adapter_keeps_exact_draw_count_and_bound() {
    let mut ctx = context();
    let tuning = tuning();
    let mut offer = candidate(2, 0);
    let mut draws = Vec::new();
    let selected = select_with_random(&ctx, &[offer.clone()], &tuning, |bound| {
        draws.push(bound);
        9999
    })
    .unwrap();
    assert!(matches!(selected, AutonomySelection::Selected(_)));
    assert_eq!(draws, [10_000]);
    offer.auto_first = true;
    select_with_random(&ctx, &[offer], &tuning, |_| {
        panic!("AutoFirst must not draw")
    })
    .unwrap();
    assert_eq!(
        select_with_random(&ctx, &[], &tuning, |_| panic!("empty result must not draw")).unwrap(),
        AutonomySelection::NoValidTarget
    );
    ctx.queued.push(QueuedPriority {
        priority: 1,
        target: entity(9),
    });
    assert_eq!(
        select_with_random(&ctx, &[], &tuning, |_| panic!(
            "queued shortcut must not draw"
        ))
        .unwrap(),
        AutonomySelection::AlreadyQueued(entity(9))
    );
}

#[test]
fn avatar_autonomy_source_ties_first_variant_duplication_and_no_ui_rng() {
    let ctx = context();
    let tuning = tuning();
    let mut a = candidate(2, 0);
    let b = candidate(3, 1);
    a.variants = vec![
        InteractionVariant {
            param0: 7,
            motive_ad_changes: None,
        },
        InteractionVariant {
            param0: 9,
            motive_ad_changes: None,
        },
    ];
    let offers = vec![b, a];
    let mut rng = SimRng::new(123);
    let initial = rng.state();
    let ranked = score_candidates(&ctx, &offers, &tuning).unwrap();
    assert_eq!(rng.state(), initial);
    assert_eq!(ranked.len(), 3);
    assert_eq!(ranked[0].target, entity(2));
    assert_eq!(ranked[1].param0, 7);
    assert_eq!(ranked[0].interaction_id, 44);
    for _ in 0..30 {
        assert_eq!(ranked, score_candidates(&ctx, &offers, &tuning).unwrap());
    }
    let mut control = SimRng::new(123);
    assert_eq!(
        select(&ctx, &offers, &tuning, &mut rng).unwrap(),
        select(&ctx, &offers, &tuning, &mut control).unwrap()
    );
    assert_eq!(rng.state(), control.state());
    assert_eq!(rng.state(), 4_127_195_237); // Source seed 123 after exactly one xorshift draw.
}

#[test]
fn avatar_autonomy_queue_autofirst_and_no_candidate_do_not_draw_rng() {
    let mut ctx = context();
    let tuning = tuning();
    let mut rng = SimRng::new(3);
    let seed = rng.state();
    let mut offer = candidate(2, 0);
    offer.auto_first = true;
    assert!(matches!(
        select(&ctx, &[offer.clone()], &tuning, &mut rng).unwrap(),
        AutonomySelection::Selected(_)
    ));
    ctx.queued.push(QueuedPriority {
        priority: 1,
        target: entity(7),
    });
    assert_eq!(
        select(&ctx, &[offer.clone()], &tuning, &mut rng).unwrap(),
        AutonomySelection::AlreadyQueued(entity(7))
    );
    ctx.queued.clear();
    offer.status = OfferStatus::PermissionDenied;
    assert_eq!(
        select(&ctx, &[offer], &tuning, &mut rng).unwrap(),
        AutonomySelection::NoValidTarget
    );
    assert_eq!(rng.state(), seed);
}

#[test]
fn avatar_autonomy_repairs_carrying_occupancy_and_stray_filters_are_explicit() {
    let mut ctx = context();
    let tuning = tuning();
    let mut offers: Vec<_> = (0..6).map(|i| candidate(i + 2, i as u64)).collect();
    offers[0].status = OfferStatus::RepairMismatch;
    offers[1].status = OfferStatus::CarryingDenied;
    offers[2].disabled = true;
    offers[3].occupied = true;
    offers[4].use_count = 1;
    offers[5].use_count = 1;
    offers[5].set_joining_indices(&[300, 44]);
    assert_eq!(score_candidates(&ctx, &offers, &tuning).unwrap().len(), 1);
    offers[0].status = OfferStatus::Available;
    offers[0].is_game_object = false;
    ctx.person_data.values[65] = 16;
    ctx.person_data.values[32] = 1;
    assert_eq!(score_candidates(&ctx, &offers, &tuning).unwrap().len(), 1);
    offers[0].outside = false;
    assert!(score_candidates(&ctx, &offers, &tuning).unwrap().is_empty());
}

#[test]
fn avatar_autonomy_joining_checks_use_distinct_pre_and_post_narrow_ids() {
    let ctx = context();
    let tuning = tuning();
    let mut offer = candidate(2, 0);
    offer.occupied = true;
    offer.use_count = 1;
    offer.set_joining_indices(&[300]);
    assert!(score_candidates(&ctx, &[offer.clone()], &tuning)
        .unwrap()
        .is_empty());
    offer.set_joining_indices(&[300, 44]);
    assert_eq!(score_candidates(&ctx, &[offer], &tuning).unwrap().len(), 1);
}

#[test]
fn avatar_autonomy_advertisement_order_matches_source_motive_array() {
    let mut ctx = context();
    ctx.motives.set(Motive::Energy, -99);
    ctx.motives.set(Motive::Hunger, -99);
    let tuning = tuning();
    let mut offer = candidate(2, 0);
    offer.advertisements.push(MotiveAdvertisement {
        motive: Motive::Energy,
        minimum: 0,
        delta: 1,
        personality_modifier: 0,
    });
    let forward = score_candidates(&ctx, &[offer.clone()], &tuning).unwrap();
    offer.advertisements.reverse();
    let reverse = score_candidates(&ctx, &[offer], &tuning).unwrap();
    assert_eq!(forward, reverse);
    assert_eq!(forward[0].score.to_bits(), 0x3de3c800);

    let motives = [
        Motive::Mood,
        Motive::Energy,
        Motive::Comfort,
        Motive::Hunger,
        Motive::Hygiene,
        Motive::Bladder,
        Motive::Room,
        Motive::Social,
        Motive::Fun,
    ];
    let values = [-5, -63, 43, -100, -90, 0, -9, 51, -38];
    let deltas = [28534, 600, 16228, 3860, 19699, 1877, 16979, 945, 9366];
    let mut offer = candidate(2, 0);
    offer.advertisements.clear();
    for i in 0..9 {
        ctx.motives.set(motives[i], values[i]);
        offer.advertisements.push(MotiveAdvertisement {
            motive: motives[i],
            minimum: 0,
            delta: deltas[i],
            personality_modifier: 0,
        });
    }
    let source = score_candidates(&ctx, &[offer.clone()], &tuning).unwrap();
    offer.advertisements.reverse();
    let reordered = score_candidates(&ctx, &[offer], &tuning).unwrap();
    assert_eq!(source, reordered);
    assert_eq!(source[0].score.to_bits(), 0x412e60ee);
}

#[test]
fn avatar_autonomy_ad_override_missing_keys_zero_and_delta_narrows() {
    let ctx = context();
    let tuning = tuning();
    let mut offer = candidate(2, 0);
    offer.variants[0].motive_ad_changes = Some(BTreeMap::new());
    assert!(score_candidates(&ctx, &[offer.clone()], &tuning)
        .unwrap()
        .is_empty());
    offer.variants[0].motive_ad_changes = Some(BTreeMap::from([(7, 1000)]));
    assert_eq!(
        score_candidates(&ctx, &[offer.clone()], &tuning)
            .unwrap()
            .len(),
        1
    );
    offer.variants[0].motive_ad_changes = None;
    offer.advertisements[0].minimum = 30000;
    offer.advertisements[0].delta = 30000;
    assert!(score_candidates(&ctx, &[offer], &tuning)
        .unwrap()
        .is_empty());
}

#[test]
fn avatar_autonomy_source_curve_duplicate_x_and_distance_attenuation() {
    let curve = ScoreCurve::new(vec![(0, 0), (10, 10), (10, 20), (20, 30)]).unwrap();
    assert_eq!(curve.sample(10.0), 20.0);
    assert_eq!(curve.sample(15.0), 25.0);
    let mut ctx = context();
    let tuning = tuning();
    let mut offer = candidate(2, 0);
    offer.x = 160;
    offer.attenuation_code = 2;
    let resident = score_candidates(&ctx, &[offer.clone()], &tuning).unwrap()[0].score;
    ctx.person_data.values[32] = 1;
    let visitor = score_candidates(&ctx, &[offer], &tuning).unwrap()[0].score;
    assert!(visitor > resident);
}
