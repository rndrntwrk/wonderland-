use std::cell::Cell;
use std::rc::Rc;
use wonderland_render_core::{cache::*, frame::*, *};

fn entity(id: u32, generation: u32, revision: u64) -> EntityProjection {
    EntityProjection {
        reference: EntityRef {
            object_id: id,
            generation,
        },
        visual_revision: revision,
        transform: Transform::IDENTITY,
        previous_transform: None,
        asset: AssetKey([8; 32]),
        level: 1,
        visible: true,
        selectable: true,
    }
}
fn frame(tick: u64, entities: Vec<EntityProjection>) -> RenderFrame {
    RenderFrame {
        stamp: FrameStamp {
            lot_id: 7,
            epoch: 2,
            tick,
            architecture_revision: 3,
            content: AssetKey([9; 32]),
        },
        entities,
        selected: None,
    }
}
fn store() -> FrameStore {
    let mut s = FrameStore::new(RenderLimits::default());
    s.reset(7, 2);
    s
}

#[test]
fn rejection_is_atomic_for_duplicate_invalid_and_inconsistent_selection() {
    let mut s = store();
    s.admit(frame(1, vec![entity(1, 1, 1)])).unwrap();
    let original = s.current().unwrap().clone();
    let duplicate = frame(2, vec![entity(1, 1, 2), entity(1, 2, 2)]);
    assert!(s.admit(duplicate).is_err());
    assert_eq!(s.current(), Some(&original));
    let mut invalid = frame(2, vec![entity(1, 1, 2)]);
    invalid.entities[0].transform.translation.x = f32::NAN;
    assert!(s.admit(invalid).is_err());
    assert_eq!(s.current(), Some(&original));
    let mut selected = frame(2, vec![entity(1, 1, 2)]);
    selected.selected = Some(EntityRef {
        object_id: 1,
        generation: 2,
    });
    assert!(s.admit(selected).is_err());
    assert_eq!(s.current(), Some(&original));
}

#[test]
fn admission_requires_explicit_boundary_and_monotonic_full_frames() {
    let mut s = FrameStore::new(RenderLimits::default());
    assert!(s.admit(frame(1, vec![])).is_err());
    s.reset(7, 2);
    s.admit(frame(1, vec![entity(1, 1, 4)])).unwrap();
    s.admit(frame(90, vec![entity(1, 1, 4)])).unwrap();
    assert!(s.admit(frame(89, vec![entity(1, 1, 4)])).is_err());
    assert!(s.admit(frame(91, vec![entity(1, 1, 3)])).is_err());
    let mut old = frame(91, vec![entity(1, 1, 5)]);
    old.stamp.architecture_revision = 2;
    assert!(s.admit(old).is_err());
    let mut new_lot = frame(91, vec![]);
    new_lot.stamp.lot_id = 8;
    assert!(s.admit(new_lot.clone()).is_err());
    s.reset(8, 2);
    s.admit(new_lot).unwrap();
}

#[test]
fn generation_zero_reuse_and_unversioned_visual_changes_are_rejected() {
    let mut s = store();
    assert!(s.admit(frame(1, vec![entity(1, 0, 1)])).is_err());
    s.admit(frame(1, vec![entity(1, 3, 1)])).unwrap();
    let mut moved = frame(2, vec![entity(1, 3, 1)]);
    moved.entities[0].transform.translation.x = 1.;
    assert!(s.admit(moved).is_err());
    s.admit(frame(2, vec![])).unwrap();
    assert!(s.admit(frame(3, vec![entity(1, 3, 2)])).is_err());
    s.admit(frame(3, vec![entity(1, 4, 1)])).unwrap();
}

#[test]
fn pick_survives_unrelated_update_but_not_target_motion_content_or_device_reset() {
    let mut s = store();
    s.admit(frame(1, vec![entity(1, 1, 1), entity(2, 1, 1)]))
        .unwrap();
    let ticket = s
        .pick_ticket(EntityRef {
            object_id: 1,
            generation: 1,
        })
        .unwrap();
    s.admit(frame(2, vec![entity(1, 1, 1), entity(2, 1, 2)]))
        .unwrap();
    assert_eq!(s.resolve_pick(&ticket), Some(ticket.reference));
    let mut moved = frame(3, vec![entity(1, 1, 2), entity(2, 1, 2)]);
    moved.entities[0].transform.translation.x = 1.;
    s.admit(moved).unwrap();
    assert_eq!(s.resolve_pick(&ticket), None);
    let ticket = s.pick_ticket(ticket.reference).unwrap();
    s.device_reset();
    assert_eq!(s.resolve_pick(&ticket), None);
    let ticket = s.pick_ticket(ticket.reference).unwrap();
    let mut content = frame(4, vec![entity(1, 1, 3)]);
    content.stamp.content = AssetKey([4; 32]);
    s.admit(content).unwrap();
    assert_eq!(s.resolve_pick(&ticket), None);
}

#[test]
fn pick_fails_after_deletion_reuse_and_even_same_boundary_reset() {
    let mut s = store();
    s.admit(frame(1, vec![entity(1, 1, 1)])).unwrap();
    let old = s
        .pick_ticket(EntityRef {
            object_id: 1,
            generation: 1,
        })
        .unwrap();
    s.admit(frame(2, vec![])).unwrap();
    assert_eq!(s.resolve_pick(&old), None);
    s.admit(frame(3, vec![entity(1, 2, 1)])).unwrap();
    assert_eq!(s.resolve_pick(&old), None);
    let current = s
        .pick_ticket(EntityRef {
            object_id: 1,
            generation: 2,
        })
        .unwrap();
    s.reset(7, 2);
    s.admit(frame(1, vec![entity(1, 2, 1)])).unwrap();
    assert_eq!(s.resolve_pick(&current), None);
}

fn key(n: u8) -> DerivedKey {
    DerivedKey::new(AssetKey([n; 32]), AssetKey([4; 32]), 1, &[5])
}
fn bytes(n: u64) -> ResourceBytes {
    ResourceBytes {
        encoded: n,
        decoded_cpu: n,
        staging: 0,
        gpu: n,
    }
}
#[derive(Debug)]
struct Owned(Rc<Cell<u32>>);
impl Drop for Owned {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn cache_accounts_each_category_evicts_lru_and_drops_owned_handles() {
    let dropped = Rc::new(Cell::new(0));
    let mut cache = BoundedCache::new(bytes(2), 3);
    cache
        .insert(key(1), Owned(dropped.clone()), bytes(1))
        .unwrap();
    cache
        .insert(key(2), Owned(dropped.clone()), bytes(1))
        .unwrap();
    assert!(cache.get(&key(1)).is_some());
    cache
        .insert(key(3), Owned(dropped.clone()), bytes(1))
        .unwrap();
    assert!(cache.get(&key(2)).is_none());
    assert_eq!(dropped.get(), 1);
    assert_eq!(cache.resident_bytes(), bytes(2));
    cache.clear();
    assert_eq!(dropped.get(), 3);
    assert_eq!(cache.resident_bytes(), ResourceBytes::ZERO);
}

#[test]
fn pinned_entries_make_failed_admission_atomic_and_category_budgets_apply() {
    let mut cache = BoundedCache::new(bytes(1), 2);
    cache.insert(key(1), 7, bytes(1)).unwrap();
    assert!(cache.pin(&key(1)));
    assert!(cache.insert(key(2), 8, bytes(1)).is_err());
    assert_eq!(cache.get(&key(1)), Some(&7));
    assert!(cache.unpin(&key(1)));
    cache.insert(key(2), 8, bytes(1)).unwrap();
    assert!(cache.get(&key(1)).is_none());
    assert!(cache
        .insert(
            key(3),
            9,
            ResourceBytes {
                encoded: 0,
                decoded_cpu: 0,
                staging: 1,
                gpu: 0
            }
        )
        .is_err());
    assert_eq!(cache.get(&key(2)), Some(&8));
}

#[test]
fn device_reset_releases_gpu_handle_without_discarding_immutable_cpu_data() {
    let dropped = Rc::new(Cell::new(0));
    let mut resource = CachedResource::new(vec![1, 2], Some(Owned(dropped.clone())));
    let mut cache = BoundedCache::new(bytes(10), 2);
    cache.insert(key(1), resource, bytes(3)).unwrap();
    cache.device_reset();
    assert_eq!(dropped.get(), 1);
    resource = cache.remove(&key(1)).unwrap();
    assert_eq!(resource.decoded, vec![1, 2]);
    assert!(resource.gpu.is_none());
    assert_eq!(cache.resident_bytes(), ResourceBytes::ZERO);
}

#[test]
fn derivation_and_thumbnail_identity_include_effective_inputs() {
    assert_ne!(key(1), key(2));
    assert_ne!(
        key(1),
        DerivedKey::new(AssetKey([1; 32]), AssetKey([4; 32]), 2, &[5])
    );
    assert_ne!(
        key(1),
        DerivedKey::new(AssetKey([1; 32]), AssetKey([4; 32]), 1, &[6])
    );
    let a = ThumbnailKey::new(key(1), ViewMode::Full2D, 64, 64, &[1, 2]);
    assert_ne!(
        a,
        ThumbnailKey::new(key(1), ViewMode::Full3D, 64, 64, &[1, 2])
    );
    assert_ne!(
        a,
        ThumbnailKey::new(key(1), ViewMode::Full2D, 128, 64, &[1, 2])
    );
    assert_ne!(
        a,
        ThumbnailKey::new(key(1), ViewMode::Full2D, 64, 64, &[2, 1])
    );
}

#[test]
fn checked_byte_accounting_rejects_overflow() {
    assert!(ResourceBytes {
        encoded: u64::MAX,
        decoded_cpu: 0,
        staging: 0,
        gpu: 0
    }
    .checked_add(bytes(1))
    .is_none());
    assert!(ResourceBytes {
        encoded: u64::MAX,
        decoded_cpu: 1,
        staging: 0,
        gpu: 0
    }
    .total()
    .is_none());
}

#[test]
fn zero_cost_entries_still_obey_entry_limit_and_pins_prevent_removal() {
    let mut c = BoundedCache::new(ResourceBytes::ZERO, 1);
    c.insert(key(1), 1, ResourceBytes::ZERO).unwrap();
    c.pin(&key(1));
    assert_eq!(c.remove(&key(1)), None);
    assert!(c.insert(key(1), 2, ResourceBytes::ZERO).is_err());
    assert!(c.insert(key(2), 2, ResourceBytes::ZERO).is_err());
    assert_eq!(c.get(&key(1)), Some(&1));
    c.unpin(&key(1));
    c.insert(key(2), 2, ResourceBytes::ZERO).unwrap();
    assert_eq!(c.len(), 1);
    assert_eq!(c.get(&key(1)), None);
}

#[test]
fn identity_history_budget_rejects_churn_atomically_and_reset_releases_it() {
    let limits = RenderLimits {
        max_entities: 1,
        ..Default::default()
    };
    let mut s = FrameStore::new(limits);
    s.reset(7, 2);
    s.admit(frame(1, vec![entity(1, 1, 1)])).unwrap();
    s.admit(frame(2, vec![])).unwrap();
    assert!(s.admit(frame(3, vec![entity(2, 1, 1)])).is_err());
    assert_eq!(s.current().unwrap().stamp.tick, 2);
    s.reset(7, 2);
    s.admit(frame(1, vec![entity(2, 1, 1)])).unwrap();
}

#[test]
fn interpolation_history_change_requires_visual_revision_and_invalidates_picks() {
    let mut s = store();
    s.admit(frame(1, vec![entity(1, 1, 1)])).unwrap();
    let ticket = s
        .pick_ticket(EntityRef {
            object_id: 1,
            generation: 1,
        })
        .unwrap();
    let mut changed = frame(2, vec![entity(1, 1, 1)]);
    let mut previous = Transform::IDENTITY;
    previous.translation.x = 10.;
    changed.entities[0].previous_transform = Some(previous);
    assert!(s.admit(changed.clone()).is_err());
    assert_eq!(s.resolve_pick(&ticket), Some(ticket.reference));
    changed.entities[0].visual_revision = 2;
    s.admit(changed).unwrap();
    assert_eq!(s.resolve_pick(&ticket), None);
}
