// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Exact bounded private codec. Counts are rejected before copying any payload.
use crate::Error;

#[derive(Default)]
pub(crate) struct Writer(pub(crate) Vec<u8>);
macro_rules! write_int {
    ($($name:ident: $ty:ty),*) => {$ (pub(crate) fn $name(&mut self, v: $ty) { self.0.extend_from_slice(&v.to_le_bytes()); })*};
}
impl Writer {
    pub(crate) fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    pub(crate) fn bool(&mut self, v: bool) {
        self.u8(u8::from(v));
    }
    write_int!(u16:u16,i16:i16,u32:u32,i32:i32,u64:u64,i64:i64);
    pub(crate) fn fixed(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }
    pub(crate) fn bytes(&mut self, bytes: &[u8]) {
        self.u32(bytes.len() as u32);
        self.fixed(bytes);
    }
    pub(crate) fn string(&mut self, s: &str) {
        self.bytes(s.as_bytes());
    }
}
pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
macro_rules! read_int {
    ($($name:ident: $ty:ty),*) => {$ (pub(crate) fn $name(&mut self) -> Result<$ty,Error> {
        Ok(<$ty>::from_le_bytes(self.take(std::mem::size_of::<$ty>())?.try_into().map_err(|_|Error::InvalidCheckpoint)?))
    })*};
}
impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }
    pub(crate) fn take(&mut self, len: usize) -> Result<&'a [u8], Error> {
        let end = self.at.checked_add(len).ok_or(Error::InvalidCheckpoint)?;
        let result = self
            .bytes
            .get(self.at..end)
            .ok_or(Error::InvalidCheckpoint)?;
        self.at = end;
        Ok(result)
    }
    pub(crate) fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }
    pub(crate) fn bool(&mut self) -> Result<bool, Error> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::InvalidCheckpoint),
        }
    }
    read_int!(u16:u16,i16:i16,u32:u32,i32:i32,u64:u64,i64:i64);
    pub(crate) fn bytes(&mut self, max: usize) -> Result<Vec<u8>, Error> {
        let n = self.u32()? as usize;
        if n > max {
            return Err(Error::InvalidCheckpoint);
        }
        Ok(self.take(n)?.to_vec())
    }
    pub(crate) fn string(&mut self, max: usize) -> Result<String, Error> {
        let bytes = self.bytes(max)?;
        String::from_utf8(bytes).map_err(|_| Error::InvalidCheckpoint)
    }
    pub(crate) fn count(&mut self, max: usize) -> Result<usize, Error> {
        let n = self.u32()? as usize;
        if n > max {
            Err(Error::InvalidCheckpoint)
        } else {
            Ok(n)
        }
    }
    pub(crate) fn finish(&self) -> Result<(), Error> {
        if self.at == self.bytes.len() {
            Ok(())
        } else {
            Err(Error::InvalidCheckpoint)
        }
    }
}
