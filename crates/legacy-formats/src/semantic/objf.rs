// SPDX-License-Identifier: MPL-2.0
//! Source layout: TSOClient/tso.files/Formats/IFF/Chunks/OBJf.cs.
//! The source reader accepts version/padding words and trailing bytes. Preserve
//! them for editing; only the supported fJBO-tagged layout is admitted here.

use super::{resource_input, Budget};
use crate::{reader::Reader, Error, ErrorKind, Limits, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjfFunction {
    pub condition: u16,
    pub action: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Objf {
    pub padding: u32,
    pub version: u32,
    pub functions: Vec<ObjfFunction>,
    pub trailing: Vec<u8>,
}

pub fn decode_objf(bytes: &[u8], limits: &Limits) -> Result<Objf> {
    resource_input(bytes, limits)?;
    let mut reader = Reader::new(bytes);
    let padding = reader.u32_le()?;
    let version = reader.u32_le()?;
    if reader.read_bytes(4)? != b"fJBO" {
        return Err(Error::new(ErrorKind::InvalidData, 8, "OBJf magic"));
    }
    let count = usize::try_from(reader.u32_le()?)
        .map_err(|_| Error::new(ErrorKind::Overflow, 12, "OBJf function count"))?;
    let mut budget = Budget::new(limits);
    budget.entries::<ObjfFunction>(count, 12)?;
    let table_bytes = count
        .checked_mul(4)
        .ok_or_else(|| Error::new(ErrorKind::Overflow, 12, "OBJf function bytes"))?;
    // Validate the entire declared range before allocating an entry vector.
    let mut entries = Reader::new(reader.read_bytes(table_bytes)?);
    budget.add(reader.remaining(), reader.position())?;
    let mut functions = Vec::new();
    functions
        .try_reserve_exact(count)
        .map_err(|_| Error::new(ErrorKind::LimitExceeded, 16, "OBJf function allocation"))?;
    for _ in 0..count {
        functions.push(ObjfFunction {
            condition: entries.u16_le()?,
            action: entries.u16_le()?,
        });
    }
    let trailing = reader.read_bytes(reader.remaining())?.to_vec();
    Ok(Objf {
        padding,
        version,
        functions,
        trailing,
    })
}

pub fn encode_objf(table: &Objf, limits: &Limits) -> Result<Vec<u8>> {
    limits.check_count(
        table.functions.len(),
        limits.max_entries,
        12,
        "OBJf functions",
    )?;
    let count = u32::try_from(table.functions.len())
        .map_err(|_| Error::new(ErrorKind::Overflow, 12, "OBJf function count"))?;
    let size = table
        .functions
        .len()
        .checked_mul(4)
        .and_then(|n| n.checked_add(16))
        .and_then(|n| n.checked_add(table.trailing.len()))
        .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "OBJf encoded length"))?;
    limits.check_count(size, limits.max_resource_bytes, 0, "OBJf output bytes")?;
    limits.check_count(
        size,
        limits.max_total_decoded_bytes,
        0,
        "OBJf output budget",
    )?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size)
        .map_err(|_| Error::new(ErrorKind::LimitExceeded, 0, "OBJf output allocation"))?;
    bytes.extend_from_slice(&table.padding.to_le_bytes());
    bytes.extend_from_slice(&table.version.to_le_bytes());
    bytes.extend_from_slice(b"fJBO");
    bytes.extend_from_slice(&count.to_le_bytes());
    for function in &table.functions {
        bytes.extend_from_slice(&function.condition.to_le_bytes());
        bytes.extend_from_slice(&function.action.to_le_bytes());
    }
    bytes.extend_from_slice(&table.trailing);
    Ok(bytes)
}
