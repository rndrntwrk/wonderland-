//! Byte fixtures reproduce original file syntax; they are not original game assets.
use wonderland_audio_content::*;
use wonderland_audio_runtime::{hit::HitLimits, mixer::VolumeGroup, runtime::TsoGroup};
use wonderland_legacy_formats::Limits;
#[test]
fn source_hit_binding_retains_entire_program_and_absolute_pc() {
    let mut hit = b"HIT!".to_vec();
    hit.extend(1u32.to_le_bytes());
    hit.extend(0u32.to_le_bytes());
    hit.extend(b"TSO!");
    hit.extend(b"ENTP");
    hit.extend(7u32.to_le_bytes());
    hit.extend(32u32.to_le_bytes());
    hit.extend(b"EENT");
    hit.push(0x0c);
    let group = bind_group(
        TsoGroup::NewMain,
        &hit,
        b"fixture,1,7,0,0,0,0\r\n",
        Some(b"fixture 32\n"),
        &Limits::default(),
        &HitLimits::default(),
    )
    .unwrap();
    assert_eq!(group.program.bytes, hit);
    assert_eq!(group.entrypoints, vec![(7, 32)]);
    assert_eq!(group.events[0].track_id, 7);
    assert_eq!(group.hsm.unwrap()[0], ("fixture".into(), 32));
}
#[test]
fn source_wave_metadata_decodes_pcm_and_reports_browser_float_allocation() {
    let mut b = b"RIFF".to_vec();
    b.extend(40u32.to_le_bytes());
    b.extend(b"WAVEfmt ");
    b.extend(16u32.to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(8000u32.to_le_bytes());
    b.extend(16000u32.to_le_bytes());
    b.extend(2u16.to_le_bytes());
    b.extend(16u16.to_le_bytes());
    b.extend(b"data");
    b.extend(4u32.to_le_bytes());
    b.extend(12i16.to_le_bytes());
    b.extend((-12i16).to_le_bytes());
    let sample = prepare_sample(
        &b,
        VolumeGroup::Vox,
        &Limits::default(),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(sample.browser_decoded_bytes, 8);
    assert_eq!(sample.pcm.samples, vec![12, -12]);
    assert_eq!(sample.reference.frames, 2);
    assert_eq!(sample.reference.group, VolumeGroup::Vox);
}
