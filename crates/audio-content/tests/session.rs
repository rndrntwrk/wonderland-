//! Deliberately identified source-syntax fixtures; no fixture is game audio.
use std::sync::Arc;
use wonderland_audio_content::{bind_group, session::AcceptedAudioSession};
use wonderland_audio_runtime::{cue::*, hit::*, mixer::*, runtime::*, system::*};
use wonderland_legacy_formats::Limits;
use wonderland_render_core::{AssetKey, EntityRef};
fn session() -> AcceptedAudioSession {
    let limits = HitLimits::default();
    let mut hit = b"HIT!".to_vec();
    hit.extend(1u32.to_le_bytes());
    hit.extend(0u32.to_le_bytes());
    hit.extend(b"TSO!");
    hit.extend(b"ENTP");
    hit.extend(7u32.to_le_bytes());
    hit.extend(32u32.to_le_bytes());
    hit.extend(b"EENT");
    hit.extend([2, 0, 0x0b, 0x0c]);
    let group = bind_group(
        TsoGroup::NewMain,
        &hit,
        b"fixture,1,7,0,0,0,0\r\n",
        Some(b"fixture 32\n"),
        &Limits::default(),
        &limits,
    )
    .unwrap();
    let mut cat = HitCatalog::default();
    cat.samples.insert(
        8,
        SampleRef {
            key: AssetKey([1; 32]),
            group: VolumeGroup::Fx,
            sample_rate: 8000,
            frames: 8000,
        },
    );
    cat.tracks.insert(
        7,
        Track {
            track_id: 7,
            sound_id: 8,
            hitlist_id: None,
            looped: None,
        },
    );
    let runtime = AudioRuntime::new(
        HitHost::new(Arc::new(cat), limits.clone(), 1, 1).unwrap(),
        EventBank::new(vec![group], &limits).unwrap(),
    );
    AcceptedAudioSession::new(
        AudioSystem::new(
            runtime,
            CueLedger::new(1, 2, 128).unwrap(),
            AudioContent::default(),
        )
        .unwrap(),
    )
}
fn cue() -> AudioCue {
    AudioCue {
        id: CueId {
            lot_id: 1,
            timeline: 2,
            tick: 42,
            event_ordinal: 9,
            nested_ordinal: 0,
            owner: Some(EntityRef {
                object_id: 1,
                generation: 5,
            }),
        },
        action: CueAction::Play {
            event: "fixture".into(),
            looped: false,
        },
    }
}
#[test]
fn duplicate_cue_does_not_replay_and_presentation_completion_is_separate() {
    let mut s = session();
    let c = cue();
    assert_eq!(s.accept(&c).unwrap(), CueAdmission::New);
    assert_eq!(s.accept(&c).unwrap(), CueAdmission::Duplicate);
    let intents = s.tick().unwrap();
    assert_eq!(
        intents
            .iter()
            .filter(|i| matches!(i, MixerIntent::Start { .. }))
            .count(),
        1
    );
    let voice = intents
        .iter()
        .find_map(|i| {
            if let MixerIntent::Start { voice, .. } = i {
                Some(*voice)
            } else {
                None
            }
        })
        .unwrap();
    assert!(s.complete_voice(voice));
    assert_eq!(s.accept(&c).unwrap(), CueAdmission::Duplicate);
    assert!(!s
        .tick()
        .unwrap()
        .iter()
        .any(|i| matches!(i, MixerIntent::Start { .. })));
}
#[test]
fn mute_restores_stored_per_group_volume() {
    let mut s = session();
    s.set_volume(VolumeGroup::Music, 0.3).unwrap();
    s.mute(true).unwrap();
    assert_eq!(s.system.runtime.host.masters, [0.; 4]);
    s.mute(false).unwrap();
    assert_eq!(s.system.runtime.host.masters, [1., 0.3, 1., 1.]);
}
#[test]
fn source_missing_event_is_rejected_without_admitting_causal_id() {
    let mut s = session();
    let mut c = cue();
    c.action = CueAction::Play {
        event: "absent_source_event".into(),
        looped: false,
    };
    assert!(s.accept(&c).is_err());
    assert_eq!(s.system.ledger.len(), 0);
}
#[test]
fn presentation_cadence_is_sixty_hertz_independent_of_vm_ticks() {
    let mut clock = wonderland_audio_content::session::AudioCadence::default();
    assert_eq!(clock.advance(1000. / 30.), 2);
    assert_eq!(clock.advance(1000. / 60.), 1);
    assert_eq!(clock.advance(0.), 0);
    assert_eq!(clock.advance(f64::NAN), 0);
    assert_eq!(clock.advance(-1.), 0);
    assert_eq!(clock.advance(30000.), 0); // suspended tab drops presentation backlog
    assert_eq!(clock.advance(1000. / 60.), 1);
}
