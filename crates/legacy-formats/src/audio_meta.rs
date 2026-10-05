//! Audio headers and HIT resource metadata. No decoder, audio device, script
//! execution, or file access is performed by these byte-slice readers.
//!
//! Source anchors at FreeSO `4c6b3e8f5835b228723caea3c9f683c62f244f73`:
//! `tso.files/{XA/XAFile,UTK/UTKFile2,HIT/{Track,Hitlist,HSM,EVT,HITFile}}.cs`.
//! WAVE fields also match the exact RIFF headers emitted by XAFile/UTKFile2.

use crate::vitaboy::{invalid, unsupported, DecodeBudget};
use crate::{reader::Reader, Error, ErrorKind, Limits, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioEncoding {
    XaSpeech,
    XaMusic,
    Utk,
    PcmWave,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaveFormat {
    pub format_tag: u16,
    pub channels: u16,
    pub sample_rate: u32,
    pub average_bytes_per_second: u32,
    pub block_align: u16,
    pub bits_per_sample: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioSample {
    pub encoding: AudioEncoding,
    pub format: WaveFormat,
    pub decoded_bytes: u32,
    pub sample_frames: u64,
    pub payload_offset: usize,
    pub payload_bytes: usize,
    /// XA has no WAVEFORMATEX length; UTM0 stores a 32-bit value of 20.
    pub wave_format_size: Option<u32>,
    pub append_size: Option<u32>,
}

fn read_format(r: &mut Reader<'_>) -> Result<WaveFormat> {
    Ok(WaveFormat {
        format_tag: r.u16_le()?,
        channels: r.u16_le()?,
        sample_rate: r.u32_le()?,
        average_bytes_per_second: r.u32_le()?,
        block_align: r.u16_le()?,
        bits_per_sample: r.u16_le()?,
    })
}

fn validate_format(format: WaveFormat, decoded_bytes: u32, limits: &Limits) -> Result<u64> {
    if format.format_tag != 1 {
        return Err(unsupported(0, "only PCM output metadata is supported"));
    }
    if format.channels == 0
        || format.sample_rate == 0
        || format.bits_per_sample == 0
        || !format.bits_per_sample.is_multiple_of(8)
    {
        return Err(invalid(
            0,
            "invalid PCM channels, sample rate, or sample width",
        ));
    }
    let alignment = u32::from(format.channels)
        .checked_mul(u32::from(format.bits_per_sample) / 8)
        .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "PCM alignment"))?;
    if alignment == 0 || alignment != u32::from(format.block_align) {
        return Err(invalid(0, "PCM block alignment mismatch"));
    }
    let rate = format
        .sample_rate
        .checked_mul(alignment)
        .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "PCM byte rate"))?;
    if rate != format.average_bytes_per_second {
        return Err(invalid(0, "PCM byte rate mismatch"));
    }
    if !decoded_bytes.is_multiple_of(alignment) {
        return Err(invalid(0, "PCM byte count ends within a sample frame"));
    }
    limits.check_count(
        decoded_bytes as usize,
        limits.max_total_decoded_bytes,
        0,
        "declared decoded audio bytes",
    )?;
    Ok(u64::from(decoded_bytes / alignment))
}

pub fn decode_audio(bytes: &[u8], limits: &Limits) -> Result<AudioSample> {
    limits.check_input(bytes)?;
    match bytes.get(..4) {
        Some(b"XAI\0") | Some(b"XAJ\0") => decode_xa(bytes, limits),
        Some(b"UTM0") => decode_utk(bytes, limits),
        Some(b"RIFF") => decode_wave(bytes, limits),
        Some(_) => Err(unsupported(
            0,
            "unsupported audio encoding (MP3 and other codecs are opaque)",
        )),
        None => Err(Error::new(ErrorKind::Truncated, 0, "audio magic")),
    }
}

/// XA metadata only. The block-size check follows the source mono 15-byte and
/// stereo 30-byte decoder loops; decoded sample contents are not validated.
pub fn decode_xa(bytes: &[u8], limits: &Limits) -> Result<AudioSample> {
    DecodeBudget::new(bytes, limits)?;
    let mut r = Reader::new(bytes);
    let encoding = match r.read_bytes(4)? {
        b"XAI\0" => AudioEncoding::XaSpeech,
        b"XAJ\0" => AudioEncoding::XaMusic,
        _ => return Err(Error::new(ErrorKind::InvalidMagic, 0, "XA magic")),
    };
    let decoded_bytes = r.u32_le()?;
    let format = read_format(&mut r)?;
    if ![1, 2].contains(&format.channels) || format.bits_per_sample != 16 {
        return Err(unsupported(
            8,
            "XA requires mono or stereo 16-bit PCM output",
        ));
    }
    let sample_frames = validate_format(format, decoded_bytes, limits)?;
    let block = usize::from(format.channels) * 15;
    if !r.remaining().is_multiple_of(block) {
        return Err(Error::new(
            ErrorKind::Truncated,
            r.position(),
            "incomplete XA compressed block",
        ));
    }
    let decoded_capacity = (r.remaining() / block)
        .checked_mul(28)
        .and_then(|n| n.checked_mul(format.block_align as usize))
        .ok_or_else(|| Error::new(ErrorKind::Overflow, r.position(), "XA decoded capacity"))?;
    if decoded_capacity != decoded_bytes as usize {
        return Err(invalid(
            4,
            "XA declared size differs from encoded block capacity",
        ));
    }
    Ok(AudioSample {
        encoding,
        format,
        decoded_bytes,
        sample_frames,
        payload_offset: r.position(),
        payload_bytes: r.remaining(),
        wave_format_size: None,
        append_size: None,
    })
}

/// Reads the fixed UTM0 header and bounds the advertised decoded allocation.
/// The variable-bit-rate payload requires a separate UTK decoder to validate.
pub fn decode_utk(bytes: &[u8], limits: &Limits) -> Result<AudioSample> {
    DecodeBudget::new(bytes, limits)?;
    let mut r = Reader::new(bytes);
    if r.read_bytes(4)? != b"UTM0" {
        return Err(Error::new(ErrorKind::InvalidMagic, 0, "UTM0 magic"));
    }
    let decoded_bytes = r.u32_le()?;
    let wave_format_size = r.u32_le()?;
    if wave_format_size != 20 {
        return Err(unsupported(8, "unsupported UTM0 WAVEFORMATEX size"));
    }
    let format = read_format(&mut r)?;
    let append_size = r.u32_le()?;
    if append_size != 0 {
        return Err(unsupported(28, "UTM0 format extensions are unsupported"));
    }
    if format.channels != 1 || format.bits_per_sample != 16 {
        return Err(unsupported(
            12,
            "UTK source decoder emits mono 16-bit samples",
        ));
    }
    let sample_frames = validate_format(format, decoded_bytes, limits)?;
    // ReadHeader consumes 1+4+4+6 bits to initialize the source decoder.
    if r.remaining() < 2 {
        return Err(Error::new(
            ErrorKind::Truncated,
            r.position(),
            "UTK initialization bits",
        ));
    }
    Ok(AudioSample {
        encoding: AudioEncoding::Utk,
        format,
        decoded_bytes,
        sample_frames,
        payload_offset: r.position(),
        payload_bytes: r.remaining(),
        wave_format_size: Some(wave_format_size),
        append_size: Some(append_size),
    })
}

/// Standard little-endian RIFF/WAVE PCM metadata; arbitrary RIFF chunks are
/// bounded and skipped. Other WAVE codecs and format extensions fail explicitly.
pub fn decode_wave(bytes: &[u8], limits: &Limits) -> Result<AudioSample> {
    DecodeBudget::new(bytes, limits)?;
    let mut r = Reader::new(bytes);
    if r.read_bytes(4)? != b"RIFF" {
        return Err(Error::new(ErrorKind::InvalidMagic, 0, "RIFF magic"));
    }
    let size = r.u32_le()? as usize;
    if size.checked_add(8) != Some(bytes.len()) {
        return Err(invalid(4, "RIFF length does not match input"));
    }
    if r.read_bytes(4)? != b"WAVE" {
        return Err(Error::new(ErrorKind::InvalidMagic, 8, "WAVE magic"));
    }
    let mut format = None;
    let mut data = None;
    let mut chunks = 0usize;
    while r.remaining() != 0 {
        chunks += 1;
        limits.check_count(chunks, limits.max_entries, r.position(), "WAVE chunks")?;
        let kind = r.read_bytes(4)?;
        let length = r.u32_le()? as usize;
        let offset = r.position();
        let body = r.read_bytes(length)?;
        match kind {
            b"fmt " => {
                if format.is_some() {
                    return Err(Error::new(
                        ErrorKind::Duplicate,
                        offset,
                        "duplicate WAVE fmt chunk",
                    ));
                }
                if length != 16 {
                    return Err(unsupported(
                        offset,
                        "extended WAVE fmt chunks are not supported",
                    ));
                }
                format = Some(read_format(&mut Reader::new(body))?);
            }
            b"data" => {
                if data.replace((offset, length)).is_some() {
                    return Err(Error::new(
                        ErrorKind::Duplicate,
                        offset,
                        "duplicate WAVE data chunk",
                    ));
                }
            }
            _ => (),
        }
        if !length.is_multiple_of(2) {
            r.u8()?;
        }
    }
    let format = format.ok_or_else(|| invalid(12, "missing WAVE fmt chunk"))?;
    let (payload_offset, payload_bytes) =
        data.ok_or_else(|| invalid(12, "missing WAVE data chunk"))?;
    let decoded_bytes = u32::try_from(payload_bytes)
        .map_err(|_| Error::new(ErrorKind::Overflow, payload_offset, "WAVE data length"))?;
    let sample_frames = validate_format(format, decoded_bytes, limits)?;
    Ok(AudioSample {
        encoding: AudioEncoding::PcmWave,
        format,
        decoded_bytes,
        sample_frames,
        payload_offset,
        payload_bytes,
        wave_format_size: Some(16),
        append_size: None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HitlistEncoding {
    VersionedBinary,
    CountedBinary,
    PascalRanges,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hitlist {
    pub encoding: HitlistEncoding,
    pub ids: Vec<u32>,
    pub range_text: Option<String>,
}

/// Explicit encoding avoids the source's exception-driven ambiguous fallback.
/// Every listed ID and duplicate survives in the original order.
pub fn decode_hitlist(bytes: &[u8], encoding: HitlistEncoding, limits: &Limits) -> Result<Hitlist> {
    let mut budget = DecodeBudget::new(bytes, limits)?;
    let mut r = Reader::new(bytes);
    let first = r.u32_le()?;
    let (ids, range_text) = match encoding {
        HitlistEncoding::VersionedBinary | HitlistEncoding::CountedBinary => {
            let n = if encoding == HitlistEncoding::VersionedBinary {
                if first != 1 {
                    return Err(unsupported(0, "unsupported binary hitlist version"));
                }
                r.u32_le()? as usize
            } else {
                first as usize
            };
            budget.reserve::<u32>(n, limits.max_entries, 0, "hitlist IDs")?;
            if n.checked_mul(4) != Some(r.remaining()) {
                return Err(invalid(
                    r.position(),
                    "binary hitlist count does not match input",
                ));
            }
            let mut ids = Vec::with_capacity(n);
            for _ in 0..n {
                ids.push(r.u32_le()?);
            }
            (ids, None)
        }
        HitlistEncoding::PascalRanges => {
            budget.reserve::<u8>(
                first as usize,
                limits.max_string_bytes,
                0,
                "hitlist range string",
            )?;
            let text = r.read_bytes(first as usize)?;
            if !text.is_ascii() {
                return Err(unsupported(4, "non-ASCII hitlist range text"));
            }
            let text = std::str::from_utf8(text).expect("ASCII");
            if r.remaining() == 1 && r.u8()? != b'\n' {
                return Err(invalid(r.position() - 1, "unexpected hitlist suffix"));
            }
            let body = text.trim_end_matches(['\r', '\n']);
            let mut ids = Vec::new();
            if !body.is_empty() {
                for entry in body.split(',') {
                    let (first, last) = if let Some((first, last)) = entry.split_once('-') {
                        (decimal(first, 4)?, decimal(last, 4)?)
                    } else {
                        let value = decimal(entry, 4)?;
                        (value, value)
                    };
                    if last < first {
                        return Err(invalid(4, "descending hitlist range"));
                    }
                    let count = u64::from(last) - u64::from(first) + 1;
                    let count = usize::try_from(count)
                        .map_err(|_| Error::new(ErrorKind::Overflow, 4, "hitlist range count"))?;
                    let total = ids
                        .len()
                        .checked_add(count)
                        .ok_or_else(|| Error::new(ErrorKind::Overflow, 4, "hitlist expansion"))?;
                    limits.check_count(total, limits.max_entries, 4, "expanded hitlist IDs")?;
                    budget.reserve::<u32>(count, limits.max_entries, 4, "hitlist IDs")?;
                    ids.reserve_exact(count);
                    // Iterating a count avoids uint wrap at a final 0xffffffff.
                    for offset in 0..count {
                        ids.push((u64::from(first) + offset as u64) as u32);
                    }
                }
            }
            (ids, Some(text.to_owned()))
        }
    };
    if r.remaining() != 0 {
        return Err(unsupported(r.position(), "trailing hitlist data"));
    }
    Ok(Hitlist {
        encoding,
        ids,
        range_text,
    })
}

fn decimal(text: &str, at: usize) -> Result<u32> {
    text.trim()
        .parse()
        .map_err(|_| invalid(at, "invalid decimal audio metadata value"))
}

fn source_number(text: &str, lowercase: bool, at: usize) -> Result<u32> {
    let lower;
    let text = if lowercase {
        lower = text.to_ascii_lowercase();
        lower.as_str()
    } else {
        text
    };
    if text.is_empty() {
        return Ok(0);
    }
    if let Some(text) = text.strip_prefix("0x") {
        return u32::from_str_radix(text.trim(), 16)
            .map_err(|_| invalid(at, "invalid hexadecimal audio metadata value"));
    }
    if text.bytes().any(|b| (b'a'..=b'f').contains(&b)) {
        u32::from_str_radix(text.trim(), 16)
            .map_err(|_| invalid(at, "invalid hexadecimal audio metadata value"))
    } else {
        decimal(text, at)
    }
}

fn fields(
    text: &str,
    separator: char,
    budget: &mut DecodeBudget<'_>,
    at: usize,
) -> Result<Vec<String>> {
    let n = text.split(separator).count();
    budget.reserve::<String>(n, budget.limits.max_entries, at, "audio metadata fields")?;
    let mut result = Vec::with_capacity(n);
    for field in text.split(separator) {
        budget.reserve::<u8>(
            field.len(),
            budget.limits.max_string_bytes,
            at,
            "audio metadata string",
        )?;
        result.push(field.to_owned());
    }
    Ok(result)
}

fn ascii_text(bytes: &[u8], at: usize) -> Result<&str> {
    if !bytes.is_ascii() {
        return Err(unsupported(at, "non-ASCII audio metadata text"));
    }
    Ok(std::str::from_utf8(bytes).expect("ASCII"))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackDetail {
    pub argument_type: u32,
    pub control_group: u32,
    pub ducking_priority: u32,
    pub looped: u32,
    pub volume: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub magic: [u8; 4],
    pub wrapped: bool,
    pub version: u32,
    pub track_name: String,
    pub sound_id: u32,
    pub track_id: u32,
    pub detail: Option<TrackDetail>,
    /// Includes all unknown/unused CSV fields without reordering or discarding.
    pub fields: Vec<String>,
    pub trailing: Vec<u8>,
}

pub fn decode_track(bytes: &[u8], limits: &Limits) -> Result<Track> {
    let mut budget = DecodeBudget::new(bytes, limits)?;
    let mut r = Reader::new(bytes);
    let magic: [u8; 4] = r.read_bytes(4)?.try_into().unwrap();
    let wrapped = &magic == b"2DKT";
    if !wrapped && &magic != b"TKDT" {
        return Err(Error::new(ErrorKind::InvalidMagic, 0, "TRK magic"));
    }
    let text = if wrapped {
        let n = r.i32_le()?;
        if n < 0 {
            return Err(invalid(4, "negative 2DKT text length"));
        }
        limits.check_count(n as usize, limits.max_resource_bytes, 4, "2DKT text bytes")?;
        ascii_text(r.read_bytes(n as usize)?, 8)?
    } else {
        ascii_text(r.read_bytes(r.remaining())?, 4)?
    };
    let fields = fields(text, ',', &mut budget, 4)?;
    if fields.len() < 6 {
        return Err(invalid(4, "TRK requires at least six CSV fields"));
    }
    let version = source_number(&fields[1], false, 4)?;
    if version != 1 && version != 2 {
        return Err(unsupported(4, "unsupported TRK version"));
    }
    budget.reserve::<u8>(fields[2].len(), limits.max_string_bytes, 4, "track name")?;
    let track_name = fields[2].clone();
    let sound_id = source_number(&fields[3], false, 4)?;
    let track_id = source_number(&fields[4], false, 4)?;
    let detail = if fields[5] == "\r\n" || fields[5] == "ETKD" || fields[5].is_empty() {
        None
    } else {
        let index = if version == 2 { 12 } else { 11 };
        if fields.len() <= index + 2 {
            return Err(invalid(4, "truncated extended TRK fields"));
        }
        Some(TrackDetail {
            argument_type: source_number(&fields[5], false, 4)?,
            control_group: source_number(&fields[7], false, 4)?,
            ducking_priority: source_number(&fields[index], false, 4)?,
            looped: source_number(&fields[index + 1], false, 4)?,
            volume: source_number(&fields[index + 2], false, 4)?,
        })
    };
    budget.reserve::<u8>(
        r.remaining(),
        limits.max_resource_bytes,
        r.position(),
        "TRK trailing bytes",
    )?;
    let trailing = r.read_bytes(r.remaining())?.to_vec();
    Ok(Track {
        magic,
        wrapped,
        version,
        track_name,
        sound_id,
        track_id,
        detail,
        fields,
        trailing,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEntry {
    pub source_name: String,
    pub name: String,
    pub event_type: u32,
    pub track_id: u32,
    pub unknown: [u32; 4],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventTable {
    pub entries: Vec<EventEntry>,
}

pub fn decode_events(bytes: &[u8], limits: &Limits) -> Result<EventTable> {
    let mut budget = DecodeBudget::new(bytes, limits)?;
    let text = ascii_text(bytes, 0)?;
    let n = text.split("\r\n").filter(|line| !line.is_empty()).count();
    budget.reserve::<EventEntry>(n, limits.max_entries, 0, "EVT entries")?;
    let mut entries = Vec::with_capacity(n);
    for line in text.split("\r\n").filter(|line| !line.is_empty()) {
        // Source splits specifically on CRLF. LF-only records are not guessed.
        if line.contains('\n') || line.contains('\r') {
            return Err(unsupported(0, "EVT requires CRLF records"));
        }
        let values = fields(line, ',', &mut budget, 0)?;
        if values.len() != 7 {
            return Err(invalid(0, "EVT record requires seven fields"));
        }
        budget.reserve::<u8>(
            values[0].len().saturating_mul(2),
            limits.max_string_bytes.saturating_mul(2),
            0,
            "EVT names",
        )?;
        entries.push(EventEntry {
            source_name: values[0].clone(),
            name: values[0].to_ascii_lowercase(),
            event_type: source_number(&values[1], true, 0)?,
            track_id: source_number(&values[2], true, 0)?,
            unknown: [
                source_number(&values[3], true, 0)?,
                source_number(&values[4], true, 0)?,
                source_number(&values[5], true, 0)?,
                source_number(&values[6], true, 0)?,
            ],
        });
    }
    Ok(EventTable { entries })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedConstant {
    pub source_name: String,
    pub name: String,
    pub value: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hsm {
    pub constants: Vec<NamedConstant>,
}

impl Hsm {
    /// HSM source retains the first definition of each lowercased name.
    pub fn first(&self, name: &str) -> Option<i32> {
        self.constants
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
            .map(|c| c.value)
    }
}

pub fn decode_hsm(bytes: &[u8], limits: &Limits) -> Result<Hsm> {
    let mut budget = DecodeBudget::new(bytes, limits)?;
    let text = ascii_text(bytes, 0)?;
    let n = text.lines().count();
    budget.reserve::<NamedConstant>(n, limits.max_entries, 0, "HSM constants")?;
    let mut constants = Vec::with_capacity(n);
    for line in text.lines() {
        let mut fields = line.split(' ');
        let name = fields
            .next()
            .ok_or_else(|| invalid(0, "missing HSM name"))?;
        let value = fields
            .next()
            .ok_or_else(|| invalid(0, "missing HSM value"))?
            .trim()
            .parse::<i32>()
            .map_err(|_| invalid(0, "invalid HSM integer"))?;
        budget.reserve::<u8>(
            name.len().saturating_mul(2),
            limits.max_string_bytes.saturating_mul(2),
            0,
            "HSM names",
        )?;
        constants.push(NamedConstant {
            source_name: name.to_owned(),
            name: name.to_ascii_lowercase(),
            value,
        });
    }
    Ok(Hsm { constants })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HitEntrypoint {
    pub track_id: u32,
    pub address: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HitMetadata {
    /// Raw identifiers/versions are metadata only; no HIT VM version is claimed.
    pub magic: [u8; 4],
    pub major_version: u32,
    pub minor_version: u32,
    pub signature: [u8; 4],
    pub table_offset: Option<usize>,
    pub entrypoints: Vec<HitEntrypoint>,
    pub source_bytes: usize,
}

/// Mirrors HITFile's first ENTP marker search and ordered entrypoint table, with
/// explicit duplicate/offset/bounds checks. Bytecode remains opaque to this API.
pub fn decode_hit_metadata(bytes: &[u8], limits: &Limits) -> Result<HitMetadata> {
    let mut budget = DecodeBudget::new(bytes, limits)?;
    let mut r = Reader::new(bytes);
    let magic = r.read_bytes(4)?.try_into().unwrap();
    let major_version = r.u32_le()?;
    let minor_version = r.u32_le()?;
    let signature = r.read_bytes(4)?.try_into().unwrap();
    let table_offset = bytes
        .windows(4)
        .position(|w| w == b"ENTP")
        .map(|offset| offset + 4);
    let mut entrypoints = Vec::new();
    if let Some(offset) = table_offset {
        r.seek(offset)?;
        let mut tracks = BTreeSet::new();
        loop {
            let at = r.position();
            let marker = r.read_bytes(4)?;
            if marker.eq_ignore_ascii_case(b"EENT") {
                break;
            }
            let track_id = u32::from_le_bytes(marker.try_into().unwrap());
            let address = r.u32_le()?;
            if address as usize >= bytes.len() {
                return Err(invalid(at + 4, "HIT entrypoint outside bytecode file"));
            }
            limits.check_count(
                entrypoints.len() + 1,
                limits.max_entries,
                at,
                "HIT entrypoints",
            )?;
            budget.reserve::<HitEntrypoint>(1, limits.max_entries, at, "HIT entrypoints")?;
            budget.reserve::<usize>(
                4,
                limits.max_entries.saturating_mul(4),
                at,
                "HIT duplicate-check scratch",
            )?;
            if !tracks.insert(track_id) {
                return Err(Error::new(
                    ErrorKind::Duplicate,
                    at,
                    "duplicate HIT track ID",
                ));
            }
            entrypoints.push(HitEntrypoint { track_id, address });
        }
    }
    Ok(HitMetadata {
        magic,
        major_version,
        minor_version,
        signature,
        table_offset,
        entrypoints,
        source_bytes: bytes.len(),
    })
}
