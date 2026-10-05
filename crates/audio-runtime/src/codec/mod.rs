//! Decodes bounded sample payloads after B (or another declared provider) has
//! supplied normalized metadata. No duplicate HIT/audio header parser lives here.
use crate::{pcm::PcmBuffer, AudioError, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Encoding {
    XaSpeech,
    XaMusic,
    Utk,
    PcmWave,
    Mp3,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SampleMetadata {
    pub encoding: Encoding,
    pub sample_rate: u32,
    pub channels: u16,
    pub frames: u64,
    pub bits_per_sample: u16,
    pub payload_offset: u64,
    pub payload_bytes: u64,
}
#[derive(Clone, Debug)]
pub struct DecodeLimits {
    pub input_bytes: usize,
    pub pcm_bytes: usize,
    pub frames: u64,
    pub unary_bits: usize,
}
impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            input_bytes: 32 * 1024 * 1024,
            pcm_bytes: 64 * 1024 * 1024,
            frames: 16_000_000,
            unary_bits: 4096,
        }
    }
}
mod utk;
pub fn decode_sample(
    bytes: &[u8],
    meta: &SampleMetadata,
    limits: &DecodeLimits,
) -> Result<PcmBuffer> {
    if meta.encoding == Encoding::Mp3 {
        return Err(AudioError::Unsupported("MP3 requires external codec"));
    }
    if bytes.len() > limits.input_bytes {
        return Err(AudioError::Limit("encoded audio bytes"));
    }
    if meta.sample_rate == 0
        || meta.sample_rate > 384_000
        || ![1, 2].contains(&meta.channels)
        || meta.frames == 0
    {
        return Err(AudioError::Invalid("decoded audio shape"));
    }
    if meta.frames > limits.frames {
        return Err(AudioError::Limit("decoded audio frames"));
    }
    let count = meta
        .frames
        .checked_mul(u64::from(meta.channels))
        .and_then(|n| usize::try_from(n).ok())
        .ok_or(AudioError::Limit("decoded sample count"))?;
    if count
        .checked_mul(2)
        .filter(|&b| b <= limits.pcm_bytes)
        .is_none()
    {
        return Err(AudioError::Limit("decoded PCM bytes"));
    }
    let offset =
        usize::try_from(meta.payload_offset).map_err(|_| AudioError::Invalid("payload offset"))?;
    let length =
        usize::try_from(meta.payload_bytes).map_err(|_| AudioError::Invalid("payload length"))?;
    let end = offset
        .checked_add(length)
        .ok_or(AudioError::Invalid("payload span"))?;
    let payload = bytes
        .get(offset..end)
        .ok_or(AudioError::Truncated(meta.payload_offset))?;
    let samples = match meta.encoding {
        Encoding::XaSpeech | Encoding::XaMusic => {
            if meta.bits_per_sample != 16 {
                return Err(AudioError::Unsupported("XA output width"));
            }
            decode_xa(payload, meta.channels, count)?
        }
        Encoding::Utk => {
            if meta.channels != 1 || meta.bits_per_sample != 16 {
                return Err(AudioError::Unsupported("UTK output must be mono PCM16"));
            }
            utk::decode(payload, count, limits)?
        }
        Encoding::PcmWave => decode_pcm(payload, meta.bits_per_sample, count)?,
        Encoding::Mp3 => unreachable!(),
    };
    Ok(PcmBuffer {
        sample_rate: meta.sample_rate,
        channels: meta.channels,
        samples,
    })
}
// MPL-2.0 translation of SimsLib XAFile, Mats 'Afr0' Vederhus and
// Andrew D'Addesio. Full notices and source hashes accompany this crate.
fn decode_xa(payload: &[u8], channels: u16, count: usize) -> Result<Vec<i16>> {
    const EA: [i32; 20] = [
        0, 240, 460, 392, 0, 0, -208, -220, 0, 1, 3, 4, 7, 8, 10, 11, 0, -1, -3, -4,
    ];
    let channels = usize::from(channels);
    let block = 15 * channels;
    if payload.len() % block != 0 {
        return Err(AudioError::Truncated(payload.len() as u64));
    }
    if payload.len() / block * 28 * channels != count {
        return Err(AudioError::Invalid(
            "XA capacity differs from declared frames",
        ));
    }
    let mut history = [[0i32; 2]; 2];
    let mut out = Vec::with_capacity(count);
    for chunk in payload.chunks_exact(block) {
        for pair in 0..14 {
            for high in [true, false] {
                for channel in 0..channels {
                    let header = chunk[channel];
                    let code = chunk[channels + pair * channels + channel];
                    let nibble = if high { code >> 4 } else { code & 15 };
                    let predictor = usize::from(header >> 4);
                    let shift = u32::from(header & 15) + 8;
                    let residual = (i32::from(nibble) << 28) >> shift;
                    let sample = (residual
                        + history[channel][0] * EA[predictor]
                        + history[channel][1] * EA[predictor + 4]
                        + 128)
                        >> 8;
                    let sample = sample.clamp(-32768, 32767);
                    history[channel][1] = history[channel][0];
                    history[channel][0] = sample;
                    out.push(sample as i16);
                }
            }
        }
    }
    Ok(out)
}
fn decode_pcm(bytes: &[u8], bits: u16, count: usize) -> Result<Vec<i16>> {
    if ![8, 16, 24, 32].contains(&bits) {
        return Err(AudioError::Unsupported("PCM integer sample width"));
    }
    let width = usize::from(bits / 8);
    if count.checked_mul(width) != Some(bytes.len()) {
        return Err(AudioError::Invalid("PCM payload frame count"));
    }
    let mut out = Vec::with_capacity(count);
    for sample in bytes.chunks_exact(width) {
        out.push(match bits {
            8 => (i16::from(sample[0]) - 128) << 8,
            16 => i16::from_le_bytes([sample[0], sample[1]]),
            24 => (i32::from_le_bytes([0, sample[0], sample[1], sample[2]]) >> 16) as i16,
            32 => (i32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]]) >> 16) as i16,
            _ => unreachable!(),
        });
    }
    Ok(out)
}
pub fn encode_wave(pcm: &PcmBuffer) -> Result<Vec<u8>> {
    pcm.validate(64 * 1024 * 1024)?;
    let data =
        u32::try_from(pcm.samples.len() * 2).map_err(|_| AudioError::Limit("WAVE output"))?;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend(b"RIFF");
    out.extend((36 + data).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(pcm.channels.to_le_bytes());
    out.extend(pcm.sample_rate.to_le_bytes());
    out.extend((pcm.sample_rate * u32::from(pcm.channels) * 2).to_le_bytes());
    out.extend((pcm.channels * 2).to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend(data.to_le_bytes());
    for sample in &pcm.samples {
        out.extend(sample.to_le_bytes());
    }
    Ok(out)
}
