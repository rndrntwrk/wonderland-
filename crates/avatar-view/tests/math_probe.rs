//! Numeric vectors measured by tools/swarm-c/avatar-cooker/source_probe.py.
//! The checkout DLL identifies itself as version 0.0.0.0; this is evidence for
//! that SHA256-pinned assembly, not verification of the project's 3.6 NuGet pin.
//! Explicit binary32 patterns preserve the measured values at their exact original precision.
use wonderland_avatar_view::*;
use wonderland_render_core::math::*;
fn close(q: Quat, expected: [f32; 4]) {
    let actual = [q.x, q.y, q.z, q.w];
    for i in 0..4 {
        assert!(
            (actual[i] - expected[i]).abs() <= 0.0000005,
            "{actual:?} != {expected:?}"
        );
    }
}
#[test]
fn quaternion_unit_vectors_match_local_source_probe_with_explicit_tolerance() {
    close(
        Quat::IDENTITY.slerp(Quat::from_axis_angle(Vec3::Y, 1.0).unwrap(), 1.0),
        [0.0, 0.4794256, 0.0, f32::from_bits(0x3f60a942)],
    );
    close(
        Quat::IDENTITY.slerp(Quat::new(0.0, 0.0, 0.0, -1.0), 0.5),
        [0.0, 0.0, 0.0, 1.0],
    );
    close(
        Quat::IDENTITY.slerp(Quat::from_axis_angle(Vec3::Y, 0.01).unwrap(), 0.37),
        [0.0, 0.001849999, 0.0, f32::from_bits(0x3f7fffe4)],
    );
    close(
        Quat::IDENTITY.slerp(Quat::new(0.0, 1.0, 0.0, 0.0), 0.5),
        [
            0.0,
            f32::from_bits(0x3f3504f3),
            0.0,
            f32::from_bits(0x3f3504f3),
        ],
    );
    close(
        head_seek(
            Mat4::IDENTITY,
            Vec3::new(10.0, 0.0, 1.0),
            std::f32::consts::PI,
        )
        .unwrap(),
        [
            0.0,
            f32::from_bits(0x3ec3ef15),
            0.0,
            f32::from_bits(0x3f6c835f),
        ],
    );
    close(
        head_seek(
            Mat4::IDENTITY,
            Vec3::new(0.0, 10.0, 1.0),
            std::f32::consts::PI,
        )
        .unwrap(),
        [
            f32::from_bits(0xbf098c78),
            f32::from_bits(0x317c1528),
            f32::from_bits(0xb1209828),
            0.8433914,
        ],
    );
}
