//! Source-wire golden builders mirror SerializeInto at FreeSO 4c6b3e8. These
//! deterministic fixtures are testing only, never production source content.
use wonderland_vm_protocol::snapshot::*;
use wonderland_vm_protocol::*;
#[derive(Default)]
struct Wire(Vec<u8>);
impl Wire {
    fn b(&mut self, n: u8) {
        self.0.push(n)
    }
    fn i16(&mut self, n: i16) {
        self.0.extend(n.to_le_bytes())
    }
    fn u16(&mut self, n: u16) {
        self.0.extend(n.to_le_bytes())
    }
    fn i32(&mut self, n: i32) {
        self.0.extend(n.to_le_bytes())
    }
    fn u32(&mut self, n: u32) {
        self.0.extend(n.to_le_bytes())
    }
    fn u64(&mut self, n: u64) {
        self.0.extend(n.to_le_bytes())
    }
    fn f32(&mut self, n: f32) {
        self.0.extend(n.to_le_bytes())
    }
    fn text(&mut self, s: &str) {
        assert!(s.len() < 128);
        self.b(s.len() as u8);
        self.0.extend(s.as_bytes())
    }
}
fn body() -> Vec<u8> {
    let mut w = Wire::default();
    w.b(0); // TSO, not TS1
    w.u64(123);
    for n in [0, 30, 5, 12, 1, 6, 1997, 20000] {
        w.i32(n)
    }
    w.u64(999);
    for n in [2, 2, 1] {
        w.i32(n)
    }
    w.b(0);
    w.b(0);
    w.i32(8);
    for n in [0, 16, -16, 32] {
        w.i16(n)
    }
    w.i32(4);
    w.0.extend([1, 2, 3, 4]);
    for i in 0..4 {
        w.b(if i == 0 { 1 } else { 0 });
        for n in [9, 10, 11, 12, 1, 2] {
            w.u16(n)
        }
    }
    for n in [9, 0, 0, 9] {
        w.u16(n)
    }
    w.b(1);
    w.b(0);
    w.u32(16);
    w.f32(0.66);
    w.b(1);
    w.i32(1);
    w.u16(9);
    w.text("original.wall");
    w.i32(0);
    w.text("original.roof");
    w.b(1);
    w.0.extend([1, 1, 0, 0]);
    w.b(1);
    w.u64(1);
    w.u64(u64::MAX);
    w.i32(1);
    w.b(0); // one game object, common VMEntity then object extension
    w.i16(7);
    w.u32(11);
    w.u32(100);
    w.u32(42);
    w.u16(80);
    w.b(4);
    w.b(1);
    w.b(2);
    w.i32(2);
    w.i16(1);
    w.i16(2);
    w.i32(0);
    w.b(0);
    w.u32(0x12345678);
    w.u32(0);
    w.i16(0);
    w.i16(0);
    w.i32(0);
    w.i16(0);
    w.i16(0);
    w.i32(1);
    w.i16(9);
    w.i32(0);
    w.i32(0);
    w.u64(1 << 60);
    w.u64(0);
    w.i16(16);
    w.i16(32);
    w.b(1);
    w.u32(0);
    w.u32(u32::MAX);
    w.b(4);
    w.b(0);
    w.i32(1);
    w.i32(0);
    w.i32(1); // one corresponding thread, empty stack, one queue action
    w.u16(4096);
    w.u16(0);
    w.i16(7);
    w.i16(7);
    w.i16(7);
    w.u32(0x12345678);
    w.b(1);
    w.text("Drink");
    w.i32(0);
    w.i32(3);
    w.b(1);
    w.i16(50);
    w.b(0);
    w.u32(0);
    w.u32(0);
    w.u16(5);
    w.b(0);
    w.b(255);
    w.u16(0);
    w.b(0);
    w.0.extend([0; 40]);
    w.0.extend([0; 8]);
    w.b(0);
    w.b(0);
    w.b(0);
    w.b(0);
    w.u16(5);
    w.i32(0);
    w.u32(0);
    w.i32(1);
    w.b(1);
    w.text("Source");
    w.i32(100);
    w.i32(-1);
    w.i32(1);
    w.i16(7);
    w.i16(0);
    w.i16(0);
    w.b(0);
    w.i32(2);
    w.i16(1);
    w.i16(2);
    w.text("Original lot");
    w.u32(55);
    w.0.extend([0; 61]);
    w.b(7);
    w.i32(2);
    w.u32(42);
    w.i16(1);
    w.u32(42);
    w.i16(0);
    w.b(0);
    w.b(0);
    w.b(0);
    w.u32(9);
    w.i16(8);
    w.b(0);
    w.0
}
fn fsov(compressed: bool) -> Vec<u8> {
    let mut w = Wire(b"FSOv".to_vec());
    w.i32(38);
    w.b(compressed as u8);
    if compressed {
        use std::io::Write;
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gzip.write_all(&body()).unwrap();
        let b = gzip.finish().unwrap();
        w.i32(b.len() as i32);
        w.0.extend(b);
    } else {
        w.0.extend(body());
    }
    w.0
}
#[test]
fn full_source38_snapshot_consumes_geometry_entity_queue_and_platform() {
    for compressed in [false, true] {
        let b = fsov(compressed);
        let s = decode_snapshot(&b, &Default::default()).unwrap();
        assert_eq!(s.consumed, b.len());
        assert_eq!(s.source_body, body());
        assert!(matches!(s.semantics, RestoreSemantics::RefreshOnly));
        let a = &s.context.architecture;
        assert_eq!((a.width, a.height, a.stories), (2, 2, 1));
        assert_eq!(a.heights, vec![0, 16, -16, 32]);
        assert_eq!(a.grass, vec![1, 2, 3, 4]);
        assert_eq!(a.floors[0], vec![9, 0, 0, 9]);
        assert_eq!(a.walls[0][0].patterns, [9, 10, 11, 12]);
        assert_eq!(s.entities[0].guid, 0x12345678);
        assert_eq!(s.entities[0].position.x, 16);
        assert_eq!(s.threads[0].queue[0].name.as_deref(), Some("Drink"));
        assert_eq!(s.platform.lot_id, 55);
        assert_eq!(s.platform.owner_id, 42);
        assert_eq!(s.platform.roommates, vec![42]);
    }
}
#[test]
fn source_version_compression_size_and_trailing_bytes_reject() {
    let mut b = fsov(false);
    b[4..8].copy_from_slice(&37i32.to_le_bytes());
    assert_eq!(
        decode_snapshot(&b, &Default::default()).unwrap_err().kind,
        ErrorKind::UnsupportedVersion(37)
    );
    let mut b = fsov(true);
    b[9..13].copy_from_slice(&i32::MAX.to_le_bytes());
    assert!(decode_snapshot(&b, &Default::default()).is_err());
    let mut b = fsov(false);
    b.push(0);
    assert_eq!(
        decode_snapshot(&b, &Default::default()).unwrap_err().kind,
        ErrorKind::Trailing
    );
}
#[test]
fn gzip_bomb_and_architecture_shape_reject_before_projection() {
    let limits = DecodeLimits {
        max_decompressed_bytes: 32,
        ..Default::default()
    };
    assert_eq!(
        decode_snapshot(&fsov(true), &limits).unwrap_err().kind,
        ErrorKind::Limit
    );
    let mut b = fsov(false); // FSO header9 + TS1 flag1 + source Clock48; architecture width begins58
    b[58..62].copy_from_slice(&(-1i32).to_le_bytes());
    assert_eq!(
        decode_snapshot(&b, &Default::default()).unwrap_err().kind,
        ErrorKind::Invalid
    );
}
#[test]
fn state_sync_inside_tick_has_no_actor_prefix_and_leaves_next_command_aligned() {
    let state = fsov(true);
    let mut cmd = vec![12];
    cmd.extend(state);
    cmd.push(0);
    let mut b = vec![1, 1, 0, 0, 0];
    b.extend(42u32.to_le_bytes());
    b.extend(99u64.to_le_bytes());
    b.extend(2i32.to_le_bytes());
    b.extend(&cmd);
    b.extend([41, 7, 0, 0, 0]);
    let list = decode_tick_list(&b, &Default::default()).unwrap();
    assert_eq!(list.ticks[0].commands[0].consumed, cmd.len());
    assert_eq!(list.ticks[0].commands[1].actor_uid, Some(7));
    let value = serde_json::to_value(&list).unwrap();
    assert_eq!(
        value["ticks"][0]["commands"][0]["body"]["StateSync"]["snapshot"]["context"]
            ["architecture"]["width"],
        2
    );
}
impl Wire {
    fn f64(&mut self, n: f64) {
        self.0.extend(n.to_le_bytes())
    }
    fn position(&mut self, x: i16, y: i16, level: u8) {
        self.i16(x);
        self.i16(y);
        self.b(level)
    }
}
fn rich_avatar(frame: u8, async_kind: u8) -> Vec<u8> {
    let mut w = Wire::default();
    w.b(0);
    w.0.extend([0; 48]);
    for _ in 0..3 {
        w.i32(1)
    }
    w.b(0);
    w.b(0);
    w.i32(2);
    w.i16(0);
    w.i32(1);
    w.b(255);
    w.0.extend([0; 13]);
    w.u16(9);
    w.b(0);
    w.b(0);
    w.u32(16);
    w.f32(0.66);
    w.b(0);
    w.b(0);
    w.b(1);
    w.u64(0);
    w.u64(7);
    w.i32(1);
    w.b(1);
    w.i16(7);
    w.u32(11);
    w.u32(123);
    w.b(3);
    w.i32(1);
    w.u32(99);
    w.i32(1);
    w.i16(2);
    for n in [10, 2, 3, 4] {
        w.i16(n)
    }
    w.u32(4);
    w.0.extend([255, 0, 170, 231, 1]);
    w.i32(1);
    w.i16(99);
    w.i32(0);
    w.b(0);
    w.u32(1);
    w.u32(0);
    w.i16(0);
    w.i16(0);
    w.i32(0);
    w.i16(0);
    w.i16(0);
    w.i32(0);
    w.i32(0);
    w.i32(0);
    w.u64(u64::MAX);
    w.u64(1 << 60);
    w.position(16, 16, 1);
    w.u32(0);
    w.u32(u32::MAX);
    w.i32(1);
    w.text("walk");
    w.f32(1.5);
    w.b(1);
    w.i16(5);
    w.b(2);
    w.b(0);
    w.b(0);
    w.f32(1.);
    w.f32(1.);
    w.b(1);
    w.b(0);
    w.text("Hello");
    w.i32(30);
    w.i32(1);
    w.i16(-100);
    w.i16(100);
    w.b(1);
    w.f64(0.5);
    w.i32(12);
    for n in 1..=7 {
        w.i16(n)
    }
    w.i32(3);
    for n in [9, 8, 7] {
        w.i16(n)
    }
    w.i32(16);
    for n in -10..6 {
        w.i16(n)
    }
    w.i16(0);
    w.f32(std::f32::consts::FRAC_PI_2);
    w.i32(-1);
    w.u64(u64::MAX);
    w.u64(1);
    w.u64(u32::MAX as u64);
    w.text("named.sleep");
    for n in 10..18 {
        w.u64(n)
    }
    w.i32(1);
    w.text("fixture.apr");
    w.u64(u64::MAX - 1);
    w.u64(u32::MAX as u64);
    w.text("custom.head");
    w.b(1);
    w.i32(1);
    w.i32(1);
    w.b(frame);
    w.u16(4096);
    w.u16(2);
    w.i16(7);
    w.i16(7);
    w.i16(7);
    w.u32(1);
    w.i32(-1);
    w.i32(2);
    w.i16(9);
    w.i16(8);
    w.b(0);
    w.b(1);
    if frame == 1 {
        w.i32(1);
        w.i16(8);
        w.u16(2);
        w.b(1);
        w.i16(8);
        w.u16(2);
        w.i32(2);
        w.b(0);
        for n in [0, 0, 10, 10] {
            w.i32(n)
        }
        w.b(1);
        for n in [0, 0, 3, 3, 7, 7, 10, 10] {
            w.i32(n)
        }
        w.f64(0.);
        w.f64(1.);
        w.b(0);
        w.b(1);
        for _ in 0..4 {
            w.i32(0)
        }
        w.b(1);
        w.f32(0.2);
        w.i32(1);
        for n in [10, 3, 2] {
            w.i32(n)
        }
        w.b(1);
        w.i32(0);
        w.i32(1);
        w.i16(7);
        w.position(16, 16, 1);
        w.b(0);
        for n in [0, 0, 10, 10] {
            w.i32(n)
        }
        w.b(0);
        w.b(1);
        w.u16(1);
        for _ in 0..3 {
            w.f32(0.)
        }
        for _ in 0..8 {
            w.i32(0)
        }
        w.f32(0.);
        for _ in 0..3 {
            w.i32(0)
        }
        w.i16(8);
        w.i32(-1);
        w.b(0);
        w.i16(1);
    }
    if frame == 2 {
        for n in [16, 16, 1, 2] {
            w.i32(n)
        }
        w.i16(30);
        w.i16(40);
        for _ in 0..3 {
            w.f32(0.)
        }
        w.b(1);
        for _ in 0..5 {
            w.i32(0)
        }
    }
    w.i32(1);
    w.u16(4096);
    w.u16(0);
    w.i16(7);
    w.i16(7);
    w.i16(7);
    w.u32(1);
    w.b(1);
    w.text("Source queue");
    w.i32(-1);
    w.i32(3);
    w.b(1);
    w.i16(50);
    w.b(0);
    w.u32(0);
    w.u32(0);
    w.u16(5);
    w.b(1);
    w.i32(1);
    w.i16(8);
    w.i16(300);
    w.b(1);
    w.i16(7);
    w.i16(7);
    w.b(0);
    w.b(255);
    w.u16(0);
    w.b(0);
    w.0.extend([0; 48]);
    w.b(0);
    w.b(1);
    w.b(async_kind);
    w.b(1);
    w.i32(9);
    match async_kind {
        0 => {
            w.b(1);
            w.i32(100);
            for n in [7, 123, 8, 456] {
                w.u32(n)
            }
        }
        1 => {
            w.i32(30);
            w.b(2);
            w.text("dialog result");
            w.b(0)
        }
        2 => {
            w.i16(7);
            w.i16(8);
            w.b(1);
            w.b(0);
            w.b(0)
        }
        3 => {
            w.b(1);
            w.b(1);
            w.i32(123456);
            w.u32(8);
            w.b(0);
            w.i16(0);
            w.i32(2);
            w.i16(9);
            w.i16(10)
        }
        _ => {}
    }
    w.b(1);
    w.b(0);
    w.i32(3);
    w.i16(7);
    w.i16(8);
    w.b(1);
    w.b(0);
    w.b(1);
    w.i16(-2);
    w.b(2);
    w.i16(1);
    w.i16(-1);
    w.b(0);
    w.u16(5);
    w.i32(0);
    w.u32(1);
    w.i32(0);
    w.i32(0);
    w.text("Avatar lot");
    w.u32(55);
    w.0.extend([0; 61]);
    w.b(7);
    w.i32(1);
    w.u32(42);
    w.i16(0);
    w.i16(0);
    w.b(1);
    w.i32(1);
    w.text("job");
    w.b(1);
    w.b(2);
    w.b(3);
    w.b(3);
    w.b(1);
    w.b(0);
    w.text("General");
    w.text("Case");
    w.b(0);
    w.b(0);
    w.b(3);
    w.u32(u32::MAX);
    w.u32(9);
    w.i16(8);
    w.b(1);
    w.i32(0);
    w.i32(1);
    w.text("city");
    w.i32(1);
    w.i32(0);
    w.i32(1);
    w.i32(2);
    w.f32(1.25);
    let mut envelope = Wire(b"FSOv".to_vec());
    envelope.i32(38);
    envelope.b(0);
    envelope.0.extend(w.0);
    envelope.0
}
#[test]
fn avatar_routing_direct_control_async_and_eod_state_preserve_source_fields() {
    for frame in 0..=2 {
        for async_kind in 0..=3 {
            let s = decode_snapshot(&rich_avatar(frame, async_kind), &Default::default()).unwrap();
            let Appearance::Avatar(a) = &s.entities[0].appearance else {
                panic!("avatar lost")
            };
            assert_eq!(a.body.id, u64::MAX - 1);
            assert_eq!(a.head.name.as_deref(), Some("custom.head"));
            assert_eq!(a.default_suits[2].name.as_deref(), Some("named.sleep"));
            assert_eq!(a.motives, (-10i16..6).collect::<Vec<_>>());
            assert_eq!(a.animations[0].event_queue, vec![5]);
            let t = &s.threads[0];
            assert_eq!(t.stack[0].frame_type, frame);
            assert_eq!(t.blocking_state.as_ref().unwrap().kind, async_kind);
            assert_eq!(t.queue[0].callback.as_ref().unwrap().interaction, 300);
            assert_eq!(
                t.eod_connection.as_ref().unwrap().events[0].data,
                vec![1, -1]
            );
            assert_eq!(s.platform.chat_channels[0].name, "General");
            assert_eq!(s.tuning.as_ref().unwrap().types[0].1[0].1[0], (2, 1.25));
        }
    }
}
#[test]
fn unknown_stack_or_async_variant_never_yields_partial_snapshot() {
    assert!(decode_snapshot(&rich_avatar(99, 0), &Default::default()).is_err());
    assert!(decode_snapshot(&rich_avatar(0, 99), &Default::default()).is_err());
}
#[test]
fn every_truncated_prefix_and_bad_crc_reject_without_panicking() {
    let b = rich_avatar(1, 3);
    for n in 0..b.len() {
        assert!(
            decode_snapshot(&b[..n], &Default::default()).is_err(),
            "accepted truncated prefix {n}"
        );
    }
    let mut b = fsov(true);
    let last = b.len() - 1;
    b[last] ^= 1;
    assert_eq!(
        decode_snapshot(&b, &Default::default()).unwrap_err().kind,
        ErrorKind::Compression
    );
}
#[test]
fn aggregate_compressed_snapshots_share_one_decode_budget() {
    let state = fsov(true);
    let mut command = vec![12];
    command.extend(state);
    command.push(0);
    let mut b = vec![1, 1, 0, 0, 0];
    b.extend(42u32.to_le_bytes());
    b.extend(99u64.to_le_bytes());
    b.extend(2i32.to_le_bytes());
    b.extend(&command);
    b.extend(&command);
    let limits = DecodeLimits {
        max_decompressed_bytes: body().len() + 10,
        ..Default::default()
    };
    assert_eq!(
        decode_tick_list(&b, &limits).unwrap_err().kind,
        ErrorKind::Limit
    );
}
#[test]
fn source_long_keys_and_clock_values_cross_json_as_decimal_strings() {
    let s = decode_snapshot(&rich_avatar(0, 0), &Default::default()).unwrap();
    let value = serde_json::to_value(s).unwrap();
    assert_eq!(value["context"]["clock"]["ticks"], "0");
    assert_eq!(value["entities"][0]["dynamic_flags"], u64::MAX.to_string());
    assert_eq!(
        value["entities"][0]["appearance"]["Avatar"]["body"]["id"],
        (u64::MAX - 1).to_string()
    );
    assert!(value.get("source_body").is_none());
}
#[test]
fn configured_decode_budget_cannot_overflow_the_expansion_sentinel() {
    let limits = DecodeLimits {
        max_decompressed_bytes: usize::MAX,
        ..Default::default()
    };
    assert!(decode_snapshot(&fsov(true), &limits).is_ok());
}
#[test]
fn checked_in_source_wire_fixtures_match_the_independent_golden_builder() {
    let bytes = include_bytes!("fixtures/source-v38-object.fsov");
    assert_eq!(bytes.as_slice(), fsov(false));
    let expected = decode_snapshot(bytes, &Default::default()).unwrap();
    let compressed = include_bytes!("fixtures/source-v38-object-compressed.fsov");
    assert_eq!(
        decode_snapshot(compressed, &Default::default())
            .unwrap()
            .source_body,
        expected.source_body
    );
    let direct = include_bytes!("fixtures/source-v38-state-sync.bin");
    assert!(matches!(
        decode_direct_command(direct, &Default::default())
            .unwrap()
            .body,
        CommandBody::StateSync { .. }
    ));
    let tick = include_bytes!("fixtures/source-v38-state-sync-tick.bin");
    assert_eq!(
        decode_tick_list(tick, &Default::default()).unwrap().ticks[0].tick_id,
        42
    );
}
