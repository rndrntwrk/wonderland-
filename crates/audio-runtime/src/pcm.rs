//! Bounded native PCM mixer. Output is interleaved stereo PCM16. Resampling is
//! explicit zero-order hold with integer phase; this reference mixer makes no
//! device-clock or simulation-clock claim.
use std::{collections::BTreeMap, sync::Arc};
use wonderland_render_core::AssetKey;
use crate::{AudioError, Result};
use crate::mixer::{MixerIntent, VoiceId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PcmBuffer { pub sample_rate: u32, pub channels: u16, pub samples: Vec<i16> }
impl PcmBuffer {
    pub fn validate(&self, max_bytes: usize) -> Result<()> {
        if self.sample_rate == 0 || self.sample_rate > 384_000 || ![1,2].contains(&self.channels) || self.samples.is_empty() || self.samples.len() % usize::from(self.channels) != 0 {
            return Err(AudioError::Invalid("PCM shape"));
        }
        if self.samples.len().checked_mul(2).ok_or(AudioError::Limit("PCM bytes"))? > max_bytes { return Err(AudioError::Limit("PCM bytes")); }
        Ok(())
    }
    pub fn frames(&self) -> u64 { self.samples.len() as u64 / u64::from(self.channels.max(1)) }
}
#[derive(Debug)]
struct Voice { pcm: Arc<PcmBuffer>, phase: u128, gain: f32, pan: f32, looped: bool, paused: bool }
#[derive(Debug)]
pub struct NativeMixer {
    sample_rate: u32, max_voices: usize, max_pcm_bytes: usize,
    pcm_bytes: usize, samples: BTreeMap<AssetKey, Arc<PcmBuffer>>,
    voices: BTreeMap<VoiceId, Voice>, finished: Vec<VoiceId>, watermark:Option<VoiceId>,
}
impl NativeMixer {
    pub fn new(sample_rate: u32, max_voices: usize, max_pcm_bytes: usize) -> Result<Self> {
        if sample_rate == 0 || sample_rate > 384_000 || max_voices == 0 || max_pcm_bytes < 2 { return Err(AudioError::Invalid("mixer limits")); }
        Ok(Self { sample_rate, max_voices, max_pcm_bytes, pcm_bytes: 0, samples:BTreeMap::new(), voices:BTreeMap::new(), finished:Vec::new(), watermark:None })
    }
    pub fn insert_sample(&mut self, key: AssetKey, pcm: PcmBuffer) -> Result<()> {
        pcm.validate(self.max_pcm_bytes)?;
        if self.samples.contains_key(&key) { return Err(AudioError::Invalid("sample key already resident")); }
        if self.samples.len()>=self.max_voices.saturating_mul(2){return Err(AudioError::Limit("PCM entries"));}
        let bytes=pcm.samples.len()*2;
        if bytes > self.max_pcm_bytes - self.pcm_bytes { return Err(AudioError::Limit("PCM residency")); }
        self.samples.insert(key,Arc::new(pcm)); self.pcm_bytes+=bytes; Ok(())
    }
    pub fn apply(&mut self, intent: &MixerIntent) -> Result<()> {
        match *intent {
            MixerIntent::Start { voice,sample,gain,pan,looped,seek_frame,.. } => {
                validate_gain_pan(gain,pan)?;
                if voice.generation==0||voice.serial==0{return Err(AudioError::Invalid("voice identity"));}
                if self.watermark.map_or(false,|old|voice<=old){return Err(AudioError::Stale);}
                let new_generation=self.watermark.map_or(false,|old|voice.generation>old.generation);
                if self.voices.contains_key(&voice) { return Err(AudioError::Invalid("voice already active")); }
                if !new_generation && self.voices.len()+self.finished.len() >= self.max_voices { return Err(AudioError::Limit("voices")); }
                let pcm=self.samples.get(&sample).ok_or(AudioError::Missing("PCM sample"))?.clone();
                let seek=if looped { seek_frame%pcm.frames() } else { seek_frame };
                if seek >= pcm.frames() { return Err(AudioError::Invalid("seek frame")); }
                if new_generation{self.voices.clear();self.finished.clear();}
                self.watermark=Some(voice);
                self.voices.insert(voice, Voice { pcm, phase:u128::from(seek)*u128::from(self.sample_rate),gain,pan,looped,paused:false });
            }
            MixerIntent::SetGainPan {voice,gain,pan} => { validate_gain_pan(gain,pan)?; if let Some(v)=self.voices.get_mut(&voice) { v.gain=gain; v.pan=pan; } }
            MixerIntent::Pause {voice} => { if let Some(v)=self.voices.get_mut(&voice) { v.paused=true; } }
            MixerIntent::Resume {voice} => { if let Some(v)=self.voices.get_mut(&voice) { v.paused=false; } }
            MixerIntent::Stop {voice} | MixerIntent::Release {voice} => { self.voices.remove(&voice);self.finished.retain(|id|*id!=voice); }
        } Ok(())
    }
    pub fn render(&mut self, frames: usize) -> Result<Vec<i16>> {
        // At most one second per callback prevents accidental unbounded output.
        if frames > self.sample_rate as usize { return Err(AudioError::Limit("render frames")); }
        let mut out=vec![0;frames*2];
        for frame in 0..frames {
            let mut sum=[0f64;2];
            for voice in self.voices.values_mut() {
                if voice.paused { continue; }
                let span=u128::from(voice.pcm.frames())*u128::from(self.sample_rate);
                if voice.phase>=span { if voice.looped { voice.phase%=span; } else { continue; } }
                let pos=(voice.phase/u128::from(self.sample_rate)) as usize;
                let left=voice.pcm.samples[pos*usize::from(voice.pcm.channels)];
                let right=if voice.pcm.channels==2 {voice.pcm.samples[pos*2+1]} else {left};
                let lg=voice.gain*(1.0-voice.pan.max(0.0));
                let rg=voice.gain*(1.0+voice.pan.min(0.0));
                sum[0]+=f64::from(left)*f64::from(lg); sum[1]+=f64::from(right)*f64::from(rg);
                voice.phase+=u128::from(voice.pcm.sample_rate);
            }
            out[frame*2]=sum[0].round().clamp(-32768.0,32767.0) as i16;
            out[frame*2+1]=sum[1].round().clamp(-32768.0,32767.0) as i16;
        }
        let completed:Vec<_>=self.voices.iter().filter(|(_,v)| !v.looped && v.phase>=u128::from(v.pcm.frames())*u128::from(self.sample_rate)).map(|(id,_)|*id).collect();
        for id in completed { self.voices.remove(&id); self.finished.push(id); }
        Ok(out)
    }
    /// Presentation feedback only; no simulation acknowledgement is exposed.
    pub fn take_finished(&mut self) -> Vec<VoiceId> { std::mem::take(&mut self.finished) }
    pub fn active_voices(&self) -> usize { self.voices.len() }
    pub fn resident_bytes(&self)->usize{self.pcm_bytes}
    /// Active voices retain PCM ownership; eviction cannot hide that allocation.
    pub fn evict_sample(&mut self,key:AssetKey)->Result<bool>{
        if let Some(pcm)=self.samples.get(&key){if Arc::strong_count(pcm)>1{return Err(AudioError::Invalid("sample still playing"));}}
        if let Some(pcm)=self.samples.remove(&key){self.pcm_bytes-=pcm.samples.len()*2;Ok(true)}else{Ok(false)}
    }
    /// Clears player/cache resources while retaining the accepted voice watermark.
    pub fn reset(&mut self){self.voices.clear();self.finished.clear();self.samples.clear();self.pcm_bytes=0;}
}
pub fn validate_gain_pan(gain:f32,pan:f32)->Result<()> {
    if !gain.is_finite() || !(0.0..=1.0).contains(&gain) || !pan.is_finite() || !(-1.0..=1.0).contains(&pan) { return Err(AudioError::Invalid("gain or pan")); } Ok(())
}
