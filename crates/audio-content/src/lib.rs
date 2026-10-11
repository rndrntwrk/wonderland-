//! Original audio resource bindings. Metadata readers preserve original source
//! file semantics; the engine import is f6f78be1fef247f2db47e19f56d94054f0c9e88c.
pub mod position;
pub mod session;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};
use wonderland_audio_runtime::{
    codec::{decode_sample, DecodeLimits, Encoding, SampleMetadata},
    hit::{HitCatalog, HitLimits, HitProgram, SampleRef, Track},
    mixer::VolumeGroup,
    pcm::PcmBuffer,
    projection::FwavProvider,
    runtime::{EventRecord, ResourceGroup, TsoGroup},
};
use wonderland_legacy_formats::{
    audio_meta::{self, AudioEncoding, HitlistEncoding},
    Limits,
};
use wonderland_render_core::AssetKey;
pub struct PreparedSample {
    pub reference: SampleRef,
    pub pcm: PcmBuffer,
    pub browser_decoded_bytes: u64,
}
pub fn bind_group(
    kind: TsoGroup,
    hit: &[u8],
    events: &[u8],
    hsm: Option<&[u8]>,
    limits: &Limits,
    hit_limits: &HitLimits,
) -> Result<ResourceGroup, String> {
    let meta = audio_meta::decode_hit_metadata(hit, limits).map_err(|e| e.to_string())?;
    let events = audio_meta::decode_events(events, limits).map_err(|e| e.to_string())?;
    let hsm = hsm
        .map(|b| audio_meta::decode_hsm(b, limits))
        .transpose()
        .map_err(|e| e.to_string())?;
    Ok(ResourceGroup {
        kind,
        program: Arc::new(HitProgram::new(hit.to_vec(), hit_limits).map_err(|e| e.to_string())?),
        entrypoints: meta
            .entrypoints
            .iter()
            .map(|e| (e.track_id, e.address))
            .collect(),
        hsm: hsm.map(|m| m.constants.into_iter().map(|c| (c.name, c.value)).collect()),
        events: events
            .entries
            .into_iter()
            .map(|e| EventRecord {
                name: e.name,
                event_type: e.event_type,
                track_id: e.track_id,
            })
            .collect(),
    })
}
pub fn prepare_sample(
    bytes: &[u8],
    group: VolumeGroup,
    limits: &Limits,
    decode: &DecodeLimits,
) -> Result<PreparedSample, String> {
    let source = audio_meta::decode_audio(bytes, limits).map_err(|e| e.to_string())?;
    let encoding = match source.encoding {
        AudioEncoding::XaSpeech => Encoding::XaSpeech,
        AudioEncoding::XaMusic => Encoding::XaMusic,
        AudioEncoding::Utk => Encoding::Utk,
        AudioEncoding::PcmWave => Encoding::PcmWave,
    };
    let meta = SampleMetadata {
        encoding,
        sample_rate: source.format.sample_rate,
        channels: source.format.channels,
        frames: source.sample_frames,
        bits_per_sample: source.format.bits_per_sample,
        payload_offset: source.payload_offset as u64,
        payload_bytes: source.payload_bytes as u64,
    };
    let pcm = decode_sample(bytes, &meta, decode).map_err(|e| e.to_string())?;
    let browser_decoded_bytes = source
        .sample_frames
        .checked_mul(u64::from(source.format.channels))
        .and_then(|n| n.checked_mul(4))
        .ok_or("browser audio allocation overflow")?;
    Ok(PreparedSample {
        reference: SampleRef {
            key: AssetKey(Sha256::digest(bytes).into()),
            group,
            sample_rate: source.format.sample_rate,
            frames: source.sample_frames,
        },
        pcm,
        browser_decoded_bytes,
    })
}
/// DBPF instance IDs are primary keys; the source track ID is a backup lookup.
/// The ordered DBPF provider decides override precedence before calling here.
pub fn bind_track(
    catalog: &mut HitCatalog,
    instance: u32,
    bytes: &[u8],
    limits: &Limits,
) -> Result<audio_meta::Track, String> {
    let source = audio_meta::decode_track(bytes, limits).map_err(|e| e.to_string())?;
    if catalog.tracks.contains_key(&instance) {
        return Err("duplicate track instance".into());
    }
    catalog.tracks.insert(
        instance,
        Track {
            track_id: source.track_id,
            sound_id: source.sound_id,
            hitlist_id: None,
            looped: source.detail.as_ref().map(|d| d.looped != 0),
        },
    );
    catalog.backups.insert(source.track_id, instance);
    Ok(source)
}
pub fn bind_hitlist(
    catalog: &mut HitCatalog,
    id: u32,
    bytes: &[u8],
    encoding: HitlistEncoding,
    limits: &Limits,
) -> Result<(), String> {
    let source = audio_meta::decode_hitlist(bytes, encoding, limits).map_err(|e| e.to_string())?;
    if catalog.hitlists.contains_key(&id) {
        return Err("duplicate hitlist instance".into());
    }
    catalog.hitlists.insert(id, source.ids);
    Ok(())
}
/// FWAV strings are supplied by original IFF resource resolution. Scope and
/// global fallback stay separate; instance identities are never JS numbers.
#[derive(Default)]
pub struct SourceFwav {
    pub scoped: BTreeMap<(AssetKey, u16), String>,
    pub globals: BTreeMap<u16, String>,
}
impl FwavProvider for SourceFwav {
    fn scoped_event(&self, scope: AssetKey, id: u16) -> Option<String> {
        self.scoped.get(&(scope, id)).cloned()
    }
    fn global_event(&self, id: u16) -> Option<String> {
        self.globals.get(&id).cloned()
    }
}

pub mod pack;
