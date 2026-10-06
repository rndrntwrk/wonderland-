use wonderland_avatar_view::*;
use wonderland_render_core::{math::*, EntityRef};
fn close(a: Vec3, b: Vec3) {
    assert!((a - b).length() < 0.00002, "{a:?} != {b:?}");
}
#[test]
fn source_slot_positions_and_avatar_seat_height() {
    close(
        avatar_slot_position(
            Vec3::new(3.0, 6.0, 9.0),
            Vec3::new(10.0, 20.0, 0.0),
            0.0,
            2.0,
        )
        .unwrap(),
        Vec3::new(7.5, 13.5, 4.0),
    );
    let slot = VisualSlot {
        offset: Vec3::new(16.0, 32.0, 20.0),
        height: 5,
    };
    close(
        object_slot_position(Vec3::new(10.0, 20.0, 0.0), 0.0, slot, None).unwrap(),
        Vec3::new(11.0, 22.0, 4.0),
    );
    close(
        object_slot_position(Vec3::new(10.0, 20.0, 0.0), 0.0, slot, Some(0.5)).unwrap(),
        Vec3::new(11.5, 22.5, 0.25),
    );
    let chair = VisualSlot { height: 7, ..slot };
    close(
        object_slot_position(Vec3::new(10.0, 20.0, 3.0), 0.0, chair, Some(1.0)).unwrap(),
        Vec3::new(11.5, 22.5, 3.0),
    );
    assert_eq!(avatar_slot_bone(0), Some("R_FINGER0"));
    assert_eq!(avatar_slot_bone(1), Some("HEAD"));
    assert_eq!(avatar_slot_bone(2), Some("PELVIS"));
    assert_eq!(avatar_slot_bone(3), None);
}
#[test]
fn admitted_attachments_drop_departed_or_recycled_participants() {
    let holder = EntityRef {
        object_id: 1,
        generation: 1,
    };
    let child = EntityRef {
        object_id: 2,
        generation: 1,
    };
    let recycled = EntityRef {
        object_id: 1,
        generation: 2,
    };
    let a = Attachment {
        child,
        container: holder,
        slot: 0,
        endpoint_tile: Vec3::X,
    };
    let mut registry = AttachmentRegistry::new(8);
    registry.admit(&[holder, child], vec![a]).unwrap();
    assert_eq!(registry.attachments().len(), 1);
    registry.admit(&[child, recycled], vec![]).unwrap();
    assert!(registry.attachments().is_empty());
    assert!(registry.admit(&[child, recycled], vec![a]).is_err());
    assert!(registry.attachments().is_empty());
    let mut lerp = ContactInterpolation::new(Vec3::ZERO);
    close(lerp.sample(Vec3::X, 0.5).unwrap(), Vec3::new(0.5, 0.0, 0.0));
    lerp.commit(Vec3::new(3.0, 0.0, 0.0)).unwrap();
    close(
        lerp.sample(Vec3::new(3.0, 0.0, 0.0), 0.0).unwrap(),
        Vec3::new(3.0, 0.0, 0.0),
    );
}
#[test]
fn look_clamps_and_antipodal_smoothing_is_stationary() {
    let vertical = head_seek(
        Mat4::IDENTITY,
        Vec3::new(10.0, 0.0, 1.0),
        std::f32::consts::PI,
    )
    .unwrap();
    let expected = Quat::from_axis_angle(Vec3::Y, std::f32::consts::FRAC_PI_4).unwrap();
    assert!((vertical.y - expected.y).abs() < 0.000001);
    let horizontal = head_seek(
        Mat4::IDENTITY,
        Vec3::new(0.0, 10.0, 1.0),
        std::f32::consts::PI,
    )
    .unwrap();
    let expected = Quat::from_axis_angle(Vec3::X, -65.0f32.to_radians()).unwrap();
    assert!((horizontal.x - expected.x).abs() < 0.000001);
    assert_eq!(
        smooth_head(Quat::IDENTITY, Quat::new(0.0, 0.0, 0.0, -1.0), 1.0).unwrap(),
        Quat::IDENTITY
    );
    assert!(head_seek(Mat4::from_scale(Vec3::ZERO), Vec3::Y, 0.0).is_err());
}

#[test]
fn seek_fade_caps_full_weight_without_clamping_fade_out_below_zero() {
    for (committed, fraction, fading_out, expected) in [
        (15.0, 0.75, false, 1.0),
        (30.0, 0.5, false, 1.0),
        (15.0, 0.75, true, 0.95),
        (7.0, 0.5, false, 0.5),
        (0.0, 0.75, true, -0.05),
    ] {
        assert_eq!(
            seek_weight(committed, fraction, fading_out).unwrap(),
            expected,
            "committed {committed}, fraction {fraction}, fading_out {fading_out}"
        );
    }
}
