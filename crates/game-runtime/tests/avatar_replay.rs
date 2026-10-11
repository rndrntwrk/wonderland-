#[allow(dead_code)]
#[path = "live_session/support.rs"]
mod support;
use wonderland_game_runtime::live_session::TickFrame;
use wonderland_game_runtime::sim_core::vm::{EntityField, MemoryAddress};
use wonderland_game_runtime::{AcceptedCommand, EntityRef, GameRuntime};
fn change(server: &mut GameRuntime, actor: EntityRef, scale: i16) -> TickFrame {
    let accepted = server
        .sim()
        .next_tick(vec![AcceptedCommand::WriteMemory {
            address: MemoryAddress::Entity {
                entity: actor,
                field: EntityField::PersonData,
                index: 63,
            },
            value: scale,
        }])
        .unwrap();
    let outcome = server.apply_accepted(&accepted).unwrap();
    TickFrame {
        accepted,
        state_hash: outcome.state_hash,
    }
}
#[test]
fn a_complete_batch_captures_intermediate_accepted_avatar_frames_in_order() {
    let (mut server, mut replica, actor) = support::pair_with_content(
        support::content("cursebook_set_permission.iff", 4107, false),
        true,
    );
    let first = change(&mut server, actor, 25);
    let first_expected = server.avatar_visual_frame();
    let second = change(&mut server, actor, 50);
    let second_expected = server.avatar_visual_frame();
    let (outcomes, frames) = replica
        .apply_batch_with_avatar_frames(replica.connection(), &[first, second])
        .unwrap();
    assert_eq!(outcomes.len(), 2);
    assert_eq!(frames.unwrap(), vec![first_expected, second_expected]);
    assert_eq!(
        replica.runtime().unwrap().snapshot().unwrap(),
        server.snapshot().unwrap()
    );
}
#[test]
fn a_bad_later_tick_publishes_neither_avatar_frames_nor_earlier_state() {
    let (mut server, mut replica, actor) = support::pair_with_content(
        support::content("cursebook_set_permission.iff", 4107, false),
        true,
    );
    let before = replica.runtime().unwrap().snapshot().unwrap();
    let first = change(&mut server, actor, 25);
    let mut second = change(&mut server, actor, 50);
    second.state_hash[0] ^= 1;
    assert!(
        replica
            .apply_batch_with_avatar_frames(replica.connection(), &[first, second])
            .is_err()
    );
    assert_eq!(replica.runtime().unwrap().snapshot().unwrap(), before);
}
#[test]
fn latest_tick_duplicates_cannot_commit_a_retained_blend_twice() {
    let (mut server, mut replica, actor) = support::pair_with_content(
        support::content("cursebook_set_permission.iff", 4107, false),
        true,
    );
    let frame = change(&mut server, actor, 25);
    replica
        .apply_batch_with_avatar_frames(replica.connection(), std::slice::from_ref(&frame))
        .unwrap();
    let (outcomes, frames) = replica
        .apply_batch_with_avatar_frames(replica.connection(), &[frame])
        .unwrap();
    assert!(outcomes.is_empty());
    assert!(frames.unwrap().is_empty());
}
#[test]
fn presentation_capture_leaves_the_existing_replay_hash_and_events_unchanged() {
    let (mut server, mut a, actor) = support::pair_with_content(
        support::content("cursebook_set_permission.iff", 4107, false),
        true,
    );
    let (_, mut b, _) = support::pair_with_content(
        support::content("cursebook_set_permission.iff", 4107, false),
        true,
    );
    for scale in [25, 40, 70, 100] {
        let frame = change(&mut server, actor, scale);
        let normal = a
            .apply_batch(a.connection(), std::slice::from_ref(&frame))
            .unwrap();
        let (visual, _) = b
            .apply_batch_with_avatar_frames(b.connection(), &[frame])
            .unwrap();
        assert_eq!(normal, visual);
        assert_eq!(
            a.runtime().unwrap().snapshot().unwrap(),
            b.runtime().unwrap().snapshot().unwrap()
        );
    }
}
