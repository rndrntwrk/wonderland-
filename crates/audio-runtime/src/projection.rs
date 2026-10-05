//! Normalized views of A's sound-output seam. No A parser or VM is duplicated.
use wonderland_render_core::{AssetKey, EntityRef};
use crate::{cue::{AudioCue, CueAction, CueId}, AudioError, Result};

#[derive(Clone, Debug)]
pub struct SoundRequest {
    pub opcode: u8,
    pub operand: [u8; 8],
    pub caller: EntityRef,
    pub stack: Option<EntityRef>,
    pub scope: AssetKey,
}

pub trait FwavProvider {
    fn scoped_event(&self, scope: AssetKey, id: u16) -> Option<String>;
    fn global_event(&self, id: u16) -> Option<String>;
}

#[derive(Clone, Debug)]
pub struct ProjectedSound {
    pub cue: AudioCue,
    pub no_pan: bool,
    pub no_zoom: bool,
    pub source_sample_rate: u16,
    /// Preserved source operand metadata; the source primitive ignores this.
    pub source_volume: u8,
}

pub fn project_request(mut id: CueId, request: &SoundRequest, provider: &impl FwavProvider) -> Result<Option<ProjectedSound>> {
    let bytes = &request.operand;
    match request.opcode {
        23 => {
            let fwav = u16::from_le_bytes([bytes[0], bytes[1]]);
            let flags = bytes[4];
            let owner = if flags & 2 != 0 { request.stack } else { Some(request.caller) };
            let Some(owner) = owner else { return Ok(None); };
            validate_owner(owner)?;
            let Some(event) = provider.scoped_event(request.scope, fwav).or_else(|| provider.global_event(fwav)) else { return Ok(None); };
            validate_event(&event)?;
            id.owner = Some(owner);
            Ok(Some(ProjectedSound {
                cue: AudioCue { id, action: CueAction::Play { event, looped: flags & 1 != 0 } },
                no_pan: flags & 8 != 0,
                no_zoom: flags & 4 != 0,
                source_sample_rate: u16::from_le_bytes([bytes[2], bytes[3]]),
                source_volume: bytes[5],
            }))
        }
        48 => {
            // Source checks equality with one, not a bit-mask test.
            let owner = if bytes[0] == 1 { request.stack } else { Some(request.caller) };
            let Some(owner) = owner else { return Ok(None); };
            validate_owner(owner)?;
            id.owner = Some(owner);
            Ok(Some(ProjectedSound {
                cue: AudioCue { id, action: CueAction::StopOwner },
                no_pan: false, no_zoom: false, source_sample_rate: 0, source_volume: 0,
            }))
        }
        _ => Err(AudioError::Unsupported("sound primitive opcode")),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AvatarCueView {
    Other,
    Sound(String),
    Dress(String),
    Undress(String),
}

pub fn project_avatar(position: CueId, properties: &[AvatarCueView]) -> Result<Vec<AudioCue>> {
    if properties.len() > 4096 { return Err(AudioError::Limit("avatar audio properties")); }
    if let Some(owner) = position.owner { validate_owner(owner)?; }
    let mut result = vec![];
    for (index, property) in properties.iter().enumerate() {
        if let AvatarCueView::Sound(event) = property {
            validate_event(event)?;
            let mut id = position.clone();
            id.nested_ordinal = u32::try_from(index).map_err(|_| AudioError::Limit("avatar property ordinal"))?;
            result.push(AudioCue { id, action: CueAction::Play { event: event.clone(), looped: false } });
        }
        // Dress and Undress remain appearance events. Non-audio properties
        // still occupy source ordinals; filtering never renumbers sounds.
    }
    Ok(result)
}

fn validate_owner(owner: EntityRef) -> Result<()> {
    if owner.generation == 0 { return Err(AudioError::Invalid("owner generation")); }
    Ok(())
}
fn validate_event(event: &str) -> Result<()> {
    if event.is_empty() || event.len() > 256 || !event.is_ascii() || event.bytes().any(|b| b < 32) {
        return Err(AudioError::Invalid("sound event name"));
    }
    Ok(())
}
