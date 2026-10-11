use std::{collections::BTreeMap, sync::Arc};
use wonderland_audio_runtime::{
    ambience::*,
    fsc::*,
    hit::*,
    mixer::*,
    station::{self, MusicQueue, StationPlayer},
};
use wonderland_render_core::AssetKey;
fn host() -> HitHost {
    HitHost::new(Arc::new(HitCatalog::default()), HitLimits::default(), 1, 5).unwrap()
}
fn sample(id: u8) -> SampleRef {
    SampleRef {
        key: AssetKey([id; 32]),
        group: VolumeGroup::Fx,
        sample_rate: 60,
        frames: 60,
    }
}
fn row(file: &str, loop_count: u16, random: bool) -> String {
    format!(
        "cell\t512\t{}\t256\t0\t0\t0\t0\t0\t0\t{loop_count}\t0\t0\t0\t-1\t2\t0\t0\t0\t{file}",
        u8::from(random)
    )
}
fn source() -> String {
    format!("FSC1\n# comment\nheader\t512\t0\t0\t0\t3\t1\t60\t4\t0\t0\t-1\t-2\t0\t0\ncells\n{}\n{}\n{}\n{}\n",row("skipped.xa",1,false),row("a.xa",2,false),row("NONE",0,false),row("last.xa",1,false))
}
#[test]
fn ambience_bitfields_preserve_source_guid_order_and_only_one_loop() {
    let entries = wonderland_audio_runtime::ambience::catalog();
    assert_eq!(entries.len(), 39);
    assert_eq!(entries.iter().filter(|e| e.category == 4).count(), 12);
    assert_eq!(
        (entries[0].guid, entries[38].guid),
        (0x3dd887a6, 0xa9b9653e)
    );
    let mut s = AmbienceSelection::default();
    assert_eq!(s.set(0, true).unwrap(), vec![AmbienceChange::Start(0)]);
    s.set(6, true).unwrap();
    assert_eq!(
        s.set(7, true).unwrap(),
        vec![AmbienceChange::Stop(6), AmbienceChange::Start(7)]
    );
    assert_eq!(s.bits, 129);
    assert!(s.set(39, true).is_err());
    assert_eq!(s.bits, 129);
    assert!(s.set(7, true).unwrap().is_empty());
}
#[test]
fn dj_order_compensation_and_base_four_digits_are_validated() {
    assert_eq!(dj_pattern(0, [3, 2, 1]).unwrap(), (11, 57));
    assert_eq!(dj_pattern(1, [0, 1, 2]).unwrap(), (10, 6));
    assert_eq!(dj_pattern(3, [1, 2, 3]).unwrap(), (13, 27));
    assert!(dj_pattern(4, [0; 3]).is_err());
    assert!(dj_pattern(1, [4, 0, 0]).is_err());
}
#[test]
fn fsc_parser_retains_the_skipped_first_row_and_raw_unused_metadata() {
    let f = Fsc::parse(source().as_bytes(), 4096, 8).unwrap();
    assert_eq!(f.notes.len(), 3);
    assert!(f.skipped_first.ends_with("skipped.xa"));
    assert_eq!(f.notes[0].filename, "a.xa");
    assert_eq!((f.notes[0].pitch_left, f.notes[0].pitch_right), (-1, 2));
    assert_eq!((f.header[10], f.header[11]), (0, 0));
    assert!(Fsc::parse(source().as_bytes(), 16, 8).is_err());
    assert!(Fsc::parse(source().replace("\t60\t4", "\t0\t4").as_bytes(), 4096, 8).is_err());
    assert!(Fsc::parse(source().as_bytes(), 4096, 1).is_err());
}
#[test]
fn fsc_strict_beat_spacing_last_row_restart_and_cleanup() {
    let mut h = host();
    let f = Arc::new(Fsc::parse(source().as_bytes(), 4096, 8).unwrap());
    let mut p = FscPlayer::new(f, &mut h).unwrap();
    p.set_volume(0.5).unwrap();
    h.masters[3] = 0.5;
    let mut samples = BTreeMap::new();
    samples.insert("a.xa".into(), sample(1));
    samples.insert("last.xa".into(), sample(2));
    p.tick(1.0, &mut h, &samples).unwrap();
    assert!(h.drain_intents().is_empty());
    p.tick(0.01, &mut h, &samples).unwrap();
    let notes = h.drain_intents();
    assert!(
        matches!(notes.as_slice(),[MixerIntent::Start{gain,pan,group:VolumeGroup::Ambience,..}] if *gain==0.0625 && *pan == -0.5)
    );
    assert_eq!(p.loop_count, 1);
    p.tick(1.0, &mut h, &samples).unwrap();
    p.tick(1.0, &mut h, &samples).unwrap();
    assert!(h.drain_intents().is_empty());
    assert_eq!(p.loop_count, -1);
    p.tick(1.0, &mut h, &samples).unwrap();
    assert!(h.drain_intents().is_empty());
    p.tick(1.0, &mut h, &samples).unwrap();
    let notes = h.drain_intents();
    assert!(matches!(
        notes.as_slice(),
        [MixerIntent::Start {
            sample: AssetKey([1, ..]),
            ..
        }]
    ));
    p.stop(&mut h);
    assert_eq!(
        h.drain_intents()
            .iter()
            .filter(|i| matches!(i, MixerIntent::Release { .. }))
            .count(),
        2
    );
    p.stop(&mut h);
    assert!(h.drain_intents().is_empty());
}
#[test]
fn station_codes_modes_and_commercial_paths_follow_source() {
    assert_eq!(station::catalog().len(), 23);
    assert_eq!(
        station::code_from_track(u32::from_le_bytes(*b"KROC")).unwrap(),
        "KROC"
    );
    assert_eq!(station::mode_station(11).unwrap(), Some("KSEL"));
    assert_eq!(station::mode_station(9).unwrap(), None);
    assert!(station::mode_station(5).is_err());
    assert_eq!(
        station::commercial_directory("sounddata/tvstations/tv_action/").unwrap(),
        "sounddata/tvstations"
    );
}
#[test]
fn station_initial_shuffle_then_fixed_rotation_and_fade_final_silence() {
    let mut h = host();
    let mut p = StationPlayer::new(
        vec![sample(1), sample(2), sample(3)],
        true,
        VolumeGroup::Fx,
        &mut h,
    )
    .unwrap();
    let mut order = vec![];
    for _ in 0..6 {
        p.tick(&mut h).unwrap();
        let out = h.drain_intents();
        order.push(
            out.iter()
                .find_map(|i| {
                    if let MixerIntent::Start { sample, .. } = i {
                        Some(sample.0[0])
                    } else {
                        None
                    }
                })
                .unwrap(),
        );
        p.complete_current();
    }
    assert_eq!(&order[..3], &order[3..]);
    p.fade();
    for _ in 0..120 {
        p.tick(&mut h).unwrap();
        h.drain_intents();
    }
    assert!(!p.dead);
    let mut silent = vec![];
    for _ in 0..59 {
        p.tick(&mut h).unwrap();
        silent.extend(h.drain_intents());
    }
    assert!(!p.dead);
    assert!(silent
        .iter()
        .all(|i| !matches!(i,MixerIntent::SetGainPan{gain,..} if *gain>0.0)));
    p.tick(&mut h).unwrap();
    assert!(p.dead);
    assert!(h
        .drain_intents()
        .iter()
        .any(|i| matches!(i, MixerIntent::Release { .. })));
}
#[test]
fn music_queue_promotes_after_tick_and_latest_pending_replaces_prior() {
    let mut h = host();
    let mut q = MusicQueue::default();
    q.replace(
        StationPlayer::new(vec![sample(1)], true, VolumeGroup::Music, &mut h).unwrap(),
        false,
        &mut h,
    )
    .unwrap();
    q.tick(&mut h).unwrap();
    assert!(h.drain_intents().is_empty());
    q.tick(&mut h).unwrap();
    assert!(h
        .drain_intents()
        .iter()
        .any(|i| matches!(i, MixerIntent::Start { .. })));
    q.replace(
        StationPlayer::new(vec![sample(2)], true, VolumeGroup::Music, &mut h).unwrap(),
        false,
        &mut h,
    )
    .unwrap();
    q.replace(
        StationPlayer::new(vec![sample(3)], true, VolumeGroup::Music, &mut h).unwrap(),
        false,
        &mut h,
    )
    .unwrap();
    for _ in 0..180 {
        q.tick(&mut h).unwrap();
        assert!(!h
            .drain_intents()
            .iter()
            .any(|i| matches!(i, MixerIntent::Start { .. })));
    }
    q.tick(&mut h).unwrap();
    assert!(h.drain_intents().iter().any(|i| matches!(
        i,
        MixerIntent::Start {
            sample: AssetKey([3, ..]),
            ..
        }
    )));
}
