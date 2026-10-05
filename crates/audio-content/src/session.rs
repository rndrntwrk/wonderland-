use wonderland_audio_runtime::{
    cue::{AudioCue, CueAdmission},
    mixer::{MixerIntent, VoiceId, VolumeGroup},
    system::AudioSystem,
    Result,
};
use wonderland_render_core::EntityRef;
/// The caller supplies accepted causal IDs from the runtime/authority. One call
/// to tick means one 60 Hz presentation tick, independent of the 30 Hz VM.
pub struct AcceptedAudioSession {
    pub system: AudioSystem,
    volumes: [f32; 4],
    muted: bool,
}
impl AcceptedAudioSession {
    pub fn new(system: AudioSystem) -> Self {
        Self {
            system,
            volumes: [1.; 4],
            muted: false,
        }
    }
    pub fn accept(&mut self, cue: &AudioCue) -> Result<CueAdmission> {
        self.system.accept(cue)
    }
    pub fn tick(&mut self) -> Result<Vec<MixerIntent>> {
        self.system.tick()
    }
    pub fn complete_voice(&mut self, voice: VoiceId) -> bool {
        self.system.complete_voice(voice)
    }
    pub fn reconcile_owners(&mut self, owners: &[EntityRef]) -> Result<()> {
        self.system.reconcile_owners(owners)
    }
    pub fn set_volume(&mut self, group: VolumeGroup, gain: f32) -> Result<()> {
        wonderland_audio_runtime::pcm::validate_gain_pan(gain, 0.)?;
        self.volumes[group as usize] = gain;
        self.system
            .set_master(group, if self.muted { 0. } else { gain })
    }
    pub fn mute(&mut self, muted: bool) -> Result<()> {
        self.muted = muted;
        for group in [
            VolumeGroup::Fx,
            VolumeGroup::Music,
            VolumeGroup::Vox,
            VolumeGroup::Ambience,
        ] {
            self.system.set_master(
                group,
                if muted {
                    0.
                } else {
                    self.volumes[group as usize]
                },
            )?;
        }
        Ok(())
    }
    pub fn stop_all(&mut self) -> Vec<MixerIntent> {
        self.system.stop_all()
    }
}
/// Converts browser elapsed time to the engine's fixed 60 Hz audio cadence.
/// Device clocks and simulation ticks are not inputs to this accumulator.
#[derive(Default)]
pub struct AudioCadence {
    elapsed: f64,
}
impl AudioCadence {
    pub fn advance(&mut self, elapsed_ms: f64) -> usize {
        if !elapsed_ms.is_finite() || elapsed_ms < 0. {
            return 0;
        }
        // A background-tab suspension does not replay seconds of presentation work.
        if elapsed_ms > 250. {
            self.elapsed = 0.;
            return 0;
        }
        self.elapsed += elapsed_ms;
        let count = ((self.elapsed + 1e-8) / (1000. / 60.)).floor() as usize;
        self.elapsed = (self.elapsed - count as f64 * (1000. / 60.)).max(0.);
        count
    }
}
