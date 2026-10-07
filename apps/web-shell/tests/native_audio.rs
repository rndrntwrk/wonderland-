use std::sync::Arc;
use wonderland_audio_content::pack::*;
use wonderland_audio_runtime::mixer::MixerIntent;
use wonderland_game_runtime::sim_core::{
    avatars::{AvatarTickOutput, events::AnimationCue},
    ids::ObjectId,
};
use wonderland_game_runtime::{EntityRef, RuntimeEvent, RuntimeProjection, TickOutcome, VmMode};
use wonderland_web_shell::native_audio::NativeAudio;
fn pack() -> Arc<AudioPack> {
    let mut hit = b"HIT!".to_vec();
    hit.extend(1u32.to_le_bytes());
    hit.extend(0u32.to_le_bytes());
    hit.extend(b"TSO!ENTP");
    hit.extend(7u32.to_le_bytes());
    hit.extend(32u32.to_le_bytes());
    hit.extend(b"EENT");
    hit.extend([2, 0, 0x0b, 0x0c]);
    let mut wav = b"RIFF".to_vec();
    wav.extend((36 + 16000u32).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(8000u32.to_le_bytes());
    wav.extend(16000u32.to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend(16000u32.to_le_bytes());
    for _ in 0..8000 {
        wav.extend(12000i16.to_le_bytes());
    }
    Arc::new(
        AudioPack::load(
            AudioPackSpec {
                version: 1,
                groups: vec![GroupSpec {
                    kind: "new_main".into(),
                    hit: "s.hit".into(),
                    events: "s.evt".into(),
                    hsm: None,
                }],
                tracks: vec![TrackSpec {
                    instance: 7,
                    file: "s.trk".into(),
                }],
                samples: vec![SampleSpec {
                    id: 8,
                    file: "s.wav".into(),
                    group: "fx".into(),
                }],
                fwav: vec![],
            },
            &[
                ("s.hit".into(), hit),
                ("s.evt".into(), b"fixture,1,7,0,0,0,0\r\n".to_vec()),
                ("s.trk".into(), b"TKDT,1,fixture,8,7,ETKD".to_vec()),
                ("s.wav".into(), wav),
            ],
        )
        .unwrap(),
    )
}
fn entity() -> EntityRef {
    EntityRef {
        object_id: ObjectId(1),
        generation: 1,
    }
}
fn projection(tick: u64) -> RuntimeProjection {
    let lot = wonderland_game_runtime::LotModel::new(8, 8, 1).unwrap();
    let rt = wonderland_game_runtime::GameRuntime::new(
        wonderland_game_runtime::sim_core::state::ContentSet::new(
            Default::default(),
            vec![],
            vec![],
            Default::default(),
        )
        .unwrap(),
        lot,
        wonderland_game_runtime::RuntimeConfig::new(VmMode::Ts1, 11, 7, 1),
        wonderland_game_runtime::RuntimeRole::Replica,
    );
    // Projection data is explicitly a unit-test harness; live browser delivery is tested separately.
    let mut p = rt.unwrap().projection();
    p.tick = tick;
    p.entities.push(wonderland_game_runtime::EntityProjection {
        reference: entity(),
        guid: 42,
        persistent_id: 42,
        position: wonderland_game_runtime::TilePos::new(3, 3, 1).center(),
        facing: wonderland_game_runtime::Facing::NORTH,
        lifecycle: wonderland_game_runtime::sim_core::state::LifecyclePhase::Running,
        raw_motives: None,
        needs: None,
        thread: None,
    });
    p
}
fn sound(tick: u64) -> TickOutcome {
    TickOutcome {
        tick,
        duplicate: false,
        state_hash: [0; 32],
        instructions: 0,
        effects: vec![],
        events: vec![RuntimeEvent::Avatar {
            entity: entity(),
            output: AvatarTickOutput {
                animation_cues: vec![
                    AnimationCue::Dress("not sound".into()),
                    AnimationCue::Sound("fixture".into()),
                ],
                ..Default::default()
            },
        }],
    }
}
fn starts(items: &[MixerIntent]) -> usize {
    items
        .iter()
        .filter(|i| matches!(i, MixerIntent::Start { .. }))
        .count()
}
#[test]
fn accepted_animation_sound_uses_real_hit_mixer_and_no_duplicate_replay() {
    let mut a = NativeAudio::new(pack(), 11, 7, 1).unwrap();
    a.checkpoint(2);
    a.accept(&[sound(3)], &projection(3), true).unwrap();
    let out = a.session.tick().unwrap();
    assert_eq!(starts(&out), 1);
    a.accept(&[sound(3)], &projection(3), true).unwrap();
    assert_eq!(starts(&a.session.tick().unwrap()), 0);
}
#[test]
fn checkpoint_and_disabled_audio_retire_history_without_future_backlog() {
    let mut a = NativeAudio::new(pack(), 11, 7, 1).unwrap();
    a.checkpoint(10);
    a.accept(&[sound(5)], &projection(10), true).unwrap();
    assert_eq!(starts(&a.session.tick().unwrap()), 0);
    a.accept(&[sound(11)], &projection(11), false).unwrap();
    a.accept(&[sound(11)], &projection(11), true).unwrap();
    assert_eq!(starts(&a.session.tick().unwrap()), 0);
    a.accept(&[sound(12)], &projection(12), true).unwrap();
    assert_eq!(starts(&a.session.tick().unwrap()), 1);
}
#[test]
fn recovery_stops_old_voices_and_never_plays_historical_events() {
    let mut a = NativeAudio::new(pack(), 11, 7, 1).unwrap();
    a.accept(&[sound(3)], &projection(3), true).unwrap();
    let first = a.session.tick().unwrap();
    assert_eq!(starts(&first), 1);
    assert!(
        a.checkpoint(4)
            .iter()
            .any(|i| matches!(i, MixerIntent::Stop { .. } | MixerIntent::Release { .. }))
    );
    a.accept(&[sound(3)], &projection(4), true).unwrap();
    assert_eq!(starts(&a.session.tick().unwrap()), 0);
}
#[test]
fn mismatched_timeline_and_future_output_reject_before_start() {
    let mut a = NativeAudio::new(pack(), 11, 7, 1).unwrap();
    let mut p = projection(3);
    p.epoch = 8;
    assert!(a.accept(&[sound(3)], &p, true).is_err());
    assert!(a.accept(&[sound(4)], &projection(3), true).is_err());
    assert_eq!(starts(&a.session.tick().unwrap()), 0);
}
#[test]
fn deleted_or_recycled_owner_cannot_start_a_sound() {
    let mut a = NativeAudio::new(pack(), 11, 7, 1).unwrap();
    let mut p = projection(3);
    p.entities[0].reference.generation = 2;
    a.accept(&[sound(3)], &p, true).unwrap();
    assert_eq!(starts(&a.session.tick().unwrap()), 0);
}
#[test]
fn explicit_manifest_is_optional_unique_and_bounded() {
    use wonderland_web_shell::native_audio::selected_audio_pack;
    assert!(selected_audio_pack(&[]).unwrap().is_none());
    assert!(selected_audio_pack(&[("wonderland-audio.json".into(),br#"{"version":1,"groups":[],"tracks":[],"samples":[],"url":"https://example.invalid/sound"}"#.to_vec())]).is_err());
    assert!(
        selected_audio_pack(&[("wonderland-audio.json".into(), vec![b' '; 128 * 1024 + 1])])
            .is_err()
    );
    assert!(
        selected_audio_pack(&[
            ("a/wonderland-audio.json".into(), b"{}".to_vec()),
            ("b/wonderland-audio.json".into(), b"{}".to_vec())
        ])
        .is_err()
    );
}
fn primitive(tick: u64, opcode: u16) -> TickOutcome {
    use wonderland_game_runtime::sim_core::vm::{ExternalKind, ExternalRequest, FrameContext};
    let mut outcome = sound(tick);
    outcome.events = vec![RuntimeEvent::Presentation(ExternalRequest {
        opcode,
        kind: ExternalKind::Sound,
        context: FrameContext::for_entity(entity(), 42),
        operand: if opcode == 23 {
            [5, 0, 0, 0, 0, 0, 0, 0]
        } else {
            [0; 8]
        },
        parameters: vec![],
        temps: [0; 20],
        temp_xl: [0; 2],
        is_check: false,
        amount: None,
    })];
    outcome
}
#[test]
fn accepted_fwav_primitive_starts_and_stop_owner_releases_without_replaying() {
    let mut p = pack();
    Arc::get_mut(&mut p)
        .unwrap()
        .fwav
        .scoped
        .insert((guid_scope(42), 5), "fixture".into());
    let mut a = NativeAudio::new(p, 11, 7, 1).unwrap();
    a.accept(&[primitive(3, 23)], &projection(3), true).unwrap();
    assert_eq!(starts(&a.session.tick().unwrap()), 1);
    a.accept(&[primitive(4, 48)], &projection(4), true).unwrap();
    let stops = a.session.tick().unwrap();
    assert!(
        stops
            .iter()
            .any(|i| matches!(i, MixerIntent::Stop { .. } | MixerIntent::Release { .. }))
    );
    a.accept(&[primitive(3, 23)], &projection(4), true).unwrap();
    assert_eq!(starts(&a.session.tick().unwrap()), 0);
}
#[test]
fn absent_fwav_and_wrong_generation_do_not_create_guessed_audio() {
    let mut a = NativeAudio::new(pack(), 11, 7, 1).unwrap();
    a.accept(&[primitive(3, 23)], &projection(3), true).unwrap();
    assert_eq!(starts(&a.session.tick().unwrap()), 0);
    assert!(a.notice.is_some());
    let mut p = projection(4);
    p.entities.clear();
    a.accept(&[sound(4)], &p, true).unwrap();
    assert_eq!(starts(&a.session.tick().unwrap()), 0);
}
