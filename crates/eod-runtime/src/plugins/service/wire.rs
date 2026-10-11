// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Source BinaryWriter strings and bounded legacy UI/provider records.
use crate::Error;

pub(super) struct SourceReader<'a> {
    pub(super) bytes: &'a [u8],
    pub(super) at: usize,
}
impl<'a> SourceReader<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }
    pub(super) fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let end = self.at.checked_add(n).ok_or(Error::InvalidPluginData)?;
        let b = self
            .bytes
            .get(self.at..end)
            .ok_or(Error::InvalidPluginData)?;
        self.at = end;
        Ok(b)
    }
    pub(super) fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }
    pub(super) fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| Error::InvalidPluginData)?,
        ))
    }
    pub(super) fn i64(&mut self) -> Result<i64, Error> {
        Ok(i64::from_le_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| Error::InvalidPluginData)?,
        ))
    }
    pub(super) fn string(&mut self, max_bytes: usize, max_units: usize) -> Result<String, Error> {
        let mut length = 0u32;
        for shift in (0..35).step_by(7) {
            let b = self.u8()?;
            if shift == 28 && b > 15 {
                return Err(Error::InvalidPluginData);
            }
            length |= u32::from(b & 127) << shift;
            if b & 128 == 0 {
                if length as usize > max_bytes {
                    return Err(Error::InvalidPluginData);
                }
                let s = std::str::from_utf8(self.take(length as usize)?)
                    .map_err(|_| Error::InvalidPluginData)?;
                if s.encode_utf16().count() > max_units {
                    return Err(Error::InvalidPluginData);
                }
                return Ok(s.to_string());
            }
        }
        Err(Error::InvalidPluginData)
    }
    pub(super) fn finish(&self) -> Result<(), Error> {
        if self.at == self.bytes.len() {
            Ok(())
        } else {
            Err(Error::InvalidPluginData)
        }
    }
}
pub(super) fn write_string(out: &mut Vec<u8>, s: &str) {
    let mut n = s.len();
    while n >= 128 {
        out.push((n as u8 & 127) | 128);
        n >>= 7;
    }
    out.push(n as u8);
    out.extend_from_slice(s.as_bytes());
}
pub(super) fn strings(values: &[&str]) -> Vec<u8> {
    let mut out = vec![];
    for s in values {
        write_string(&mut out, s)
    }
    out
}
pub(super) fn read_strings(bytes: &[u8], max: usize, units: usize) -> Result<Vec<String>, Error> {
    let mut r = SourceReader::new(bytes);
    let mut out = vec![];
    while r.at < bytes.len() {
        if out.len() >= max {
            return Err(Error::InvalidMessage);
        }
        out.push(r.string(units * 3, units)?)
    }
    Ok(out)
}
pub(super) fn parse_i32(bytes: &[u8]) -> Option<i32> {
    let s = std::str::from_utf8(bytes)
        .ok()?
        .trim_end_matches('\0')
        .trim_matches([' ', '\t', '\r', '\n', '\x0b', '\x0c']);
    let digits = s.strip_prefix(['+', '-']).unwrap_or(s);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        None
    } else {
        s.parse().ok()
    }
}
pub(super) fn parse_u32(bytes: &[u8]) -> Option<u32> {
    let s = std::str::from_utf8(bytes)
        .ok()?
        .trim_end_matches('\0')
        .trim_matches([' ', '\t', '\r', '\n', '\x0b', '\x0c']);
    let digits = s.strip_prefix('+').unwrap_or(s);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        None
    } else {
        digits.parse().ok()
    }
}
pub(super) fn parse_u64(bytes: &[u8]) -> Option<u64> {
    let s = std::str::from_utf8(bytes)
        .ok()?
        .trim_end_matches('\0')
        .trim_matches([' ', '\t', '\r', '\n', '\x0b', '\x0c']);
    let digits = s.strip_prefix('+').unwrap_or(s);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        None
    } else {
        digits.parse().ok()
    }
}
pub(super) fn truncated_utf16(text: &str, units: usize) -> String {
    let mut out = String::new();
    let mut n = 0;
    for c in text.chars() {
        if n + c.len_utf16() > units {
            break;
        }
        out.push(c);
        n += c.len_utf16();
    }
    out
}
