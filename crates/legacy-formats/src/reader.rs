// SPDX-License-Identifier: MPL-2.0
use crate::{Error, ErrorKind, Result};

/// A non-owning reader. Failed reads and seeks leave the position unchanged.
#[derive(Clone, Debug)]
pub struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

macro_rules! numeric {
    ($name:ident, $ty:ty, $size:literal, $convert:ident) => {
        pub fn $name(&mut self) -> Result<$ty> {
            let raw: [u8; $size] = self
                .read_bytes($size)?
                .try_into()
                .map_err(|_| Error::new(ErrorKind::Truncated, self.position, "numeric value"))?;
            Ok(<$ty>::$convert(raw))
        }
    };
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }
    pub fn position(&self) -> usize {
        self.position
    }
    pub fn remaining(&self) -> usize {
        self.bytes.len() - self.position
    }
    pub fn read_bytes(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(count)
            .ok_or_else(|| Error::new(ErrorKind::Overflow, self.position, "read length"))?;
        let bytes = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| Error::new(ErrorKind::Truncated, self.position, "read past input"))?;
        self.position = end;
        Ok(bytes)
    }
    pub fn skip(&mut self, count: usize) -> Result<()> {
        self.read_bytes(count).map(|_| ())
    }
    pub fn seek(&mut self, position: usize) -> Result<()> {
        if position > self.bytes.len() {
            return Err(Error::new(
                ErrorKind::Truncated,
                self.position,
                "seek past input",
            ));
        }
        self.position = position;
        Ok(())
    }
    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.read_bytes(1)?[0])
    }
    pub fn i8(&mut self) -> Result<i8> {
        Ok(self.u8()? as i8)
    }
    numeric!(u16_le, u16, 2, from_le_bytes);
    numeric!(u16_be, u16, 2, from_be_bytes);
    numeric!(i16_le, i16, 2, from_le_bytes);
    numeric!(i16_be, i16, 2, from_be_bytes);
    numeric!(u32_le, u32, 4, from_le_bytes);
    numeric!(u32_be, u32, 4, from_be_bytes);
    numeric!(i32_le, i32, 4, from_le_bytes);
    numeric!(i32_be, i32, 4, from_be_bytes);
    numeric!(u64_le, u64, 8, from_le_bytes);
    numeric!(u64_be, u64, 8, from_be_bytes);
    numeric!(f32_le, f32, 4, from_le_bytes);
    numeric!(f32_be, f32, 4, from_be_bytes);
    /// Reads UTF-8 through a NUL, with at most `max` bytes before the terminator.
    pub fn c_string(&mut self, max: usize) -> Result<String> {
        let remaining = &self.bytes[self.position..];
        let search_len = remaining.len().min(max.saturating_add(1));
        let length = remaining[..search_len]
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| {
                Error::new(
                    if remaining.len() > max {
                        ErrorKind::LimitExceeded
                    } else {
                        ErrorKind::Truncated
                    },
                    self.position,
                    "NUL-terminated string",
                )
            })?;
        let value = std::str::from_utf8(&remaining[..length])
            .map_err(|_| Error::new(ErrorKind::InvalidData, self.position, "string is not UTF-8"))?
            .to_owned();
        self.position += length + 1;
        Ok(value)
    }
}
