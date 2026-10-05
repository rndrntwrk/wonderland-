use wonderland_audio_runtime::{cue::*, mixer::*, pcm::*, AudioError};
use wonderland_render_core::{AssetKey, EntityRef};
fn id(tick: u64, nested: u32) -> CueId {
    CueId {
        lot_id: 7,
        timeline: 3,
        tick,
        event_ordinal: 1,
        nested_ordinal: nested,
        owner: Some(EntityRef {
            object_id: 2,
            generation: 1,
        }),
    }
}
fn start(voice: VoiceId, sample: AssetKey, gain: f32, pan: f32, looped: bool) -> MixerIntent {
    MixerIntent::Start {
        voice,
        sample,
        group: VolumeGroup::Fx,
        gain,
        pan,
        looped,
        seek_frame: 0,
    }
}
#[test]
fn retired_causal_cues_cannot_replay_after_eviction_or_delivery_epoch_change() {
    let mut ledger = CueLedger::new(7, 3, 2).unwrap();
    assert_eq!(ledger.admit(&id(1, 0)).unwrap(), CueAdmission::New);
    assert_eq!(ledger.admit(&id(1, 0)).unwrap(), CueAdmission::Duplicate);
    assert_eq!(ledger.admit(&id(1, 1)).unwrap(), CueAdmission::New);
    assert_eq!(
        ledger.admit(&id(2, 0)),
        Err(AudioError::Limit("unretired cues"))
    );
    ledger.retire_through(1).unwrap();
    assert_eq!(ledger.len(), 0);
    assert_eq!(ledger.admit(&id(1, 0)).unwrap(), CueAdmission::Retired);
    assert_eq!(ledger.admit(&id(2, 0)).unwrap(), CueAdmission::New);
    let mut wrong = id(3, 0);
    wrong.timeline = 4;
    assert_eq!(ledger.admit(&wrong), Err(AudioError::Stale));
}
#[test]
fn native_mixing_preserves_order_pan_rate_and_pause_without_advancing_paused_voices() {
    let key = AssetKey([3; 32]);
    let voice = VoiceId {
        generation: 1,
        serial: 1,
    };
    let mut mixer = NativeMixer::new(4, 4, 128).unwrap();
    mixer
        .insert_sample(
            key,
            PcmBuffer {
                sample_rate: 2,
                channels: 1,
                samples: vec![1000, 2000],
            },
        )
        .unwrap();
    mixer.apply(&start(voice, key, 0.5, -1.0, false)).unwrap();
    assert_eq!(mixer.render(2).unwrap(), vec![500, 0, 500, 0]);
    mixer.apply(&MixerIntent::Pause { voice }).unwrap();
    assert_eq!(mixer.render(3).unwrap(), vec![0; 6]);
    mixer.apply(&MixerIntent::Resume { voice }).unwrap();
    assert_eq!(mixer.render(3).unwrap(), vec![1000, 0, 1000, 0, 0, 0]);
    assert_eq!(mixer.active_voices(), 0);
}
#[test]
fn native_bounds_and_voice_reuse_are_explicit() {
    let key = AssetKey([4; 32]);
    let voice = VoiceId {
        generation: 2,
        serial: 1,
    };
    let mut mixer = NativeMixer::new(8, 1, 4).unwrap();
    mixer
        .insert_sample(
            key,
            PcmBuffer {
                sample_rate: 8,
                channels: 2,
                samples: vec![30000, 30000],
            },
        )
        .unwrap();
    mixer.apply(&start(voice, key, 1.0, 0.0, true)).unwrap();
    assert!(mixer.apply(&start(voice, key, 1.0, 0.0, true)).is_err());
    assert!(mixer
        .apply(&start(
            VoiceId {
                generation: 2,
                serial: 2
            },
            key,
            1.0,
            0.0,
            true
        ))
        .is_err());
    assert_eq!(mixer.render(2).unwrap(), vec![30000; 4]);
    mixer.apply(&MixerIntent::Stop { voice }).unwrap();
    mixer.apply(&MixerIntent::Release { voice }).unwrap();
    assert_eq!(mixer.render(2).unwrap(), vec![0; 4]);
    assert!(mixer
        .insert_sample(
            key,
            PcmBuffer {
                sample_rate: 8,
                channels: 1,
                samples: vec![0; 3]
            }
        )
        .is_err());
}

#[test]
fn native_residency_release_and_voice_watermark_survive_reset() {
    let key = AssetKey([9; 32]);
    let voice = VoiceId {
        generation: 1,
        serial: 1,
    };
    let pcm = PcmBuffer {
        sample_rate: 8,
        channels: 1,
        samples: vec![1000],
    };
    let mut mixer = NativeMixer::new(8, 1, 128).unwrap();
    mixer.insert_sample(key, pcm.clone()).unwrap();
    mixer.apply(&start(voice, key, 1.0, 0.0, true)).unwrap();
    assert!(mixer.evict_sample(key).is_err());
    mixer.apply(&MixerIntent::Stop { voice }).unwrap();
    assert!(mixer.apply(&start(voice, key, 1.0, 0.0, true)).is_err());
    mixer.evict_sample(key).unwrap();
    assert_eq!(mixer.resident_bytes(), 0);
    mixer.insert_sample(key, pcm.clone()).unwrap();
    mixer.reset();
    assert_eq!(mixer.resident_bytes(), 0);
    mixer.insert_sample(key, pcm).unwrap();
    assert!(mixer.apply(&start(voice, key, 1.0, 0.0, true)).is_err());
    let second = VoiceId {
        generation: 2,
        serial: 1,
    };
    mixer.apply(&start(second, key, 1.0, 0.0, true)).unwrap();
    mixer.apply(&MixerIntent::Stop { voice }).unwrap();
    assert_eq!(mixer.active_voices(), 1);
    mixer
        .apply(&start(
            VoiceId {
                generation: 3,
                serial: 1,
            },
            key,
            1.0,
            0.0,
            true,
        ))
        .unwrap();
    assert_eq!(mixer.active_voices(), 1);
    mixer.apply(&MixerIntent::Stop { voice: second }).unwrap();
    assert_eq!(mixer.active_voices(), 1);
}
#[test]
fn native_completion_queue_backpressures_starts_instead_of_losing_lifecycle_events() {
    let key = AssetKey([9; 32]);
    let mut mixer = NativeMixer::new(8, 1, 128).unwrap();
    mixer
        .insert_sample(
            key,
            PcmBuffer {
                sample_rate: 8,
                channels: 1,
                samples: vec![1000],
            },
        )
        .unwrap();
    let first = VoiceId {
        generation: 1,
        serial: 1,
    };
    let second = VoiceId {
        generation: 1,
        serial: 2,
    };
    mixer.apply(&start(first, key, 1.0, 0.0, false)).unwrap();
    mixer.render(1).unwrap();
    assert!(mixer.apply(&start(second, key, 1.0, 0.0, false)).is_err());
    assert_eq!(mixer.take_finished(), vec![first]);
    mixer.apply(&start(second, key, 1.0, 0.0, false)).unwrap();
    mixer.render(1).unwrap();
    assert_eq!(mixer.take_finished(), vec![second]);
}
#[test]
fn native_tiny_sample_residency_has_an_entry_budget() {
    let mut mixer = NativeMixer::new(8, 1, 128).unwrap();
    let pcm = PcmBuffer {
        sample_rate: 8,
        channels: 1,
        samples: vec![1],
    };
    mixer.insert_sample(AssetKey([1; 32]), pcm.clone()).unwrap();
    mixer.insert_sample(AssetKey([2; 32]), pcm.clone()).unwrap();
    assert!(mixer.insert_sample(AssetKey([3; 32]), pcm.clone()).is_err());
    assert!(mixer.evict_sample(AssetKey([1; 32])).unwrap());
    mixer.insert_sample(AssetKey([3; 32]), pcm).unwrap();
}

#[test]
fn cue_admission_preflight_does_not_record_until_committed() {
    let mut ledger = CueLedger::new(7, 3, 1).unwrap();
    assert_eq!(ledger.check(&id(1, 0)).unwrap(), CueAdmission::New);
    assert!(ledger.is_empty());
    assert_eq!(ledger.check(&id(1, 0)).unwrap(), CueAdmission::New);
    assert_eq!(ledger.admit(&id(1, 0)).unwrap(), CueAdmission::New);
    assert_eq!(ledger.check(&id(1, 0)).unwrap(), CueAdmission::Duplicate);
    assert!(ledger.check(&id(2, 0)).is_err());
    assert_eq!(ledger.len(), 1);
}
