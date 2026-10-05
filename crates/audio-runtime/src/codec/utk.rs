// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Algorithm translated from SimsLib UTKFile2. Original Code: SimsLib.
// Initial developer: Mats 'Afr0' Vederhus. Contributor: Andrew D'Addesio.
// Source baseline: 4c6b3e8f5835b228723caea3c9f683c62f244f73.
// Bounds and explicit bit-exhaustion checks are additions in this port.
use super::DecodeLimits;
use crate::{AudioError, Result};
const COSINE: [f32; 64] = [
    0.0,
    -0.99677598476409912109375,
    -0.99032700061798095703125,
    -0.983879029750823974609375,
    -0.977430999279022216796875,
    -0.970982015132904052734375,
    -0.964533984661102294921875,
    -0.958085000514984130859375,
    -0.9516370296478271484375,
    -0.930754005908966064453125,
    -0.904959976673126220703125,
    -0.879167020320892333984375,
    -0.853372991085052490234375,
    -0.827579021453857421875,
    -0.801786005496978759765625,
    -0.775991976261138916015625,
    -0.75019800662994384765625,
    -0.724404990673065185546875,
    -0.6986110210418701171875,
    -0.6706349849700927734375,
    -0.61904799938201904296875,
    -0.567460000514984130859375,
    -0.515873014926910400390625,
    -0.4642859995365142822265625,
    -0.4126980006694793701171875,
    -0.361110985279083251953125,
    -0.309523999691009521484375,
    -0.257937014102935791015625,
    -0.20634900033473968505859375,
    -0.1547619998455047607421875,
    -0.10317499935626983642578125,
    -0.05158700048923492431640625,
    0.0,
    0.05158700048923492431640625,
    0.10317499935626983642578125,
    0.1547619998455047607421875,
    0.20634900033473968505859375,
    0.257937014102935791015625,
    0.309523999691009521484375,
    0.361110985279083251953125,
    0.4126980006694793701171875,
    0.4642859995365142822265625,
    0.515873014926910400390625,
    0.567460000514984130859375,
    0.61904799938201904296875,
    0.6706349849700927734375,
    0.6986110210418701171875,
    0.724404990673065185546875,
    0.75019800662994384765625,
    0.775991976261138916015625,
    0.801786005496978759765625,
    0.827579021453857421875,
    0.853372991085052490234375,
    0.879167020320892333984375,
    0.904959976673126220703125,
    0.930754005908966064453125,
    0.9516370296478271484375,
    0.958085000514984130859375,
    0.964533984661102294921875,
    0.970982015132904052734375,
    0.977430999279022216796875,
    0.983879029750823974609375,
    0.99032700061798095703125,
    0.99677598476409912109375,
];
const CODEBOOK: [u8; 512] = [
    4, 6, 5, 9, 4, 6, 5, 13, 4, 6, 5, 10, 4, 6, 5, 17, 4, 6, 5, 9, 4, 6, 5, 14, 4, 6, 5, 10, 4, 6,
    5, 21, 4, 6, 5, 9, 4, 6, 5, 13, 4, 6, 5, 10, 4, 6, 5, 18, 4, 6, 5, 9, 4, 6, 5, 14, 4, 6, 5, 10,
    4, 6, 5, 25, 4, 6, 5, 9, 4, 6, 5, 13, 4, 6, 5, 10, 4, 6, 5, 17, 4, 6, 5, 9, 4, 6, 5, 14, 4, 6,
    5, 10, 4, 6, 5, 22, 4, 6, 5, 9, 4, 6, 5, 13, 4, 6, 5, 10, 4, 6, 5, 18, 4, 6, 5, 9, 4, 6, 5, 14,
    4, 6, 5, 10, 4, 6, 5, 0, 4, 6, 5, 9, 4, 6, 5, 13, 4, 6, 5, 10, 4, 6, 5, 17, 4, 6, 5, 9, 4, 6,
    5, 14, 4, 6, 5, 10, 4, 6, 5, 21, 4, 6, 5, 9, 4, 6, 5, 13, 4, 6, 5, 10, 4, 6, 5, 18, 4, 6, 5, 9,
    4, 6, 5, 14, 4, 6, 5, 10, 4, 6, 5, 26, 4, 6, 5, 9, 4, 6, 5, 13, 4, 6, 5, 10, 4, 6, 5, 17, 4, 6,
    5, 9, 4, 6, 5, 14, 4, 6, 5, 10, 4, 6, 5, 22, 4, 6, 5, 9, 4, 6, 5, 13, 4, 6, 5, 10, 4, 6, 5, 18,
    4, 6, 5, 9, 4, 6, 5, 14, 4, 6, 5, 10, 4, 6, 5, 2, 4, 11, 7, 15, 4, 12, 8, 19, 4, 11, 7, 16, 4,
    12, 8, 23, 4, 11, 7, 15, 4, 12, 8, 20, 4, 11, 7, 16, 4, 12, 8, 27, 4, 11, 7, 15, 4, 12, 8, 19,
    4, 11, 7, 16, 4, 12, 8, 24, 4, 11, 7, 15, 4, 12, 8, 20, 4, 11, 7, 16, 4, 12, 8, 1, 4, 11, 7,
    15, 4, 12, 8, 19, 4, 11, 7, 16, 4, 12, 8, 23, 4, 11, 7, 15, 4, 12, 8, 20, 4, 11, 7, 16, 4, 12,
    8, 28, 4, 11, 7, 15, 4, 12, 8, 19, 4, 11, 7, 16, 4, 12, 8, 24, 4, 11, 7, 15, 4, 12, 8, 20, 4,
    11, 7, 16, 4, 12, 8, 3, 4, 11, 7, 15, 4, 12, 8, 19, 4, 11, 7, 16, 4, 12, 8, 23, 4, 11, 7, 15,
    4, 12, 8, 20, 4, 11, 7, 16, 4, 12, 8, 27, 4, 11, 7, 15, 4, 12, 8, 19, 4, 11, 7, 16, 4, 12, 8,
    24, 4, 11, 7, 15, 4, 12, 8, 20, 4, 11, 7, 16, 4, 12, 8, 1, 4, 11, 7, 15, 4, 12, 8, 19, 4, 11,
    7, 16, 4, 12, 8, 23, 4, 11, 7, 15, 4, 12, 8, 20, 4, 11, 7, 16, 4, 12, 8, 28, 4, 11, 7, 15, 4,
    12, 8, 19, 4, 11, 7, 16, 4, 12, 8, 24, 4, 11, 7, 15, 4, 12, 8, 20, 4, 11, 7, 16, 4, 12, 8, 3,
];
const SKIPS: [u8; 29] = [
    8, 7, 8, 7, 2, 2, 2, 3, 3, 4, 4, 3, 3, 5, 5, 4, 4, 6, 6, 5, 5, 7, 7, 6, 6, 8, 8, 7, 7,
];
struct Bits<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Bits<'a> {
    fn read(&mut self, n: usize) -> Result<u8> {
        if n > 8
            || self
                .position
                .checked_add(n)
                .filter(|&p| p <= self.bytes.len() * 8)
                .is_none()
        {
            return Err(AudioError::Truncated((self.position / 8) as u64));
        }
        let value = self.peek8()? & (255u16 >> (8 - n)) as u8;
        self.position += n;
        Ok(value)
    }
    fn peek8(&self) -> Result<u8> {
        if self.position >= self.bytes.len() * 8 {
            return Err(AudioError::Truncated((self.position / 8) as u64));
        }
        let offset = self.position / 8;
        let shift = self.position % 8;
        let word = u16::from(self.bytes[offset])
            | (u16::from(*self.bytes.get(offset + 1).unwrap_or(&0)) << 8);
        Ok((word >> shift) as u8)
    }
}
struct Decoder<'a> {
    bits: Bits<'a>,
    half: bool,
    threshold: u8,
    power: [f32; 64],
    rc: [f32; 12],
    history: [f32; 12],
    frame: [f32; 756],
    unary_limit: usize,
}
pub(super) fn decode(payload: &[u8], frames: usize, limits: &DecodeLimits) -> Result<Vec<i16>> {
    let mut bits = Bits {
        bytes: payload,
        position: 0,
    };
    let half = bits.read(1)? != 0;
    let threshold = 32 - bits.read(4)?;
    let mut power = [0.0; 64];
    power[0] = f32::from(bits.read(4)? + 1) * 8.0;
    let base = 1.04 + f32::from(bits.read(6)?) / 1000.0;
    for i in 1..64 {
        power[i] = power[i - 1] * base;
    }
    let mut decoder = Decoder {
        bits,
        half,
        threshold,
        power,
        rc: [0.0; 12],
        history: [0.0; 12],
        frame: [0.0; 756],
        unary_limit: limits.unary_bits,
    };
    let mut out = Vec::with_capacity(frames);
    while out.len() < frames {
        decoder.decode_frame()?;
        let count = (frames - out.len()).min(432);
        for &value in &decoder.frame[324..324 + count] {
            if !value.is_finite() {
                return Err(AudioError::Fault("nonfinite UTK synthesis"));
            }
            out.push(round_even_pcm(value));
        }
    }
    Ok(out)
}
/// Math.Round(float promoted to double), followed by signed PCM16 saturation.
/// Kept explicit because Rust 1.75 `round` uses ties away from zero.
fn round_even_pcm(value: f32) -> i16 {
    let x = f64::from(value).clamp(-32768.0, 32767.0);
    let floor = x.floor();
    let part = x - floor;
    let rounded = if part < 0.5 {
        floor
    } else if part > 0.5 {
        floor + 1.0
    } else if floor as i64 % 2 == 0 {
        floor
    } else {
        floor + 1.0
    };
    rounded as i16
}
impl Decoder<'_> {
    fn decode_frame(&mut self) -> Result<()> {
        let mut excitation = [0.0; 118];
        let mut delta = [0.0; 12];
        let mut voiced = false;
        for (i, d) in delta.iter_mut().enumerate() {
            let index = usize::from(self.bits.read(if i < 4 { 6 } else { 5 })?);
            if i == 0 && index < usize::from(self.threshold) {
                voiced = true;
            }
            *d = (COSINE[index + if i < 4 { 0 } else { 16 }] - self.rc[i]) / 4.0;
        }
        for sub in 0usize..4 {
            let mut phase = usize::from(self.bits.read(8)?);
            if sub == 0 && phase > 216 {
                phase = 216;
            }
            let pitch = f32::from(self.bits.read(4)?) / 15.0;
            let mut innovation = self.power[usize::from(self.bits.read(6)?)];
            if !self.half {
                self.excitation(5, &mut excitation, voiced, 1)?;
            } else {
                let align = usize::from(self.bits.read(1)?);
                let zero = self.bits.read(1)? != 0;
                let offset = 5 + (1 - align);
                self.excitation(5 + align, &mut excitation, voiced, 2)?;
                for j in (offset..offset + 108).step_by(2) {
                    excitation[j] = if zero {
                        0.0
                    } else {
                        (excitation[j - 1] + excitation[j + 1]) * 0.5973859429
                            - (excitation[j - 3] + excitation[j + 3]) * 0.1145915613
                            + (excitation[j - 5] + excitation[j + 5]) * 0.0180326793
                    };
                }
                if !zero {
                    innovation /= 2.0;
                }
            }
            for j in 0..108 {
                let prior = (108 * sub + j + 216)
                    .checked_sub(phase)
                    .ok_or(AudioError::Fault("UTK pitch index"))?;
                self.frame[324 + 108 * sub + j] =
                    innovation * excitation[5 + j] + pitch * self.frame[prior];
            }
        }
        self.frame.copy_within(432..756, 0);
        for pass in 0..4 {
            for (i, d) in delta.iter().enumerate() {
                self.rc[i] += d;
            }
            self.synthesize(pass * 12, if pass == 3 { 396 } else { 12 });
        }
        Ok(())
    }
    fn excitation(
        &mut self,
        offset: usize,
        exc: &mut [f32; 118],
        voiced: bool,
        interval: usize,
    ) -> Result<()> {
        if voiced {
            let mut table = 0;
            let mut i = offset;
            while i < offset + 108 {
                let code = CODEBOOK[(table << 8) | usize::from(self.bits.peek8()?)];
                table = usize::from(!(2..=8).contains(&code));
                self.bits.read(usize::from(SKIPS[usize::from(code)]))?;
                if code >= 4 {
                    exc[i] = f32::from((code - 1) / 4);
                    if code & 1 != 0 {
                        exc[i] *= -1.0;
                    }
                    i += interval;
                } else if code >= 2 {
                    let count =
                        (usize::from(self.bits.read(6)?) + 7).min((offset + 108 - i) / interval);
                    for _ in 0..count {
                        exc[i] = 0.0;
                        i += interval;
                    }
                } else {
                    exc[i] = 7.0;
                    let mut unary = 0;
                    while self.bits.read(1)? != 0 {
                        unary += 1;
                        if unary > self.unary_limit {
                            return Err(AudioError::Limit("UTK unary code"));
                        }
                        exc[i] += 1.0;
                    }
                    if self.bits.read(1)? == 0 {
                        exc[i] *= -1.0;
                    }
                    i += interval;
                }
            }
        } else {
            for i in (offset..offset + 108).step_by(interval) {
                exc[i] = if self.bits.read(1)? == 0 {
                    0.0
                } else if self.bits.read(1)? == 0 {
                    -2.0
                } else {
                    2.0
                };
            }
        }
        Ok(())
    }
    fn synthesize(&mut self, mut sample: usize, count: usize) {
        let lpc = rc_to_lpc(&self.rc);
        let mut offset: i32 = -1;
        for _ in 0..count {
            for coeff in lpc {
                offset += 1;
                if offset == 12 {
                    offset = 0;
                }
                self.frame[324 + sample] += coeff * self.history[offset as usize];
            }
            self.history[offset as usize] = self.frame[324 + sample];
            offset -= 1;
            sample += 1;
        }
    }
}
fn rc_to_lpc(rc: &[f32; 12]) -> [f32; 12] {
    let mut lpc = [0.0; 12];
    let mut temp = [0.0; 12];
    let mut saved = [0.0; 12];
    temp[0] = 1.0;
    temp[1..].copy_from_slice(&rc[..11]);
    for i in 0..12 {
        for j in (0..12).rev() {
            lpc[i] -= rc[j] * temp[j];
            if j != 11 {
                temp[j + 1] = temp[j] + rc[j] * lpc[i];
            }
        }
        temp[0] = lpc[i];
        saved[i] = lpc[i];
        for j in 0..i {
            lpc[i] -= saved[i - j - 1] * lpc[j];
        }
    }
    lpc
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn halfway_rounding_matches_math_round() {
        assert_eq!(
            [
                round_even_pcm(0.5),
                round_even_pcm(1.5),
                round_even_pcm(2.5),
                round_even_pcm(-0.5),
                round_even_pcm(-1.5),
                round_even_pcm(-2.5)
            ],
            [0, 2, 2, 0, -2, -2]
        );
    }
}
