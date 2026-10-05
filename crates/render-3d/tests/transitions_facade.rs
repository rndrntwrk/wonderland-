use wonderland_render_3d::{
    camera::OrbitCamera, city::facade::*, city::transition::*, lot::*, Error,
};
use wonderland_render_core::{AssetKey, FrameStamp, Vec2};
fn destination(id: u64, location: (u16, u16), lot: u64) -> Destination {
    Destination {
        id: DestinationId(id),
        location,
        revision: 3,
        available: true,
        expected_lot_id: Some(lot),
    }
}
fn directory() -> DirectorySnapshot {
    DirectorySnapshot {
        revision: 10,
        provenance: DirectoryProvenance::Fixture,
        destinations: vec![
            destination(101, (306, 205), 9001),
            destination(202, (307, 205), 9002),
        ],
    }
}
fn intent() -> CityIntent {
    CityIntent {
        camera: OrbitCamera {
            center: Vec2::new(306.5, 205.5),
            yaw: 0.7,
            zoom: 7.,
            ..OrbitCamera::default()
        },
        selected: Some(DestinationId(101)),
    }
}
fn receipt(t: RequestTicket, lot: u64) -> AdmissionReceipt {
    AdmissionReceipt {
        destination: t.destination,
        directory_revision: t.directory_revision,
        destination_revision: t.destination_revision,
        lot_id: lot,
        epoch: 5,
    }
}
fn frame(lot: u64) -> FrameStamp {
    FrameStamp {
        lot_id: lot,
        epoch: 5,
        tick: 0,
        architecture_revision: 0,
        content: AssetKey([2; 32]),
    }
}

// Catches reusing a packed map coordinate as an ID and entering before a correct frame.
#[test]
fn two_live_ids_select_distinct_lots_and_require_the_accepted_first_frame() {
    let mut t = CityLotTransition::new(directory(), intent()).unwrap();
    assert_eq!(t.provenance(), DirectoryProvenance::Fixture);
    let first = t.begin_enter(DestinationId(101)).unwrap();
    let second = t.begin_enter(DestinationId(202)).unwrap();
    assert_ne!(first.serial, second.serial);
    assert!(matches!(
        t.admit(first, receipt(first, 9001)),
        Err(Error::StaleTransition)
    ));
    t.admit(second, receipt(second, 9002)).unwrap();
    assert!(matches!(
        t.state(),
        TransitionState::AwaitingFirstFrame { .. }
    ));
    assert!(t.present_first_frame(second, frame(9001)).is_err());
    assert!(matches!(
        t.state(),
        TransitionState::AwaitingFirstFrame { .. }
    ));
    t.present_first_frame(second, frame(9002)).unwrap();
    assert_eq!(
        t.state(),
        TransitionState::InLot {
            destination: DestinationId(202),
            lot_id: 9002,
            epoch: 5
        }
    );
    let back = t.return_to_city();
    assert_eq!(back.camera, intent().camera);
    assert_eq!(back.selected, Some(DestinationId(202)));
    assert_eq!(t.state(), TransitionState::City);
    assert!(t.begin_enter(DestinationId((306u64 << 16) | 205)).is_err());
}

// Catches a stale directory/admission response overwriting a newer selection.
#[test]
fn directory_changes_cancel_pending_tickets_and_stale_updates_are_atomic() {
    let mut t = CityLotTransition::new(directory(), intent()).unwrap();
    let ticket = t.begin_enter(DestinationId(101)).unwrap();
    let mut next = directory();
    next.revision = 11;
    next.destinations.remove(0);
    assert_eq!(t.update_directory(next).unwrap(), Some(ticket));
    assert!(t.admit(ticket, receipt(ticket, 9001)).is_err());
    assert_eq!(t.state(), TransitionState::City);
    assert_eq!(t.return_to_city().selected, None);
    assert!(matches!(
        t.update_directory(directory()),
        Err(Error::StaleTransition)
    ));
    assert!(t.begin_enter(DestinationId(101)).is_err());
    assert!(t.begin_enter(DestinationId(202)).is_ok());
}

// Catches failed admission losing saved camera intent or admitting the wrong live target.
#[test]
fn rejected_wrong_or_unavailable_admissions_preserve_city_intent() {
    let mut t = CityLotTransition::new(directory(), intent()).unwrap();
    let ticket = t.begin_enter(DestinationId(101)).unwrap();
    assert!(t.admit(ticket, receipt(ticket, 9002)).is_err());
    let back = t.reject(ticket).unwrap();
    assert_eq!(back, intent());
    assert_eq!(t.state(), TransitionState::City);
    let mut d = directory();
    d.destinations[1].available = false;
    let mut t = CityLotTransition::new(d, intent()).unwrap();
    assert!(t.begin_enter(DestinationId(202)).is_err());
    let mut duplicate = directory();
    duplicate.destinations[1].id = DestinationId(101);
    assert!(CityLotTransition::new(duplicate, intent()).is_err());
}

// Catches stale first-frame epoch and returning through an obsolete pending ticket.
#[test]
fn leaving_during_load_invalidates_late_frame_completion() {
    let mut t = CityLotTransition::new(directory(), intent()).unwrap();
    let ticket = t.begin_enter(DestinationId(101)).unwrap();
    t.admit(ticket, receipt(ticket, 9001)).unwrap();
    let mut stale = frame(9001);
    stale.epoch = 4;
    assert!(t.present_first_frame(ticket, stale).is_err());
    assert_eq!(t.return_to_city(), intent());
    assert!(t.present_first_frame(ticket, frame(9001)).is_err());
}

// Catches a facade cache that omits effective geometry, content revisions or scale.
#[test]
fn facade_mesh_and_obj_are_deterministic_and_change_with_effective_inputs() {
    let mut lot = synthetic_lot();
    let options = BuildOptions::default();
    let a = bake_facade(&lot, &options, AssetKey([1; 32]), 3, 0.5).unwrap();
    let b = bake_facade(&lot, &options, AssetKey([1; 32]), 3, 0.5).unwrap();
    assert_eq!(a.identity, b.identity);
    assert_eq!(to_obj(&a), to_obj(&b));
    assert!(to_obj(&a).contains("\nf "));
    assert!(a.bounds.max.x - a.bounds.min.x < 1.);
    assert!(!a.missing_assets.is_empty());
    assert_ne!(
        a.identity,
        bake_facade(&lot, &options, AssetKey([2; 32]), 3, 0.5)
            .unwrap()
            .identity
    );
    assert_ne!(
        a.identity,
        bake_facade(&lot, &options, AssetKey([1; 32]), 4, 0.5)
            .unwrap()
            .identity
    );
    assert_ne!(
        a.identity,
        bake_facade(&lot, &options, AssetKey([1; 32]), 3, 1.)
            .unwrap()
            .identity
    );
    lot.terrain[0] += 1;
    assert_ne!(
        a.identity,
        bake_facade(&lot, &options, AssetKey([1; 32]), 3, 0.5)
            .unwrap()
            .identity
    );
}
