use crate::{DecodeLimits, Error, ErrorKind, Result};
use std::{cell::RefCell, rc::Rc};
#[derive(Default)]
pub(crate) struct Budget {
    entries: usize,
    strings: usize,
    decompressed: usize,
}
pub(crate) struct Reader<'a, 'l> {
    pub bytes: &'a [u8],
    pub at: usize,
    pub limits: &'l DecodeLimits,
    budget: Rc<RefCell<Budget>>,
}
impl<'a, 'l> Reader<'a, 'l> {
    pub fn new(bytes: &'a [u8], limits: &'l DecodeLimits) -> Result<Self> {
        if bytes.len() > limits.max_input_bytes {
            return Err(Error {
                offset: 0,
                kind: ErrorKind::Limit,
                context: "VM wire input bytes",
            });
        }
        Ok(Self {
            bytes,
            at: 0,
            limits,
            budget: Rc::new(RefCell::new(Budget::default())),
        })
    }
    pub fn fork<'b>(&self, bytes: &'b [u8]) -> Reader<'b, 'l> {
        Reader {
            bytes,
            at: 0,
            limits: self.limits,
            budget: self.budget.clone(),
        }
    }
    pub fn error(&self, kind: ErrorKind, context: &'static str) -> Error {
        Error {
            offset: self.at,
            kind,
            context,
        }
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| self.error(ErrorKind::Limit, "VM byte offset"))?;
        let b = self
            .bytes
            .get(self.at..end)
            .ok_or_else(|| self.error(ErrorKind::Truncated, "VM field bytes"))?;
        self.at = end;
        Ok(b)
    }
    pub fn finish(&self) -> Result<()> {
        if self.at != self.bytes.len() {
            Err(self.error(ErrorKind::Trailing, "VM trailing bytes"))
        } else {
            Ok(())
        }
    }
    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    pub fn i8(&mut self) -> Result<i8> {
        Ok(self.u8()? as i8)
    }
    pub fn boolean(&mut self) -> Result<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(self.error(ErrorKind::Invalid, "noncanonical BinaryWriter boolean")),
        }
    }
    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    pub fn i16(&mut self) -> Result<i16> {
        Ok(self.u16()? as i16)
    }
    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }
    pub fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    pub fn i64(&mut self) -> Result<i64> {
        Ok(self.u64()? as i64)
    }
    pub fn f32(&mut self) -> Result<f32> {
        let n = f32::from_bits(self.u32()?);
        if !n.is_finite() {
            return Err(self.error(ErrorKind::Invalid, "nonfinite VM float"));
        }
        Ok(n)
    }
    pub fn f64(&mut self) -> Result<f64> {
        let n = f64::from_bits(self.u64()?);
        if !n.is_finite() {
            return Err(self.error(ErrorKind::Invalid, "nonfinite VM double"));
        }
        Ok(n)
    }
    pub fn reserve(&self, n: usize) -> Result<()> {
        let mut b = self.budget.borrow_mut();
        b.entries = b
            .entries
            .checked_add(n)
            .filter(|n| *n <= self.limits.max_total_entries)
            .ok_or_else(|| self.error(ErrorKind::Limit, "total VM entries"))?;
        Ok(())
    }
    /// Reject impossible collections before their callers reserve any storage.
    /// The stride is the smallest emitted record, including empty/null variants.
    pub fn require_items(&self, n: usize, min_item_bytes: usize) -> Result<()> {
        let required = n
            .checked_mul(min_item_bytes)
            .ok_or_else(|| self.error(ErrorKind::Limit, "VM collection byte span"))?;
        if required > self.bytes.len().saturating_sub(self.at) {
            return Err(self.error(ErrorKind::Truncated, "VM collection minimum wire bytes"));
        }
        Ok(())
    }
    pub fn count_max(&mut self, max: usize, min_item_bytes: usize) -> Result<usize> {
        let n = self.i32()?;
        self.checked_count(n, max, min_item_bytes)
    }
    pub fn checked_count(&self, n: i32, max: usize, min_item_bytes: usize) -> Result<usize> {
        if n < 0 {
            return Err(self.error(ErrorKind::Invalid, "negative VM count"));
        }
        let n = n as usize;
        if n > max {
            return Err(self.error(ErrorKind::Limit, "VM count"));
        }
        self.require_items(n, min_item_bytes)?;
        self.reserve(n)?;
        Ok(n)
    }
    pub fn count(&mut self, min_item_bytes: usize) -> Result<usize> {
        self.count_max(self.limits.max_count, min_item_bytes)
    }
    pub fn short_count(&mut self, min_item_bytes: usize) -> Result<usize> {
        let n = i32::from(self.i16()?);
        self.checked_count(n, self.limits.max_count, min_item_bytes)
    }
    pub fn byte_count(&mut self, min_item_bytes: usize) -> Result<usize> {
        let n = i32::from(self.u8()?);
        self.checked_count(n, self.limits.max_count, min_item_bytes)
    }
    pub fn nullable_count(&mut self, min_item_bytes: usize) -> Result<Option<usize>> {
        let n = self.i32()?;
        if n == -1 {
            Ok(None)
        } else {
            self.checked_count(n, self.limits.max_count, min_item_bytes)
                .map(Some)
        }
    }
    pub fn byte_length(&mut self) -> Result<usize> {
        let n = self.i32()?;
        if n < 0 {
            return Err(self.error(ErrorKind::Invalid, "negative VM byte length"));
        }
        let n = n as usize;
        if n > self.limits.max_input_bytes {
            return Err(self.error(ErrorKind::Limit, "VM field byte length"));
        }
        Ok(n)
    }
    pub fn bytes_i32(&mut self) -> Result<Vec<u8>> {
        let n = self.byte_length()?;
        Ok(self.take(n)?.to_vec())
    }
    pub fn shorts(&mut self) -> Result<Vec<i16>> {
        let n = self.count(2)?;
        self.short_values(n)
    }
    pub fn short_values(&mut self, n: usize) -> Result<Vec<i16>> {
        let b = self.take(
            n.checked_mul(2)
                .ok_or_else(|| self.error(ErrorKind::Limit, "short array bytes"))?,
        )?;
        Ok(b.as_chunks::<2>()
            .0
            .iter()
            .copied()
            .map(i16::from_le_bytes)
            .collect())
    }
    pub fn optional_shorts(&mut self) -> Result<Option<Vec<i16>>> {
        self.nullable_count(2)?
            .map(|n| self.short_values(n))
            .transpose()
    }
    pub fn text(&mut self) -> Result<String> {
        let mut n = 0u32;
        let mut groups = 0;
        loop {
            let b = self.u8()?;
            if groups == 4 && b > 7 {
                return Err(self.error(ErrorKind::Invalid, ".NET 7-bit string length overflow"));
            }
            n |= u32::from(b & 127) << (groups * 7);
            groups += 1;
            if b & 128 == 0 {
                if groups > 1 && b == 0 {
                    return Err(self.error(ErrorKind::Invalid, "noncanonical .NET string length"));
                }
                break;
            }
            if groups == 5 {
                return Err(self.error(ErrorKind::Invalid, ".NET string length overflow"));
            }
        }
        let n = n as usize;
        if n > self.limits.max_string_bytes {
            return Err(self.error(ErrorKind::Limit, "VM string bytes"));
        }
        let mut b = self.budget.borrow_mut();
        b.strings = b
            .strings
            .checked_add(n)
            .filter(|n| *n <= self.limits.max_total_string_bytes)
            .ok_or_else(|| self.error(ErrorKind::Limit, "total VM string bytes"))?;
        drop(b);
        self.reserve(1)?;
        let bytes = self.take(n)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| self.error(ErrorKind::Invalid, "VM UTF-8 string"))
    }
    pub fn decompress_remaining(&self) -> usize {
        self.limits
            .max_decompressed_bytes
            .saturating_sub(self.budget.borrow().decompressed)
    }
    pub fn decompress_charge(&self, n: usize) -> Result<()> {
        let mut b = self.budget.borrow_mut();
        b.decompressed = b
            .decompressed
            .checked_add(n)
            .filter(|n| *n <= self.limits.max_decompressed_bytes)
            .ok_or_else(|| self.error(ErrorKind::Limit, "total decompressed VM bytes"))?;
        Ok(())
    }
}
