//! Explicit local asset bindings; metadata never grants gameplay authority.
use crate::{bind_group, bind_track, prepare_sample, session::AcceptedAudioSession, SourceFwav};
use serde::Deserialize;
use std::{collections::BTreeMap, sync::Arc};
use wonderland_audio_runtime::{
    codec::DecodeLimits,
    cue::CueLedger,
    hit::HitHost,
    mixer::VolumeGroup,
    runtime::{AudioRuntime, TsoGroup},
    system::{AudioContent, AudioSystem},
};
use wonderland_audio_runtime::{
    hit::{HitCatalog, HitLimits},
    pcm::PcmBuffer,
    runtime::EventBank,
};
use wonderland_legacy_formats::{iff, Limits};
use wonderland_render_core::AssetKey;
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioPackSpec {
    pub version: u32,
    pub groups: Vec<GroupSpec>,
    pub tracks: Vec<TrackSpec>,
    pub samples: Vec<SampleSpec>,
    #[serde(default)]
    pub fwav: Vec<FwavSpec>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupSpec {
    pub kind: String,
    pub hit: String,
    pub events: String,
    #[serde(default)]
    pub hsm: Option<String>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackSpec {
    pub instance: u32,
    pub file: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SampleSpec {
    pub id: u32,
    pub file: String,
    pub group: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FwavSpec {
    pub scope: Option<u32>,
    pub file: String,
}
pub struct AudioPack {
    pub samples: BTreeMap<AssetKey, PcmBuffer>,
    pub fwav: SourceFwav,
    bank: EventBank,
    catalog: Arc<HitCatalog>,
    limits: HitLimits,
}
impl AudioPack {
    /// Resolve only explicitly selected files. No URL, filename-based ID inference,
    /// archive expansion, or network request is performed by this importer.
    pub fn load(spec: AudioPackSpec, files: &[(String, Vec<u8>)]) -> Result<Self, String> {
        Self::load_in_folder(spec, files, "")
    }
    /// Borrow a directory-selected cohort without duplicating its encoded bytes.
    pub fn load_in_folder(
        spec: AudioPackSpec,
        files: &[(String, Vec<u8>)],
        folder: &str,
    ) -> Result<Self, String> {
        if !folder.is_empty()
            && (folder.starts_with('/')
                || folder.contains(['\\', ':', '\0'])
                || folder
                    .split('/')
                    .any(|p| p.is_empty() || p == ".." || p == "."))
        {
            return Err("Invalid audio manifest folder".into());
        }
        let prefix = if folder.is_empty() {
            String::new()
        } else {
            format!("{folder}/")
        };
        if spec.version != 1
            || spec.groups.is_empty()
            || spec.groups.len() > 6
            || spec.tracks.len() > 4096
            || spec.samples.len() > 128
            || spec.fwav.len() > 512
            || files.len() > 100_000
        {
            return Err("Audio manifest exceeds supported version or entry limits".into());
        }
        let mut by_name = BTreeMap::new();
        for (name, data) in files {
            let Some(name) = name.strip_prefix(&prefix) else {
                continue;
            };
            if by_name.insert(name, data.as_slice()).is_some() {
                return Err(format!("Ambiguous selected file: {name}"));
            }
        }
        let mut used = std::collections::BTreeSet::new();
        let mut bytes = 0usize;
        let mut get = |name: &str| -> Result<&[u8], String> {
            if name.is_empty()
                || name.len() > 1024
                || name.starts_with('/')
                || name.contains(['\\', ':', '\0'])
                || name
                    .split('/')
                    .any(|p| p.is_empty() || p == ".." || p == ".")
            {
                return Err("Audio files must have exact relative paths".into());
            }
            let value = *by_name
                .get(name)
                .ok_or_else(|| format!("Missing selected audio file: {name}"))?;
            if value.len() > 16 * 1024 * 1024 {
                return Err("Individual audio input exceeds 16 MiB".into());
            }
            if used.insert(name.to_owned()) {
                bytes = bytes
                    .checked_add(value.len())
                    .ok_or("Audio input overflow")?;
            }
            if bytes > 64 * 1024 * 1024 {
                return Err("Audio pack input exceeds 64 MiB".into());
            }
            Ok(value)
        };
        let limits = HitLimits {
            catalog_entries: 16384,
            ..Default::default()
        };
        let source = Limits {
            max_input_bytes: 16 * 1024 * 1024,
            max_resource_bytes: 8 * 1024 * 1024,
            max_total_decoded_bytes: 32 * 1024 * 1024,
            max_entries: 4096,
            max_string_bytes: 4096,
            ..Default::default()
        };
        let mut groups = Vec::new();
        for group in spec.groups {
            let kind = match group.kind.as_str() {
                "new_main" => TsoGroup::NewMain,
                "relationships" => TsoGroup::Relationships,
                "tso_ep5" => TsoGroup::TsoEp5,
                "tso_v2" => TsoGroup::TsoV2,
                "tso_v3" => TsoGroup::TsoV3,
                "turkey" => TsoGroup::Turkey,
                _ => return Err("Unknown source HIT resource group".into()),
            };
            let hit = get(&group.hit)?;
            let events = get(&group.events)?;
            let hsm = group.hsm.as_deref().map(&mut get).transpose()?;
            groups.push(bind_group(kind, hit, events, hsm, &source, &limits)?);
        }
        let bank = EventBank::new(groups, &limits).map_err(|e| e.to_string())?;
        let mut catalog = HitCatalog::default();
        let mut track_ids = std::collections::BTreeSet::new();
        for t in spec.tracks {
            let source_track = bind_track(&mut catalog, t.instance, get(&t.file)?, &source)?;
            if !track_ids.insert(source_track.track_id) {
                return Err("Ambiguous backup track identity".into());
            }
        }
        let mut samples = BTreeMap::new();
        let mut pcm_bytes = 0usize;
        for sample in spec.samples {
            if catalog.samples.contains_key(&sample.id) {
                return Err("Duplicate source sample ID".into());
            }
            let group = match sample.group.as_str() {
                "fx" => VolumeGroup::Fx,
                "music" => VolumeGroup::Music,
                "vox" => VolumeGroup::Vox,
                "ambience" => VolumeGroup::Ambience,
                _ => return Err("Unknown audio volume group".into()),
            };
            let decoded = prepare_sample(
                get(&sample.file)?,
                group,
                &source,
                &DecodeLimits {
                    input_bytes: 16 * 1024 * 1024,
                    pcm_bytes: (32 * 1024 * 1024 - pcm_bytes).min(8 * 1024 * 1024),
                    ..Default::default()
                },
            )?;
            // Charge aggregate browser Float32 residency, not only the retained i16 copy.
            let key = decoded.reference.key;
            if !samples.contains_key(&key) {
                pcm_bytes = pcm_bytes
                    .checked_add(decoded.browser_decoded_bytes as usize)
                    .ok_or("Decoded PCM overflow")?;
            }
            if pcm_bytes > 32 * 1024 * 1024 {
                return Err("Decoded audio pack exceeds 32 MiB".into());
            }
            catalog.samples.insert(sample.id, decoded.reference);
            samples.entry(key).or_insert(decoded.pcm);
        }
        let mut fwav = SourceFwav::default();
        let mut names = 0;
        for resource in spec.fwav {
            let file = iff::decode(get(&resource.file)?, &source).map_err(|e| e.to_string())?;
            for chunk in file.chunks.into_iter().filter(|c| c.key.kind == *b"FWAV") {
                names += 1;
                if names > 4096 {
                    return Err("Audio FWAV entry limit".into());
                }
                // FWAV.Read uses a null-terminated name; trailing chunk bytes are not part
                // of the event and are not interpreted as new metadata.
                let end = chunk
                    .data
                    .iter()
                    .position(|b| *b == 0)
                    .ok_or("Unterminated FWAV event")?;
                if end == 0
                    || end > 256
                    || chunk.data[..end].iter().any(|b| !b.is_ascii() || *b < 32)
                {
                    return Err("Invalid FWAV event name".into());
                }
                let value = String::from_utf8(chunk.data[..end].to_vec())
                    .map_err(|_| "Invalid FWAV text")?;
                let old = match resource.scope {
                    Some(guid) => fwav.scoped.insert((guid_scope(guid), chunk.key.id), value),
                    None => fwav.globals.insert(chunk.key.id, value),
                };
                if old.is_some() {
                    return Err("Ambiguous scoped FWAV identity".into());
                }
            }
        }
        // Validate catalog budgets before handing a bank to any device/runtime.
        let catalog = Arc::new(catalog);
        HitHost::new(catalog.clone(), limits.clone(), 1, 1).map_err(|e| e.to_string())?;
        Ok(Self {
            samples,
            fwav,
            bank,
            catalog,
            limits,
        })
    }
    pub fn session(
        &self,
        lot: u64,
        timeline: u64,
        generation: u64,
    ) -> Result<AcceptedAudioSession, String> {
        let host = HitHost::new(
            self.catalog.clone(),
            self.limits.clone(),
            lot ^ timeline,
            generation,
        )
        .map_err(|e| e.to_string())?;
        let system = AudioSystem::new(
            AudioRuntime::new(host, self.bank.clone()),
            CueLedger::new(lot, timeline, 4096).map_err(|e| e.to_string())?,
            AudioContent::default(),
        )
        .map_err(|e| e.to_string())?;
        Ok(AcceptedAudioSession::new(system))
    }
}

pub fn guid_scope(guid: u32) -> AssetKey {
    let mut b = [0; 32];
    b[..4].copy_from_slice(&guid.to_le_bytes());
    AssetKey(b)
}
