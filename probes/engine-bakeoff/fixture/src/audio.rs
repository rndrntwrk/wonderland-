use crate::{problem, FixtureError};
use wonderland_audio_runtime::{
    cue::{CueAdmission, CueId, CueLedger},
    mixer::{MixerIntent, VoiceId, VolumeGroup},
    pcm::{NativeMixer, PcmBuffer},
};
use wonderland_render_core::{AssetKey, EntityRef};

/// One recorded second, three accepted cosmetic cues, sampled at a requested
/// render cadence. The accepted 30 Hz timeline is supplied, never advanced here.
pub fn audio_reference(render_hz: u32) -> Result<(Vec<i16>, usize), FixtureError> {
    if ![30, 60, 120].contains(&render_hz) {
        return Err(FixtureError("unsupported fixture cadence".into()));
    }
    let sample = AssetKey([73; 32]);
    let mut mixer = NativeMixer::new(48000, 8, 4096).map_err(problem)?;
    mixer
        .insert_sample(
            sample,
            PcmBuffer {
                sample_rate: 48000,
                channels: 1,
                samples: (0..960)
                    .map(|n| if n % 96 < 48 { 12000 } else { -12000 })
                    .collect(),
            },
        )
        .map_err(problem)?;
    let mut ledger = CueLedger::new(42, 1, 16).map_err(problem)?;
    let mut started = 0;
    let mut pcm = Vec::with_capacity(96000);
    for presentation_frame in 0..render_hz {
        let tick = u64::from(presentation_frame * 30 / render_hz);
        if tick % 10 == 0 {
            let cue = CueId {
                lot_id: 42,
                timeline: 1,
                tick,
                event_ordinal: 0,
                nested_ordinal: 0,
                owner: Some(EntityRef {
                    object_id: 100,
                    generation: 1,
                }),
            };
            if ledger.admit(&cue).map_err(problem)? == CueAdmission::New {
                started += 1;
                mixer
                    .apply(&MixerIntent::Start {
                        voice: VoiceId {
                            generation: 1,
                            serial: started as u64,
                        },
                        sample,
                        group: VolumeGroup::Fx,
                        gain: 0.75,
                        pan: 0.,
                        looped: false,
                        seek_frame: 0,
                    })
                    .map_err(problem)?;
            }
        }
        pcm.extend(
            mixer
                .render((48000 / render_hz) as usize)
                .map_err(problem)?,
        );
    }
    Ok((pcm, started))
}
