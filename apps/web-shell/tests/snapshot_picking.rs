use wonderland_web_shell::{
    live_world_adapter::{LiveWorldIdentity, SnapshotPickAction, SourceFrameGate},
    snapshot_world::snapshot_world,
};
use wonderland_world_view::{WorldDocument, WorldPick, WorldPickTarget, WorldSurface};

#[path = "support/snapshot.rs"]
mod support;

fn fixture() -> (SourceFrameGate, LiveWorldIdentity, WorldDocument, WorldPick) {
    let source = support::fixture();
    let identity = LiveWorldIdentity {
        browser_epoch: 2,
        source_epoch: 3,
        lot_incarnation: 4,
        lot_location: source.platform.lot_id,
        avatar_id: 5,
    };
    let mut document = snapshot_world(&source, identity.source_epoch, 123, 1).unwrap();
    let object = &mut document.objects[0];
    object.snapshot.as_mut().unwrap().avatar = true;
    let pick = WorldPick {
        revision: document.revision,
        frame_generation: 1,
        target: WorldPickTarget::Object {
            entity: None,
            source_guid: object.source_guid,
            source_record: Some(object.snapshot.unwrap().record),
        },
        screen: [40, 60],
    };
    document.validate().unwrap();
    let mut gate = SourceFrameGate::default();
    gate.reset(Some(identity));
    (gate, identity, document, pick)
}

#[test]
fn ordinary_tick_keeps_avatar_profiles_available_while_edits_require_refresh() {
    let (mut gate, identity, mut document, pick) = fixture();
    let profile = SnapshotPickAction::InspectAvatar(Some(
        document.objects[0].snapshot.unwrap().persistent_id,
    ));
    assert_eq!(
        gate.pick_action(Some(identity), &document, &pick, false),
        Some(profile)
    );

    // Original VMNetTickList: one ordinary tick with zero commands still
    // advances autonomous simulation and makes the accepted snapshot stale.
    let mut tick = vec![0, 1, 0, 0, 0];
    tick.extend(42u32.to_le_bytes());
    tick.extend(u64::MAX.to_le_bytes());
    tick.extend(0i32.to_le_bytes());
    let update = gate.admit(identity, false, &tick).unwrap();
    assert!(update.needs_refresh);
    assert_eq!(
        gate.pick_action(Some(identity), &document, &pick, update.needs_refresh),
        Some(profile)
    );

    document.objects[0].snapshot.as_mut().unwrap().avatar = false;
    assert_eq!(
        gate.pick_action(Some(identity), &document, &pick, update.needs_refresh),
        Some(SnapshotPickAction::RefreshRequired)
    );
    assert_eq!(
        gate.pick_action(Some(identity), &document, &pick, false),
        Some(SnapshotPickAction::Authoring)
    );
    let tile = WorldPick {
        target: WorldPickTarget::Tile {
            x: 1,
            y: 1,
            level: 1,
            surface: WorldSurface::Floor,
        },
        ..pick
    };
    assert_eq!(
        gate.pick_action(Some(identity), &document, &tile, update.needs_refresh),
        Some(SnapshotPickAction::RefreshRequired)
    );
}

#[test]
fn avatar_inspection_rejects_unavailable_sessions_and_mismatched_source_picks() {
    let (gate, identity, document, pick) = fixture();
    assert_eq!(gate.pick_action(None, &document, &pick, true), None);
    for wrong in [
        LiveWorldIdentity {
            browser_epoch: identity.browser_epoch + 1,
            ..identity
        },
        LiveWorldIdentity {
            source_epoch: identity.source_epoch + 1,
            ..identity
        },
        LiveWorldIdentity {
            lot_incarnation: identity.lot_incarnation + 1,
            ..identity
        },
        LiveWorldIdentity {
            avatar_id: identity.avatar_id + 1,
            ..identity
        },
    ] {
        assert_eq!(gate.pick_action(Some(wrong), &document, &pick, true), None);
    }
    let mut old_pick = pick.clone();
    old_pick.revision.architecture_revision += 1;
    assert_eq!(
        gate.pick_action(Some(identity), &document, &old_pick, true),
        None
    );
    for (guid, record) in [
        (document.objects[0].source_guid ^ 1, 0),
        (document.objects[0].source_guid, u32::MAX),
    ] {
        let wrong_pick = WorldPick {
            target: WorldPickTarget::Object {
                entity: None,
                source_guid: guid,
                source_record: Some(record),
            },
            ..pick.clone()
        };
        assert_eq!(
            gate.pick_action(Some(identity), &document, &wrong_pick, true),
            None
        );
    }
}
