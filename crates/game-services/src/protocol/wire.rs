use super::invalid;
use crate::{ErrorCode, ServiceError, ServiceResult};

pub(super) struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
    little: bool,
}
impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            pos: 0,
            little: false,
        }
    }
    pub fn little_endian(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            pos: 0,
            little: true,
        }
    }
    pub fn take(&mut self, size: usize) -> ServiceResult<&'a [u8]> {
        let end = self.pos.checked_add(size).ok_or_else(invalid)?;
        let slice = self.bytes.get(self.pos..end).ok_or_else(invalid)?;
        self.pos = end;
        Ok(slice)
    }
    pub fn byte(&mut self) -> ServiceResult<u8> {
        Ok(self.take(1)?[0])
    }
    pub fn bool(&mut self) -> ServiceResult<bool> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(invalid()),
        }
    }
    pub fn u16(&mut self) -> ServiceResult<u16> {
        let b = self.take(2)?.try_into().unwrap();
        Ok(if self.little {
            u16::from_le_bytes(b)
        } else {
            u16::from_be_bytes(b)
        })
    }
    pub fn u32(&mut self) -> ServiceResult<u32> {
        let b = self.take(4)?.try_into().unwrap();
        Ok(if self.little {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        })
    }
    pub fn u64(&mut self) -> ServiceResult<u64> {
        let b = self.take(8)?.try_into().unwrap();
        Ok(if self.little {
            u64::from_le_bytes(b)
        } else {
            u64::from_be_bytes(b)
        })
    }
    pub fn count(&mut self, min: usize) -> ServiceResult<usize> {
        let count = self.u32()? as usize;
        if count > (self.bytes.len() - self.pos) / min {
            Err(invalid())
        } else {
            Ok(count)
        }
    }
    pub fn vlc(&mut self) -> ServiceResult<String> {
        let mut size = 0usize;
        for shift in (0..35).step_by(7) {
            let b = self.byte()?;
            if shift == 28 && b > 15 {
                return Err(invalid());
            }
            size |= ((b & 127) as usize) << shift;
            if b & 128 == 0 {
                return String::from_utf8(self.take(size)?.to_vec()).map_err(|_| invalid());
            }
        }
        Err(invalid())
    }
    pub fn pascal(&mut self) -> ServiceResult<String> {
        let size = (self.u32()? & 0x7fffffff) as usize;
        Ok(self.take(size)?.iter().map(|b| char::from(*b)).collect())
    }
    pub fn long_ascii(&mut self) -> ServiceResult<String> {
        let size = self.u16()? as usize;
        if size > 32767 {
            return Err(invalid());
        }
        let bytes = self.take(size)?;
        if !bytes.is_ascii() {
            return Err(invalid());
        }
        String::from_utf8(bytes.to_vec()).map_err(|_| invalid())
    }
    pub fn finish(&self) -> ServiceResult<()> {
        if self.pos == self.bytes.len() {
            Ok(())
        } else {
            Err(invalid())
        }
    }
}

#[derive(Default)]
pub(super) struct Writer(pub Vec<u8>, bool);
impl Writer {
    pub fn little_endian() -> Self {
        Self(Vec::new(), true)
    }
    pub fn bytes(&mut self, value: &[u8]) {
        self.0.extend_from_slice(value);
    }
    pub fn byte(&mut self, value: u8) {
        self.0.push(value);
    }
    pub fn u16(&mut self, value: u16) {
        self.0.extend(if self.1 {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        });
    }
    pub fn u32(&mut self, value: u32) {
        self.0.extend(if self.1 {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        });
    }
    pub fn u64(&mut self, value: u64) {
        self.0.extend(if self.1 {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        });
    }
    pub fn vlc(&mut self, value: &str) -> ServiceResult<()> {
        if value.len() > 65535 {
            return Err(budget());
        }
        let mut len = value.len();
        loop {
            self.byte(((len & 127) as u8) | if len > 127 { 128 } else { 0 });
            len >>= 7;
            if len == 0 {
                break;
            }
        }
        self.bytes(value.as_bytes());
        Ok(())
    }
    pub fn pascal(&mut self, value: &str) -> ServiceResult<()> {
        if value.len() > 65535 || value.chars().any(|c| u32::from(c) > 255) {
            return Err(ServiceError::new(
                ErrorCode::InvalidRequest,
                "This original protocol field requires single-byte text within its transport budget",
            ));
        }
        self.u32(0x80000000 | value.chars().count() as u32);
        self.0.extend(value.chars().map(|c| c as u8));
        Ok(())
    }
    pub fn long_ascii(&mut self, value: &str) -> ServiceResult<()> {
        if !value.is_ascii() || value.len() > 32767 {
            return Err(ServiceError::new(
                ErrorCode::InvalidRequest,
                "Original mail fields require ASCII text within their 16-bit byte length",
            ));
        }
        self.u16(value.len() as u16);
        self.bytes(value.as_bytes());
        Ok(())
    }
}
fn budget() -> ServiceError {
    ServiceError::new(
        ErrorCode::InvalidRequest,
        "Original text exceeds the transport byte budget",
    )
}
