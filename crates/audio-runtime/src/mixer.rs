use serde::{Deserialize, Serialize};
use wonderland_render_core::AssetKey;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct VoiceId {
    pub generation: u64,
    pub serial: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum VolumeGroup {
    Fx = 0,
    Music = 1,
    Vox = 2,
    Ambience = 3,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MixerIntent {
    Start {
        voice: VoiceId,
        sample: AssetKey,
        group: VolumeGroup,
        gain: f32,
        pan: f32,
        looped: bool,
        seek_frame: u64,
    },
    SetGainPan {
        voice: VoiceId,
        gain: f32,
        pan: f32,
    },
    Stop {
        voice: VoiceId,
    },
    Pause {
        voice: VoiceId,
    },
    Resume {
        voice: VoiceId,
    },
    Release {
        voice: VoiceId,
    },
}
