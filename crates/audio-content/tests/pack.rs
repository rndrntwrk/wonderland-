use wonderland_audio_content::pack::*;
use wonderland_audio_runtime::{cue::*, mixer::MixerIntent, projection::FwavProvider};
fn files() -> Vec<(String, Vec<u8>)> {
    let mut hit = b"HIT!".to_vec();
    hit.extend(1u32.to_le_bytes());
    hit.extend(0u32.to_le_bytes());
    hit.extend(b"TSO!ENTP");
    hit.extend(7u32.to_le_bytes());
    hit.extend(32u32.to_le_bytes());
    hit.extend(b"EENT");
    hit.extend([2, 0, 0x0b, 0x0c]);
    let mut wav = b"RIFF".to_vec();
    wav.extend(40u32.to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(8000u32.to_le_bytes());
    wav.extend(16000u32.to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend(4u32.to_le_bytes());
    wav.extend(12000i16.to_le_bytes());
    wav.extend((-12000i16).to_le_bytes());
    vec![
        ("sound.hit".into(), hit),
        ("sound.evt".into(), b"fixture,1,7,0,0,0,0\r\n".to_vec()),
        ("sound.trk".into(), b"TKDT,1,fixture,8,7,ETKD".to_vec()),
        ("sound.wav".into(), wav),
    ]
}
fn spec() -> AudioPackSpec {
    AudioPackSpec {
        version: 1,
        groups: vec![GroupSpec {
            kind: "new_main".into(),
            hit: "sound.hit".into(),
            events: "sound.evt".into(),
            hsm: None,
        }],
        tracks: vec![TrackSpec {
            instance: 7,
            file: "sound.trk".into(),
        }],
        samples: vec![SampleSpec {
            id: 8,
            file: "sound.wav".into(),
            group: "fx".into(),
        }],
        fwav: vec![],
    }
}
#[test]
fn original_parsers_feed_the_actual_hit_engine_and_pcm() {
    let p = AudioPack::load(spec(), &files()).unwrap();
    assert_eq!(p.samples.len(), 1);
    assert_eq!(p.samples.values().next().unwrap().samples, [12000, -12000]);
    let mut s = p.session(11, 7, 99).unwrap();
    let cue = AudioCue {
        id: CueId {
            lot_id: 11,
            timeline: 7,
            tick: 3,
            event_ordinal: 0,
            nested_ordinal: 0,
            owner: None,
        },
        action: CueAction::Play {
            event: "fixture".into(),
            looped: false,
        },
    };
    assert_eq!(s.accept(&cue).unwrap(), CueAdmission::New);
    assert_eq!(s.accept(&cue).unwrap(), CueAdmission::Duplicate);
    let out = s.tick().unwrap();
    assert_eq!(
        out.iter()
            .filter(|i| matches!(i, MixerIntent::Start { .. }))
            .count(),
        1
    );
}
#[test]
fn duplicate_sample_and_ambiguous_filenames_are_rejected() {
    let mut s = spec();
    s.samples.push(s.samples[0].clone());
    assert!(AudioPack::load(s, &files()).is_err());
    let mut f = files();
    f.push(f[0].clone());
    assert!(AudioPack::load(spec(), &f).is_err());
}
#[test]
fn traversal_missing_unknown_group_and_large_manifest_are_rejected() {
    let mut s = spec();
    s.groups[0].hit = "../sound.hit".into();
    assert!(AudioPack::load(s, &files()).is_err());
    let mut s = spec();
    s.samples[0].file = "missing.wav".into();
    assert!(AudioPack::load(s, &files()).is_err());
    let mut s = spec();
    s.samples[0].group = "invented".into();
    assert!(AudioPack::load(s, &files()).is_err());
    let mut s = spec();
    s.version = 2;
    assert!(AudioPack::load(s, &files()).is_err());
    let mut s = spec();
    s.samples = vec![s.samples[0].clone(); 129];
    assert!(AudioPack::load(s, &files()).is_err());
}
#[test]
fn fwav_scope_and_global_fallback_preserve_original_iff_names() {
    use wonderland_legacy_formats::{iff::*, Limits};
    let mut header = [0; 64];
    let text = b"IFF FILE 2.5:TYPE FOLLOWED BY SIZE JAMIE DOORNBOS & MAXIS 1";
    header[..text.len()].copy_from_slice(text);
    let file = IffFile {
        header,
        chunks: vec![IffChunk {
            key: ChunkKey {
                kind: *b"FWAV",
                id: 5,
            },
            flags: 0,
            label: [0; 64],
            data: b"fixture\0".to_vec(),
        }],
    };
    let bytes = encode(&file, &Limits::default()).unwrap();
    let mut f = files();
    f.push(("sounds.iff".into(), bytes));
    let mut s = spec();
    s.fwav = vec![
        FwavSpec {
            scope: Some(42),
            file: "sounds.iff".into(),
        },
        FwavSpec {
            scope: None,
            file: "sounds.iff".into(),
        },
    ];
    let p = AudioPack::load(s, &f).unwrap();
    assert_eq!(
        p.fwav.scoped_event(guid_scope(42), 5).as_deref(),
        Some("fixture")
    );
    assert!(p.fwav.scoped_event(guid_scope(99), 5).is_none());
    assert_eq!(p.fwav.global_event(5).as_deref(), Some("fixture"));
}
#[test]
fn folder_cohorts_are_borrowed_and_cannot_reach_sibling_files() {
    let mut f = files()
        .into_iter()
        .map(|(name, data)| (format!("cohort/{name}"), data))
        .collect::<Vec<_>>();
    assert_eq!(
        AudioPack::load_in_folder(spec(), &f, "cohort")
            .unwrap()
            .samples
            .len(),
        1
    );
    assert!(AudioPack::load_in_folder(spec(), &f, "other").is_err());
    assert!(AudioPack::load_in_folder(spec(), &f, "../cohort").is_err());
    f.push(("sound.wav".into(), vec![1, 2, 3]));
    assert!(AudioPack::load_in_folder(spec(), &f, "cohort").is_ok());
}
#[test]
fn later_corrupt_sample_and_oversize_pcm_fail_without_partial_pack() {
    let mut s = spec();
    s.samples.push(SampleSpec {
        id: 9,
        file: "broken.wav".into(),
        group: "fx".into(),
    });
    let mut f = files();
    f.push(("broken.wav".into(), vec![0; 44]));
    assert!(AudioPack::load(s, &f).is_err());
    let mut f = files();
    f[3].1.resize(16 * 1024 * 1024 + 1, 0);
    assert!(AudioPack::load(spec(), &f).is_err());
}
#[test]
fn duplicate_groups_are_not_guessed() {
    let mut s = spec();
    s.groups.push(s.groups[0].clone());
    assert!(AudioPack::load(s, &files()).is_err());
}
