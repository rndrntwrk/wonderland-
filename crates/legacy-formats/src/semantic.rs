// Source-derived from FreeSO C# implementations in TSOClient/tso.files.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy was not distributed with this file, obtain it
// at https://mozilla.org/MPL/2.0/.
//! Bounded, editable semantic resource data. No BHAV execution lives here.

use crate::{reader::Reader, Error, ErrorKind, Limits, Result};
use serde::{Deserialize, Serialize};

mod objf;
pub use objf::{decode_objf, encode_objf, Objf, ObjfFunction};

fn invalid(offset: usize, context: impl Into<String>) -> Error {
    Error::new(ErrorKind::InvalidData, offset, context)
}

fn resource_input(bytes: &[u8], limits: &Limits) -> Result<()> {
    limits.check_input(bytes)?;
    limits.check_count(
        bytes.len(),
        limits.max_resource_bytes,
        0,
        "semantic resource bytes",
    )?;
    limits.check_count(
        bytes.len(),
        limits.max_total_decoded_bytes,
        0,
        "semantic decoded bytes",
    )
}

struct Budget<'a> {
    limits: &'a Limits,
    used: usize,
}

impl<'a> Budget<'a> {
    fn new(limits: &'a Limits) -> Self {
        Self { limits, used: 0 }
    }
    fn add(&mut self, bytes: usize, offset: usize) -> Result<()> {
        self.used = self
            .used
            .checked_add(bytes)
            .ok_or_else(|| Error::new(ErrorKind::Overflow, offset, "semantic allocation budget"))?;
        self.limits.check_count(
            self.used,
            self.limits.max_total_decoded_bytes,
            offset,
            "semantic allocation budget",
        )
    }
    fn entries<T>(&mut self, count: usize, offset: usize) -> Result<()> {
        self.limits.check_count(
            count,
            self.limits.max_entries,
            offset,
            "semantic entry count",
        )?;
        let bytes = count
            .checked_mul(std::mem::size_of::<T>())
            .ok_or_else(|| Error::new(ErrorKind::Overflow, offset, "semantic entry allocation"))?;
        self.add(bytes, offset)
    }
    fn tail(&mut self, r: &mut Reader<'_>) -> Result<Vec<u8>> {
        self.add(r.remaining(), r.position())?;
        Ok(r.read_bytes(r.remaining())?.to_vec())
    }
    fn reserve_one<T>(&mut self, values: &mut Vec<T>, offset: usize) -> Result<()> {
        if values.len() < values.capacity() {
            return Ok(());
        }
        let required = values
            .len()
            .checked_add(1)
            .ok_or_else(|| Error::new(ErrorKind::Overflow, offset, "vector capacity"))?;
        self.limits.check_count(
            required,
            self.limits.max_entries,
            offset,
            "vector entry count",
        )?;
        let target = values
            .capacity()
            .saturating_mul(2)
            .max(4)
            .max(required)
            .min(self.limits.max_entries);
        let extra = target
            .checked_sub(values.capacity())
            .ok_or_else(|| Error::new(ErrorKind::Overflow, offset, "vector capacity budget"))?;
        self.entries::<T>(extra, offset)?;
        values
            .try_reserve_exact(target - values.len())
            .map_err(|_| Error::new(ErrorKind::LimitExceeded, offset, "vector allocation"))?;
        Ok(())
    }
}

struct Writer<'a> {
    bytes: Vec<u8>,
    limits: &'a Limits,
}

impl<'a> Writer<'a> {
    fn new(limits: &'a Limits) -> Self {
        Self {
            bytes: Vec::new(),
            limits,
        }
    }
    fn put(&mut self, bytes: &[u8]) -> Result<()> {
        let size = self.bytes.len().checked_add(bytes.len()).ok_or_else(|| {
            Error::new(
                ErrorKind::Overflow,
                self.bytes.len(),
                "semantic encoded size",
            )
        })?;
        self.limits.check_count(
            size,
            self.limits.max_resource_bytes,
            self.bytes.len(),
            "semantic output bytes",
        )?;
        self.limits.check_count(
            size,
            self.limits.max_total_decoded_bytes,
            self.bytes.len(),
            "semantic output budget",
        )?;
        if size > self.bytes.capacity() {
            let ceiling = self
                .limits
                .max_resource_bytes
                .min(self.limits.max_total_decoded_bytes);
            let capacity = self
                .bytes
                .capacity()
                .saturating_mul(2)
                .max(16)
                .max(size)
                .min(ceiling);
            self.bytes
                .try_reserve_exact(capacity - self.bytes.len())
                .map_err(|_| {
                    Error::new(
                        ErrorKind::LimitExceeded,
                        self.bytes.len(),
                        "semantic output allocation",
                    )
                })?;
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    fn u8(&mut self, v: u8) -> Result<()> {
        self.put(&[v])
    }
    fn u16(&mut self, v: u16) -> Result<()> {
        self.put(&v.to_le_bytes())
    }
    fn u32(&mut self, v: u32) -> Result<()> {
        self.put(&v.to_le_bytes())
    }
}

fn count_u16(count: usize, limits: &Limits, context: &str) -> Result<u16> {
    limits.check_count(count, limits.max_entries, 0, context)?;
    u16::try_from(count).map_err(|_| Error::new(ErrorKind::LimitExceeded, 0, context))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BhavInstruction {
    pub opcode: u16,
    pub true_pointer: u8,
    pub false_pointer: u8,
    pub operand: [u8; 8],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bhav {
    pub format_version: u16,
    pub kind: u8,
    pub args: u8,
    pub locals: u16,
    pub tree_version: u16,
    pub reserved: Vec<u8>,
    pub instructions: Vec<BhavInstruction>,
    pub trailing: Vec<u8>,
}

/// Source: Chunks/BHAV.cs. Branch bytes are retained; 253 is the source's
/// alternate-branch/error sentinel, 254 returns true, and 255 returns false.
pub fn decode_bhav(bytes: &[u8], limits: &Limits) -> Result<Bhav> {
    resource_input(bytes, limits)?;
    let mut r = Reader::new(bytes);
    let format_version = r.u16_le()?;
    let (count, kind, args, locals, tree_version, reserved) = match format_version {
        0x8000 | 0x8001 => {
            let count = r.u16_le()? as usize;
            (count, 0, 0, 0, 0, r.read_bytes(8)?.to_vec())
        }
        0x8002 => {
            let count = r.u16_le()? as usize;
            let kind = r.u8()?;
            let args = r.u8()?;
            let locals = r.u16_le()?;
            let tree_version = r.u16_le()?;
            (
                count,
                kind,
                args,
                locals,
                tree_version,
                r.read_bytes(2)?.to_vec(),
            )
        }
        0x8003 => {
            let kind = r.u8()?;
            let args = r.u8()?;
            let locals = u16::from(r.u8()?);
            let reserved = r.read_bytes(2)?.to_vec();
            let tree_version = r.u16_le()?;
            let count = r.u32_le()? as usize;
            (count, kind, args, locals, tree_version, reserved)
        }
        _ => {
            return Err(Error::new(
                ErrorKind::UnsupportedVersion,
                0,
                format!("BHAV {format_version:#06x}"),
            ))
        }
    };
    let mut budget = Budget::new(limits);
    budget.add(reserved.len(), r.position())?;
    budget.entries::<BhavInstruction>(count, r.position())?;
    let size = count
        .checked_mul(12)
        .ok_or_else(|| Error::new(ErrorKind::Overflow, r.position(), "BHAV instructions"))?;
    if r.remaining() < size {
        return Err(Error::new(
            ErrorKind::Truncated,
            r.position(),
            "BHAV instruction data",
        ));
    }
    let mut instructions = Vec::with_capacity(count);
    for _ in 0..count {
        instructions.push(BhavInstruction {
            opcode: r.u16_le()?,
            true_pointer: r.u8()?,
            false_pointer: r.u8()?,
            operand: r
                .read_bytes(8)?
                .try_into()
                .map_err(|_| invalid(r.position(), "BHAV operand"))?,
        });
    }
    let trailing = budget.tail(&mut r)?;
    Ok(Bhav {
        format_version,
        kind,
        args,
        locals,
        tree_version,
        reserved,
        instructions,
        trailing,
    })
}

pub fn encode_bhav(bhav: &Bhav, limits: &Limits) -> Result<Vec<u8>> {
    limits.check_count(
        bhav.instructions.len(),
        limits.max_entries,
        0,
        "BHAV instruction count",
    )?;
    let mut w = Writer::new(limits);
    w.u16(bhav.format_version)?;
    match bhav.format_version {
        0x8000 | 0x8001 => {
            if bhav.reserved.len() != 8
                || bhav.kind != 0
                || bhav.args != 0
                || bhav.locals != 0
                || bhav.tree_version != 0
            {
                return Err(invalid(0, "BHAV legacy header fields"));
            }
            w.u16(count_u16(
                bhav.instructions.len(),
                limits,
                "BHAV instruction count",
            )?)?;
            w.put(&bhav.reserved)?;
        }
        0x8002 => {
            if bhav.reserved.len() != 2 {
                return Err(invalid(0, "BHAV reserved header size"));
            }
            w.u16(count_u16(
                bhav.instructions.len(),
                limits,
                "BHAV instruction count",
            )?)?;
            w.u8(bhav.kind)?;
            w.u8(bhav.args)?;
            w.u16(bhav.locals)?;
            w.u16(bhav.tree_version)?;
            w.put(&bhav.reserved)?;
        }
        0x8003 => {
            if bhav.reserved.len() != 2 || bhav.locals > 255 {
                return Err(invalid(0, "BHAV version 8003 header"));
            }
            w.u8(bhav.kind)?;
            w.u8(bhav.args)?;
            w.u8(bhav.locals as u8)?;
            w.put(&bhav.reserved)?;
            w.u16(bhav.tree_version)?;
            w.u32(
                u32::try_from(bhav.instructions.len())
                    .map_err(|_| invalid(0, "BHAV instruction count"))?,
            )?;
        }
        _ => return Err(Error::new(ErrorKind::UnsupportedVersion, 0, "BHAV version")),
    }
    for i in &bhav.instructions {
        w.u16(i.opcode)?;
        w.u8(i.true_pointer)?;
        w.u8(i.false_pointer)?;
        w.put(&i.operand)?;
    }
    w.put(&bhav.trailing)?;
    Ok(w.bytes)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bcon {
    pub flags: u8,
    pub constants: Vec<u16>,
    pub trailing: Vec<u8>,
}

pub fn decode_bcon(bytes: &[u8], limits: &Limits) -> Result<Bcon> {
    resource_input(bytes, limits)?;
    let mut r = Reader::new(bytes);
    let count = usize::from(r.u8()?);
    let flags = r.u8()?;
    let mut budget = Budget::new(limits);
    budget.entries::<u16>(count, r.position())?;
    let mut constants = Vec::with_capacity(count);
    for _ in 0..count {
        constants.push(r.u16_le()?);
    }
    Ok(Bcon {
        flags,
        constants,
        trailing: budget.tail(&mut r)?,
    })
}

pub fn encode_bcon(value: &Bcon, limits: &Limits) -> Result<Vec<u8>> {
    let count = count_u16(value.constants.len(), limits, "BCON constants")?;
    if count > 255 {
        return Err(Error::new(ErrorKind::LimitExceeded, 0, "BCON byte count"));
    }
    let mut w = Writer::new(limits);
    w.u8(count as u8)?;
    w.u8(value.flags)?;
    for v in &value.constants {
        w.u16(*v)?;
    }
    w.put(&value.trailing)?;
    Ok(w.bytes)
}

/// All stored words, including extension and reserved words, remain editable.
/// The version's documented minimum is validated; an odd trailing byte is raw.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Objd {
    pub version: u32,
    pub fields: Vec<u16>,
    pub trailing: Vec<u8>,
}

pub const OBJD_FIELDS: &[&str] = &[
    "StackSize",
    "BaseGraphicID",
    "NumGraphics",
    "BHAV_MainID",
    "BHAV_GardeningID",
    "TreeTableID",
    "InteractionGroupID",
    "ObjectType",
    "MasterID",
    "SubIndex",
    "BHAV_WashHandsID",
    "AnimationTableID",
    "GUID1",
    "GUID2",
    "Disabled",
    "BHAV_Portal",
    "Price",
    "BodyStringID",
    "SlotID",
    "BHAV_AllowIntersectionID",
    "UsesFnTable",
    "BitField1",
    "BHAV_PrepareFoodID",
    "BHAV_CookFoodID",
    "BHAV_PlaceSurfaceID",
    "BHAV_DisposeID",
    "BHAV_EatID",
    "BHAV_PickupFromSlotID",
    "BHAV_WashDishID",
    "BHAV_EatSurfaceID",
    "BHAV_SitID",
    "BHAV_StandID",
    "SalePrice",
    "InitialDepreciation",
    "DailyDepreciation",
    "SelfDepreciating",
    "DepreciationLimit",
    "RoomFlags",
    "FunctionFlags",
    "CatalogStringsID",
    "Global",
    "BHAV_Init",
    "BHAV_Place",
    "BHAV_UserPickup",
    "WallStyle",
    "BHAV_Load",
    "BHAV_UserPlace",
    "ObjectVersion",
    "BHAV_RoomChange",
    "MotiveEffectsID",
    "BHAV_Cleanup",
    "BHAV_LevelInfo",
    "CatalogID",
    "BHAV_ServingSurface",
    "LevelOffset",
    "Shadow",
    "NumAttributes",
    "BHAV_Clean",
    "BHAV_QueueSkipped",
    "FrontDirection",
    "BHAV_WallAdjacencyChanged",
    "MyLeadObject",
    "DynamicSpriteBaseId",
    "NumDynamicSprites",
    "ChairEntryFlags",
    "TileWidth",
    "LotCategories",
    "BuildModeType",
    "OriginalGUID1",
    "OriginalGUID2",
    "SuitGUID1",
    "SuitGUID2",
    "BHAV_Pickup",
    "ThumbnailGraphic",
    "ShadowFlags",
    "FootprintMask",
    "BHAV_DynamicMultiTileUpdate",
    "ShadowBrightness",
    "BHAV_Repair",
    "WallStyleSpriteID",
    "RatingHunger",
    "RatingComfort",
    "RatingHygiene",
    "RatingBladder",
    "RatingEnergy",
    "RatingFun",
    "RatingRoom",
    "RatingSkillFlags",
    "NumTypeAttributes",
    "MiscFlags",
    "TypeAttrGUID1",
    "TypeAttrGUID2",
    "FunctionSubsort",
    "DTSubsort",
    "KeepBuying",
    "VacationSubsort",
    "ResetLotAction",
    "CommunitySubsort",
    "DreamFlags",
    "RenderFlags",
    "VitaboyFlags",
    "STSubsort",
    "MTSubsort",
];

fn objd_min_words(version: u32) -> Result<usize> {
    match version {
        136 => Ok(78),
        138 => Ok(93),
        139 => Ok(94),
        140 | 141 => Ok(95),
        142 => Ok(103),
        _ => Err(Error::new(
            ErrorKind::UnsupportedVersion,
            0,
            format!("OBJD {version}"),
        )),
    }
}

impl Objd {
    pub fn field(&self, name: &str) -> Option<u16> {
        OBJD_FIELDS
            .iter()
            .position(|n| *n == name)
            .and_then(|i| self.fields.get(i))
            .copied()
    }
    pub fn set_field(&mut self, name: &str, value: u16) -> Result<()> {
        let index = OBJD_FIELDS
            .iter()
            .position(|n| *n == name)
            .ok_or_else(|| invalid(0, "unknown OBJD field"))?;
        let target = self
            .fields
            .get_mut(index)
            .ok_or_else(|| invalid(0, "OBJD field absent in this layout"))?;
        *target = value;
        Ok(())
    }
    pub fn guid(&self) -> u32 {
        u32::from(self.fields.get(12).copied().unwrap_or(0))
            | (u32::from(self.fields.get(13).copied().unwrap_or(0)) << 16)
    }
    pub fn object_type(&self) -> u16 {
        self.field("ObjectType").unwrap_or(0)
    }
    pub fn master_id(&self) -> u16 {
        self.field("MasterID").unwrap_or(0)
    }
    pub fn sub_index(&self) -> i16 {
        self.field("SubIndex").unwrap_or(0) as i16
    }
    pub fn tree_table_id(&self) -> u16 {
        self.field("TreeTableID").unwrap_or(0)
    }
    pub fn slot_id(&self) -> u16 {
        self.field("SlotID").unwrap_or(0)
    }
    pub fn catalog_strings_id(&self) -> u16 {
        self.field("CatalogStringsID").unwrap_or(0)
    }
    pub fn type_attribute_guid(&self) -> u32 {
        let raw = u32::from(self.field("TypeAttrGUID1").unwrap_or(0))
            | (u32::from(self.field("TypeAttrGUID2").unwrap_or(0)) << 16);
        if raw == 0 && self.version != 136 {
            self.guid()
        } else {
            raw
        }
    }
}

pub fn decode_objd(bytes: &[u8], limits: &Limits) -> Result<Objd> {
    resource_input(bytes, limits)?;
    let mut r = Reader::new(bytes);
    let version = r.u32_le()?;
    let min_words = objd_min_words(version)?;
    let count = r.remaining() / 2;
    if count < min_words {
        return Err(Error::new(ErrorKind::Truncated, 4, "OBJD version fields"));
    }
    let mut budget = Budget::new(limits);
    budget.entries::<u16>(count, r.position())?;
    let mut fields = Vec::with_capacity(count);
    for _ in 0..count {
        fields.push(r.u16_le()?);
    }
    Ok(Objd {
        version,
        fields,
        trailing: budget.tail(&mut r)?,
    })
}

pub fn encode_objd(value: &Objd, limits: &Limits) -> Result<Vec<u8>> {
    if value.fields.len() < objd_min_words(value.version)? {
        return Err(invalid(4, "OBJD missing version fields"));
    }
    limits.check_count(value.fields.len(), limits.max_entries, 4, "OBJD fields")?;
    if value.trailing.len() > 1 {
        return Err(invalid(
            0,
            "OBJD trailing full words must be retained in fields",
        ));
    }
    let mut w = Writer::new(limits);
    w.u32(value.version)?;
    for field in &value.fields {
        w.u16(*field)?;
    }
    w.put(&value.trailing)?;
    Ok(w.bytes)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextEncoding {
    Ascii,
    Latin1,
    Utf8,
}

/// Raw bytes retain legacy byte encodings and invalid UTF-8 on tool round trips.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyString {
    pub bytes: Vec<u8>,
    pub encoding: TextEncoding,
}

impl LegacyString {
    pub fn empty(encoding: TextEncoding) -> Self {
        Self {
            bytes: Vec::new(),
            encoding,
        }
    }
    pub fn text(&self) -> String {
        match self.encoding {
            TextEncoding::Utf8 => String::from_utf8_lossy(&self.bytes).into_owned(),
            TextEncoding::Latin1 => self.bytes.iter().map(|&b| char::from(b)).collect(),
            TextEncoding::Ascii => self
                .bytes
                .iter()
                .map(|&b| if b < 128 { char::from(b) } else { '?' })
                .collect(),
        }
    }
    pub fn from_text(text: &str, encoding: TextEncoding) -> Result<Self> {
        Limits::default().check_count(
            text.len(),
            Limits::default().max_string_bytes,
            0,
            "edited legacy string bytes",
        )?;
        let bytes = match encoding {
            TextEncoding::Utf8 => text.as_bytes().to_vec(),
            TextEncoding::Ascii | TextEncoding::Latin1 => {
                let max = if encoding == TextEncoding::Ascii {
                    127
                } else {
                    255
                };
                if text.chars().any(|c| u32::from(c) > max) {
                    return Err(invalid(0, "text cannot be represented in legacy encoding"));
                }
                text.chars().map(|c| c as u8).collect()
            }
        };
        Ok(Self { bytes, encoding })
    }
}

fn read_var_u32(r: &mut Reader<'_>) -> Result<u32> {
    let start = r.position();
    let mut value = 0u32;
    for shift in [0, 7, 14, 21, 28] {
        let next = r.u8()?;
        if shift == 28 && next & 0xf0 != 0 {
            return Err(Error::new(
                ErrorKind::Overflow,
                start,
                "32-bit variable integer",
            ));
        }
        value |= u32::from(next & 0x7f) << shift;
        if next & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(Error::new(
        ErrorKind::Overflow,
        start,
        "unterminated variable integer",
    ))
}

fn write_var_u32(w: &mut Writer<'_>, mut value: u32) -> Result<()> {
    while value >= 128 {
        w.u8((value as u8 & 127) | 128)?;
        value >>= 7;
    }
    w.u8(value as u8)
}

fn read_fixed_text(
    r: &mut Reader<'_>,
    length: usize,
    encoding: TextEncoding,
    budget: &mut Budget<'_>,
) -> Result<LegacyString> {
    budget.limits.check_count(
        length,
        budget.limits.max_string_bytes,
        r.position(),
        "legacy string bytes",
    )?;
    budget.add(length, r.position())?;
    Ok(LegacyString {
        bytes: r.read_bytes(length)?.to_vec(),
        encoding,
    })
}

fn read_var_text(r: &mut Reader<'_>, budget: &mut Budget<'_>) -> Result<LegacyString> {
    let length = read_var_u32(r)? as usize;
    if length > i32::MAX as usize {
        return Err(Error::new(
            ErrorKind::LimitExceeded,
            r.position(),
            ".NET string length",
        ));
    }
    read_fixed_text(r, length, TextEncoding::Utf8, budget)
}

fn read_c_text(
    r: &mut Reader<'_>,
    encoding: TextEncoding,
    budget: &mut Budget<'_>,
) -> Result<LegacyString> {
    let start = r.position();
    let mut length = 0usize;
    loop {
        let byte = r.u8()?;
        if byte == 0 {
            break;
        }
        length += 1;
        budget.limits.check_count(
            length,
            budget.limits.max_string_bytes,
            start,
            "NUL string bytes",
        )?;
    }
    r.seek(start)?;
    let value = read_fixed_text(r, length, encoding, budget)?;
    r.skip(1)?;
    Ok(value)
}

fn write_text(
    w: &mut Writer<'_>,
    value: &LegacyString,
    encoding: TextEncoding,
    nul: bool,
    varlen: bool,
) -> Result<()> {
    if value.encoding != encoding {
        return Err(invalid(
            w.bytes.len(),
            "string encoding does not match resource layout",
        ));
    }
    w.limits.check_count(
        value.bytes.len(),
        w.limits.max_string_bytes,
        w.bytes.len(),
        "legacy string bytes",
    )?;
    if nul && value.bytes.contains(&0) {
        return Err(invalid(
            w.bytes.len(),
            "embedded NUL in NUL-terminated text",
        ));
    }
    if varlen {
        let length = u32::try_from(value.bytes.len())
            .map_err(|_| invalid(w.bytes.len(), "string length"))?;
        if length > i32::MAX as u32 {
            return Err(Error::new(
                ErrorKind::LimitExceeded,
                w.bytes.len(),
                ".NET string length",
            ));
        }
        write_var_u32(w, length)?;
    }
    w.put(&value.bytes)?;
    if nul {
        w.u8(0)?;
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StringItem {
    pub language: u8,
    pub value: LegacyString,
    pub comment: LegacyString,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StringOrder {
    pub set: Option<usize>,
    pub index: usize,
}

/// STR# and TTAs have identical storage. Empty/absent selected sets fall back
/// as a whole to US English; missing indices in an initialized set do not.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Strings {
    pub format: i16,
    pub sets: Vec<Vec<StringItem>>,
    /// Version -3 ignores unsupported language codes at runtime; keep them raw.
    pub unassigned: Vec<StringItem>,
    /// Original -3 interleaving; positions in sets or unassigned.
    pub order: Vec<StringOrder>,
    pub trailing: Vec<u8>,
    pub header_only: bool,
}

impl Strings {
    pub fn new(format: i16) -> Self {
        Self {
            format,
            sets: vec![Vec::new(); 20],
            unassigned: Vec::new(),
            order: Vec::new(),
            trailing: Vec::new(),
            header_only: false,
        }
    }
}

pub fn decode_strings(bytes: &[u8], limits: &Limits) -> Result<Strings> {
    resource_input(bytes, limits)?;
    let mut r = Reader::new(bytes);
    let format = r.i16_le()?;
    if !(-4..=0).contains(&format) {
        return Err(Error::new(
            ErrorKind::UnsupportedVersion,
            0,
            format!("STR/TTAs {format}"),
        ));
    }
    let mut budget = Budget::new(limits);
    budget.add(20 * std::mem::size_of::<Vec<StringItem>>(), r.position())?;
    let mut result = Strings::new(format);
    if r.remaining() == 0 {
        result.header_only = true;
        return Ok(result);
    }
    if format == -4 {
        let count = usize::from(r.u8()?);
        budget.entries::<Vec<StringItem>>(count, r.position())?;
        result.sets = Vec::with_capacity(count);
        let mut total = 0usize;
        for _ in 0..count {
            let pair_count = usize::from(r.u16_le()?);
            total = total.checked_add(pair_count).ok_or_else(|| {
                Error::new(ErrorKind::Overflow, r.position(), "string pair count")
            })?;
            limits.check_count(
                total,
                limits.max_entries,
                r.position(),
                "total string pair count",
            )?;
            budget.entries::<StringItem>(pair_count, r.position())?;
            let mut set = Vec::with_capacity(pair_count);
            for _ in 0..pair_count {
                set.push(StringItem {
                    language: r.u8()?.wrapping_add(1),
                    value: read_var_text(&mut r, &mut budget)?,
                    comment: read_var_text(&mut r, &mut budget)?,
                });
            }
            result.sets.push(set);
        }
    } else {
        let count = usize::from(r.u16_le()?);
        budget.entries::<StringItem>(count, r.position())?;
        if format == -3 {
            budget.entries::<StringOrder>(count, r.position())?;
        }
        let mut parsed = Vec::with_capacity(count);
        for _ in 0..count {
            let language = if format == -3 { r.u8()? } else { 0 };
            let value = match format {
                0 => {
                    let n = usize::from(r.u8()?);
                    read_fixed_text(&mut r, n, TextEncoding::Ascii, &mut budget)?
                }
                -1 => read_c_text(&mut r, TextEncoding::Utf8, &mut budget)?,
                _ => read_c_text(&mut r, TextEncoding::Latin1, &mut budget)?,
            };
            let comment = if format <= -2 {
                read_c_text(&mut r, TextEncoding::Latin1, &mut budget)?
            } else {
                LegacyString::empty(value.encoding)
            };
            parsed.push(StringItem {
                language,
                value,
                comment,
            });
        }
        if format == -3 {
            // Count language groups before allocation: incremental pushes can
            // double retained capacity beyond a budget based only on length.
            let mut groups = [0usize; 21];
            for item in &parsed {
                groups[if item.language == 0 {
                    0
                } else {
                    (usize::from(item.language) - 1).min(20)
                }] += 1;
            }
            budget.entries::<StringItem>(count, r.position())?; // temporary + final arrays coexist
            for (set, count) in result.sets.iter_mut().zip(groups) {
                *set = Vec::with_capacity(count);
            }
            result.unassigned = Vec::with_capacity(groups[20]);
            result.order = Vec::with_capacity(count);
            for item in parsed {
                let index = if item.language == 0 {
                    0
                } else {
                    usize::from(item.language) - 1
                };
                if index < 20 {
                    result.order.push(StringOrder {
                        set: Some(index),
                        index: result.sets[index].len(),
                    });
                    result.sets[index].push(item);
                } else {
                    result.order.push(StringOrder {
                        set: None,
                        index: result.unassigned.len(),
                    });
                    result.unassigned.push(item);
                }
            }
        } else {
            result.sets[0] = parsed;
        }
    }
    result.trailing = budget.tail(&mut r)?;
    Ok(result)
}

pub fn encode_strings(value: &Strings, limits: &Limits) -> Result<Vec<u8>> {
    if !(-4..=0).contains(&value.format) {
        return Err(Error::new(
            ErrorKind::UnsupportedVersion,
            0,
            "STR/TTAs format",
        ));
    }
    limits.check_count(
        value.sets.len(),
        limits.max_entries,
        0,
        "string language sets",
    )?;
    let mut total = value.unassigned.len();
    for set in &value.sets {
        total = total
            .checked_add(set.len())
            .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "string count"))?;
    }
    limits.check_count(total, limits.max_entries, 0, "total string count")?;
    if value.format == -3 {
        for (index, set) in value.sets.iter().enumerate() {
            for item in set {
                let mapped = if item.language == 0 {
                    0
                } else {
                    usize::from(item.language) - 1
                };
                if mapped != index || mapped >= 20 {
                    return Err(invalid(0, "STR language code disagrees with its set"));
                }
            }
        }
        if value.unassigned.iter().any(|item| item.language <= 20) {
            return Err(invalid(0, "STR unassigned item has a supported language"));
        }
    }
    let mut w = Writer::new(limits);
    w.u16(value.format as u16)?;
    if value.header_only {
        if total != 0 || !value.trailing.is_empty() || !value.order.is_empty() {
            return Err(invalid(0, "nonempty header-only string table"));
        }
        return Ok(w.bytes);
    }
    if value.format == -4 {
        if !value.unassigned.is_empty() || !value.order.is_empty() {
            return Err(invalid(0, "STR -4 has no unassigned/order storage"));
        }
        w.u8(u8::try_from(value.sets.len()).map_err(|_| invalid(0, "string language set count"))?)?;
        for set in &value.sets {
            w.u16(count_u16(set.len(), limits, "STR set pair count")?)?;
            for item in set {
                w.u8(item.language.wrapping_sub(1))?;
                write_text(&mut w, &item.value, TextEncoding::Utf8, false, true)?;
                write_text(&mut w, &item.comment, TextEncoding::Utf8, false, true)?;
            }
        }
    } else {
        w.u16(count_u16(total, limits, "STR pair count")?)?;
        let mut items: Vec<&StringItem> = Vec::with_capacity(total);
        if value.format == -3 && !value.order.is_empty() {
            if value.order.len() != total {
                return Err(invalid(0, "STR original order count"));
            }
            let mut seen = std::collections::BTreeSet::new();
            for pos in &value.order {
                if !seen.insert((pos.set, pos.index)) {
                    return Err(Error::new(
                        ErrorKind::Duplicate,
                        0,
                        "STR original order position",
                    ));
                }
                let item = if let Some(set) = pos.set {
                    value.sets.get(set).and_then(|s| s.get(pos.index))
                } else {
                    value.unassigned.get(pos.index)
                }
                .ok_or_else(|| invalid(0, "STR order position out of bounds"))?;
                items.push(item);
            }
        } else {
            if value.format != -3
                && (value.sets.iter().skip(1).any(|s| !s.is_empty())
                    || !value.unassigned.is_empty())
            {
                return Err(invalid(0, "old STR version has only US English"));
            }
            for set in &value.sets {
                items.extend(set);
            }
            items.extend(&value.unassigned);
        }
        for item in items {
            if value.format == -3 {
                w.u8(item.language)?;
            }
            if value.format == 0 {
                w.u8(u8::try_from(item.value.bytes.len()).map_err(|_| {
                    Error::new(
                        ErrorKind::LimitExceeded,
                        w.bytes.len(),
                        "Pascal byte string",
                    )
                })?)?;
                write_text(&mut w, &item.value, TextEncoding::Ascii, false, false)?;
            } else {
                write_text(
                    &mut w,
                    &item.value,
                    if value.format == -1 {
                        TextEncoding::Utf8
                    } else {
                        TextEncoding::Latin1
                    },
                    true,
                    false,
                )?;
            }
            if value.format <= -2 {
                write_text(&mut w, &item.comment, TextEncoding::Latin1, true, false)?;
            } else if !item.comment.bytes.is_empty() {
                return Err(invalid(0, "STR version has no comments"));
            }
        }
    }
    w.put(&value.trailing)?;
    Ok(w.bytes)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GlobEncoding {
    Pascal,
    CString,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Glob {
    pub name: LegacyString,
    pub storage: GlobEncoding,
    pub terminated: bool,
    pub trailing: Vec<u8>,
}

pub fn decode_glob(bytes: &[u8], limits: &Limits) -> Result<Glob> {
    resource_input(bytes, limits)?;
    let mut r = Reader::new(bytes);
    let first = r.u8()?;
    let mut budget = Budget::new(limits);
    let (name, storage, terminated) = if first < 48 {
        (
            read_fixed_text(&mut r, usize::from(first), TextEncoding::Ascii, &mut budget)?,
            GlobEncoding::Pascal,
            false,
        )
    } else {
        let length = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
        r.seek(0)?;
        let name = read_fixed_text(&mut r, length, TextEncoding::Latin1, &mut budget)?;
        let terminated = r.remaining() > 0;
        if terminated {
            r.skip(1)?;
        }
        (name, GlobEncoding::CString, terminated)
    };
    Ok(Glob {
        name,
        storage,
        terminated,
        trailing: budget.tail(&mut r)?,
    })
}

pub fn encode_glob(value: &Glob, limits: &Limits) -> Result<Vec<u8>> {
    let mut w = Writer::new(limits);
    match value.storage {
        GlobEncoding::Pascal => {
            if value.name.bytes.len() >= 48 || value.terminated {
                return Err(invalid(0, "GLOB Pascal header"));
            }
            w.u8(value.name.bytes.len() as u8)?;
            write_text(&mut w, &value.name, TextEncoding::Ascii, false, false)?;
        }
        GlobEncoding::CString => {
            if value.name.bytes.first().copied().unwrap_or(0) < 48 || value.name.bytes.contains(&0)
            {
                return Err(invalid(0, "GLOB C-string first byte/NUL"));
            }
            write_text(
                &mut w,
                &value.name,
                TextEncoding::Latin1,
                value.terminated,
                false,
            )?;
            if !value.terminated && !value.trailing.is_empty() {
                return Err(invalid(0, "unterminated GLOB cannot have trailing bytes"));
            }
        }
    }
    w.put(&value.trailing)?;
    Ok(w.bytes)
}

pub use crate::sprites::{decode_slot, encode_slot, SlotItem, SlotResource};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PiffPatchMode {
    Remove,
    Add,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PiffPatch {
    pub offset: u32,
    pub size: u32,
    pub mode: PiffPatchMode,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PiffOperation {
    Patch {
        label: LegacyString,
        flags: u16,
        new_id: u16,
        new_size: u32,
        patches: Vec<PiffPatch>,
    },
    Remove,
    /// Add entries contain no payload; additions are the other IFF chunks.
    Add,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PiffEntry {
    pub kind: [u8; 4],
    pub id: u16,
    pub comment: LegacyString,
    pub operation: PiffOperation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Piff {
    pub version: u16,
    pub source: LegacyString,
    pub comment: LegacyString,
    pub entries: Vec<PiffEntry>,
    pub trailing: Vec<u8>,
}

pub fn decode_piff(bytes: &[u8], limits: &Limits) -> Result<Piff> {
    resource_input(bytes, limits)?;
    let mut r = Reader::new(bytes);
    let version = r.u16_le()?;
    if version > 2 {
        return Err(Error::new(
            ErrorKind::UnsupportedVersion,
            0,
            format!("PIFF {version}"),
        ));
    }
    let mut budget = Budget::new(limits);
    let source = read_var_text(&mut r, &mut budget)?;
    let comment = if version > 1 {
        read_var_text(&mut r, &mut budget)?
    } else {
        LegacyString::empty(TextEncoding::Utf8)
    };
    let count = usize::from(r.u16_le()?);
    budget.entries::<PiffEntry>(count, r.position())?;
    let mut entries = Vec::with_capacity(count);
    let mut operation_count = 0usize;
    for _ in 0..count {
        let kind = r
            .read_bytes(4)?
            .try_into()
            .map_err(|_| invalid(r.position(), "PIFF type"))?;
        let id = r.u16_le()?;
        let comment = if version > 1 {
            read_var_text(&mut r, &mut budget)?
        } else {
            LegacyString::empty(TextEncoding::Utf8)
        };
        let mode_offset = r.position();
        let operation = match r.u8()? {
            0 => {
                let label = read_var_text(&mut r, &mut budget)?;
                let flags = r.u16_le()?;
                let new_id = if version > 0 { r.u16_le()? } else { id };
                let new_size = r.u32_le()?;
                limits.check_count(
                    new_size as usize,
                    limits.max_resource_bytes,
                    r.position(),
                    "PIFF target resource bytes",
                )?;
                limits.check_count(
                    new_size as usize,
                    limits.max_total_decoded_bytes,
                    r.position(),
                    "PIFF target decoded bytes",
                )?;
                let count = r.u32_le()? as usize;
                operation_count = operation_count.checked_add(count).ok_or_else(|| {
                    Error::new(ErrorKind::Overflow, r.position(), "PIFF patch count")
                })?;
                limits.check_count(
                    operation_count,
                    limits.max_entries,
                    r.position(),
                    "total PIFF patches",
                )?;
                budget.entries::<PiffPatch>(count, r.position())?;
                let mut patches = Vec::with_capacity(count);
                let mut offset = 0u32;
                for _ in 0..count {
                    offset = offset.checked_add(read_var_u32(&mut r)?).ok_or_else(|| {
                        Error::new(ErrorKind::Overflow, r.position(), "PIFF delta offset")
                    })?;
                    if offset > new_size {
                        return Err(invalid(r.position(), "PIFF offset past target size"));
                    }
                    let size = read_var_u32(&mut r)?;
                    let mode_at = r.position();
                    let mode = match r.u8()? {
                        0 => PiffPatchMode::Remove,
                        1 => PiffPatchMode::Add,
                        _ => return Err(invalid(mode_at, "PIFF patch mode")),
                    };
                    let data = if mode == PiffPatchMode::Add {
                        if offset.checked_add(size).is_none_or(|end| end > new_size) {
                            return Err(invalid(mode_at, "PIFF insertion past target size"));
                        }
                        budget.add(size as usize, r.position())?;
                        r.read_bytes(size as usize)?.to_vec()
                    } else {
                        Vec::new()
                    };
                    patches.push(PiffPatch {
                        offset,
                        size,
                        mode,
                        data,
                    });
                }
                PiffOperation::Patch {
                    label,
                    flags,
                    new_id,
                    new_size,
                    patches,
                }
            }
            1 => PiffOperation::Remove,
            2 => PiffOperation::Add,
            _ => return Err(invalid(mode_offset, "PIFF entry mode")),
        };
        entries.push(PiffEntry {
            kind,
            id,
            comment,
            operation,
        });
    }
    Ok(Piff {
        version,
        source,
        comment,
        entries,
        trailing: budget.tail(&mut r)?,
    })
}

pub fn encode_piff(value: &Piff, limits: &Limits) -> Result<Vec<u8>> {
    if value.version > 2 {
        return Err(Error::new(ErrorKind::UnsupportedVersion, 0, "PIFF version"));
    }
    let count = count_u16(value.entries.len(), limits, "PIFF entry count")?;
    let mut w = Writer::new(limits);
    w.u16(value.version)?;
    write_text(&mut w, &value.source, TextEncoding::Utf8, false, true)?;
    if value.version > 1 {
        write_text(&mut w, &value.comment, TextEncoding::Utf8, false, true)?;
    } else if !value.comment.bytes.is_empty() {
        return Err(invalid(0, "old PIFF has no comment"));
    }
    w.u16(count)?;
    let mut total_patches = 0usize;
    for entry in &value.entries {
        w.put(&entry.kind)?;
        w.u16(entry.id)?;
        if value.version > 1 {
            write_text(&mut w, &entry.comment, TextEncoding::Utf8, false, true)?;
        } else if !entry.comment.bytes.is_empty() {
            return Err(invalid(0, "old PIFF entry has no comment"));
        }
        match &entry.operation {
            PiffOperation::Remove => w.u8(1)?,
            PiffOperation::Add => w.u8(2)?,
            PiffOperation::Patch {
                label,
                flags,
                new_id,
                new_size,
                patches,
            } => {
                limits.check_count(
                    *new_size as usize,
                    limits.max_resource_bytes,
                    w.bytes.len(),
                    "PIFF target resource bytes",
                )?;
                limits.check_count(
                    *new_size as usize,
                    limits.max_total_decoded_bytes,
                    w.bytes.len(),
                    "PIFF target decoded bytes",
                )?;
                total_patches = total_patches.checked_add(patches.len()).ok_or_else(|| {
                    Error::new(ErrorKind::Overflow, w.bytes.len(), "PIFF patch count")
                })?;
                limits.check_count(
                    total_patches,
                    limits.max_entries,
                    w.bytes.len(),
                    "PIFF patch count",
                )?;
                w.u8(0)?;
                write_text(&mut w, label, TextEncoding::Utf8, false, true)?;
                w.u16(*flags)?;
                if value.version > 0 {
                    w.u16(*new_id)?;
                } else if *new_id != entry.id {
                    return Err(invalid(0, "PIFF version 0 cannot move IDs"));
                }
                w.u32(*new_size)?;
                w.u32(u32::try_from(patches.len()).map_err(|_| invalid(0, "PIFF patch count"))?)?;
                let mut last = 0u32;
                for patch in patches {
                    let delta = patch
                        .offset
                        .checked_sub(last)
                        .ok_or_else(|| invalid(w.bytes.len(), "PIFF offsets must be monotonic"))?;
                    if patch.offset > *new_size {
                        return Err(invalid(w.bytes.len(), "PIFF offset past target size"));
                    }
                    write_var_u32(&mut w, delta)?;
                    write_var_u32(&mut w, patch.size)?;
                    last = patch.offset;
                    match patch.mode {
                        PiffPatchMode::Remove => {
                            if !patch.data.is_empty() {
                                return Err(invalid(w.bytes.len(), "PIFF removal has data"));
                            }
                            w.u8(0)?;
                        }
                        PiffPatchMode::Add => {
                            if patch.data.len() != patch.size as usize
                                || patch
                                    .offset
                                    .checked_add(patch.size)
                                    .is_none_or(|end| end > *new_size)
                            {
                                return Err(invalid(w.bytes.len(), "PIFF insertion size"));
                            }
                            w.u8(1)?;
                            w.put(&patch.data)?;
                        }
                    }
                }
            }
        }
    }
    w.put(&value.trailing)?;
    Ok(w.bytes)
}

/// Offsets address the output, exactly as PIFFEntry.Apply does. A reduced
/// NewDataSize can intentionally drop a source tail without a removal opcode.
pub fn apply_piff_entry(entry: &PiffEntry, source: &[u8], limits: &Limits) -> Result<Vec<u8>> {
    resource_input(source, limits)?;
    let PiffOperation::Patch {
        new_size, patches, ..
    } = &entry.operation
    else {
        return Err(invalid(0, "PIFF entry does not edit bytes"));
    };
    let target_size = *new_size as usize;
    limits.check_count(
        target_size,
        limits.max_resource_bytes,
        0,
        "PIFF result bytes",
    )?;
    limits.check_count(
        target_size,
        limits.max_total_decoded_bytes,
        0,
        "PIFF decoded result bytes",
    )?;
    limits.check_count(patches.len(), limits.max_entries, 0, "PIFF patch count")?;
    let mut result = Vec::with_capacity(target_size);
    let mut source_offset = 0usize;
    for patch in patches {
        let offset = patch.offset as usize;
        let copy = offset.checked_sub(result.len()).ok_or_else(|| {
            Error::new(
                ErrorKind::Overlap,
                offset,
                "PIFF patch overlaps emitted output",
            )
        })?;
        if offset > target_size {
            return Err(invalid(offset, "PIFF output offset"));
        }
        let end = source_offset
            .checked_add(copy)
            .ok_or_else(|| Error::new(ErrorKind::Overflow, offset, "PIFF source copy"))?;
        result.extend_from_slice(
            source.get(source_offset..end).ok_or_else(|| {
                Error::new(ErrorKind::Truncated, source_offset, "PIFF source copy")
            })?,
        );
        source_offset = end;
        match patch.mode {
            PiffPatchMode::Add => {
                if patch.data.len() != patch.size as usize {
                    return Err(invalid(offset, "PIFF insertion payload size"));
                }
                let end = result
                    .len()
                    .checked_add(patch.data.len())
                    .ok_or_else(|| Error::new(ErrorKind::Overflow, offset, "PIFF output size"))?;
                if end > target_size {
                    return Err(invalid(offset, "PIFF insertion past result"));
                }
                result.extend_from_slice(&patch.data);
            }
            PiffPatchMode::Remove => {
                if !patch.data.is_empty() {
                    return Err(invalid(offset, "PIFF removal has payload"));
                }
                source_offset = source_offset
                    .checked_add(patch.size as usize)
                    .ok_or_else(|| Error::new(ErrorKind::Overflow, offset, "PIFF removal"))?;
                if source_offset > source.len() {
                    return Err(Error::new(
                        ErrorKind::Truncated,
                        source_offset,
                        "PIFF removal past source",
                    ));
                }
            }
        }
    }
    let rest = target_size
        .checked_sub(result.len())
        .ok_or_else(|| invalid(result.len(), "PIFF result size"))?;
    let end = source_offset
        .checked_add(rest)
        .ok_or_else(|| Error::new(ErrorKind::Overflow, source_offset, "PIFF remainder"))?;
    result.extend_from_slice(source.get(source_offset..end).ok_or_else(|| {
        Error::new(
            ErrorKind::Truncated,
            source_offset,
            "PIFF remainder past source",
        )
    })?);
    Ok(result)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OtfKey {
    pub id: i32,
    pub label: String,
    pub value: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OtfTable {
    pub id: i32,
    pub name: String,
    pub keys: Vec<OtfKey>,
}

/// The XML is retained because unknown elements/attributes/comments are part of
/// the original asset. The encoder edits attribute spans without rebuilding it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Otf {
    pub tables: Vec<OtfTable>,
    pub raw_xml: Vec<u8>,
}

#[derive(Clone)]
struct XmlAttribute {
    value: String,
    range: std::ops::Range<usize>,
}

struct OtfKeySpans {
    id: std::ops::Range<usize>,
    label: std::ops::Range<usize>,
    value: std::ops::Range<usize>,
}
struct OtfTableSpans {
    id: std::ops::Range<usize>,
    name: std::ops::Range<usize>,
    keys: Vec<OtfKeySpans>,
}

fn xml_attributes(
    tag: &quick_xml::events::BytesStart<'_>,
    raw_start: usize,
    budget: &mut Budget<'_>,
) -> Result<std::collections::BTreeMap<String, XmlAttribute>> {
    let mut attrs = std::collections::BTreeMap::new();
    let raw = tag.as_ref();
    let mut cursor = tag.name().as_ref().len();
    for (count, attr) in tag.attributes().enumerate() {
        budget.limits.check_count(
            count + 1,
            budget.limits.max_entries,
            raw_start,
            "XML attributes",
        )?;
        let attr = attr.map_err(|e| invalid(raw_start, format!("XML attribute: {e}")))?;
        budget.limits.check_count(
            attr.key.as_ref().len(),
            budget.limits.max_string_bytes,
            raw_start,
            "XML attribute name",
        )?;
        budget.limits.check_count(
            attr.value.len(),
            budget.limits.max_string_bytes,
            raw_start,
            "XML attribute value",
        )?;
        while raw.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        while raw
            .get(cursor)
            .is_some_and(|b| !b.is_ascii_whitespace() && *b != b'=')
        {
            cursor += 1;
        }
        while raw.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        if raw.get(cursor) != Some(&b'=') {
            return Err(invalid(raw_start + cursor, "XML attribute separator"));
        }
        cursor += 1;
        while raw.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        let quote = *raw
            .get(cursor)
            .ok_or_else(|| invalid(raw_start + cursor, "XML attribute quote"))?;
        if quote != b'\'' && quote != b'"' {
            return Err(invalid(raw_start + cursor, "XML attribute quote"));
        }
        cursor += 1;
        let begin = cursor;
        while raw.get(cursor).is_some_and(|b| *b != quote) {
            cursor += 1;
        }
        if cursor == raw.len() {
            return Err(invalid(raw_start + cursor, "XML attribute terminator"));
        }
        let end = cursor;
        cursor += 1;
        budget.add(
            attr.key.as_ref().len() + attr.value.len() * 2 + 512,
            raw_start,
        )?;
        let name = std::str::from_utf8(attr.key.as_ref())
            .map_err(|_| invalid(raw_start, "OTF XML must be UTF-8"))?
            .to_owned();
        let text = std::str::from_utf8(attr.value.as_ref())
            .map_err(|_| invalid(raw_start, "OTF XML must be UTF-8"))?;
        // XML attribute normalization precedes character-reference expansion.
        let normalized = text
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .replace(['\n', '\t'], " ");
        let value = quick_xml::escape::unescape(&normalized)
            .map_err(|e| invalid(raw_start, format!("XML entity: {e}")))?
            .into_owned();
        budget.limits.check_count(
            value.len(),
            budget.limits.max_string_bytes,
            raw_start,
            "decoded XML value",
        )?;
        if attrs
            .insert(
                name,
                XmlAttribute {
                    value,
                    range: raw_start + begin..raw_start + end,
                },
            )
            .is_some()
        {
            return Err(Error::new(ErrorKind::Duplicate, raw_start, "XML attribute"));
        }
    }
    Ok(attrs)
}

fn required_attr(
    attrs: &std::collections::BTreeMap<String, XmlAttribute>,
    name: &str,
    offset: usize,
) -> Result<XmlAttribute> {
    attrs
        .get(name)
        .cloned()
        .ok_or_else(|| invalid(offset, format!("OTF missing attribute {name}")))
}

fn attr_i32(attr: &XmlAttribute) -> Result<i32> {
    attr.value
        .trim()
        .parse()
        .map_err(|_| invalid(attr.range.start, "OTF integer attribute"))
}

fn parse_otf(bytes: &[u8], limits: &Limits) -> Result<(Vec<OtfTable>, Vec<OtfTableSpans>)> {
    use quick_xml::events::Event;
    resource_input(bytes, limits)?;
    std::str::from_utf8(bytes)
        .map_err(|_| Error::new(ErrorKind::UnsupportedVersion, 0, "OTF supports UTF-8 XML"))?;
    let mut reader = quick_xml::Reader::from_reader(bytes);
    reader.config_mut().check_end_names = true;
    let mut budget = Budget::new(limits);
    budget.add(bytes.len(), 0)?; // original XML retained by decode_otf
    let mut stack: Vec<Option<usize>> = Vec::new();
    let mut tables = Vec::new();
    let mut spans: Vec<OtfTableSpans> = Vec::new();
    let mut elements = 0usize;
    let mut total_keys = 0usize;
    let mut roots = 0usize;
    loop {
        let offset = reader.buffer_position() as usize;
        let event = reader
            .read_event()
            .map_err(|e| invalid(offset, format!("OTF XML: {e}")))?;
        match event {
            Event::DocType(_) => {
                return Err(invalid(
                    offset,
                    "OTF DTD and external entities are forbidden",
                ))
            }
            Event::GeneralRef(reference) => {
                limits.check_count(
                    reference.as_ref().len(),
                    limits.max_string_bytes,
                    offset,
                    "XML character reference",
                )?;
                if stack.is_empty() || stack.last().is_some_and(Option::is_some) {
                    return Err(invalid(offset, "OTF table text is not a key"));
                }
                let name = std::str::from_utf8(reference.as_ref())
                    .map_err(|_| invalid(offset, "XML reference encoding"))?;
                quick_xml::escape::unescape(&format!("&{name};"))
                    .map_err(|_| invalid(offset, "OTF unknown general entity"))?;
            }
            Event::Start(ref tag) | Event::Empty(ref tag) => {
                let empty = matches!(event, Event::Empty(_));
                elements = elements
                    .checked_add(1)
                    .ok_or_else(|| Error::new(ErrorKind::Overflow, offset, "XML element count"))?;
                limits.check_count(elements, limits.max_entries, offset, "XML elements")?;
                limits.check_count(stack.len() + 1, limits.max_depth, offset, "XML depth")?;
                if stack.is_empty() {
                    roots += 1;
                    if roots > 1 {
                        return Err(invalid(offset, "OTF multiple XML roots"));
                    }
                }
                let raw_start = (reader.buffer_position() as usize)
                    .checked_sub(tag.as_ref().len() + if empty { 2 } else { 1 })
                    .ok_or_else(|| invalid(offset, "XML tag range"))?;
                let attrs = xml_attributes(tag, raw_start, &mut budget)?;
                let name = tag.name();
                let mut table_marker = None;
                if name.as_ref() == b"T" {
                    if stack.iter().any(Option::is_some) {
                        return Err(Error::new(
                            ErrorKind::UnsupportedVersion,
                            offset,
                            "nested OTF T elements",
                        ));
                    }
                    let id = required_attr(&attrs, "i", offset)?;
                    let label = required_attr(&attrs, "n", offset)?;
                    budget.reserve_one(&mut tables, offset)?;
                    budget.reserve_one(&mut spans, offset)?;
                    table_marker = Some(tables.len());
                    tables.push(OtfTable {
                        id: attr_i32(&id)?,
                        name: label.value,
                        keys: Vec::new(),
                    });
                    spans.push(OtfTableSpans {
                        id: id.range,
                        name: label.range,
                        keys: Vec::new(),
                    });
                } else if let Some(Some(table)) = stack.last() {
                    let id = required_attr(&attrs, "i", offset)?;
                    let label = required_attr(&attrs, "l", offset)?;
                    let value = required_attr(&attrs, "v", offset)?;
                    total_keys = total_keys
                        .checked_add(1)
                        .ok_or_else(|| Error::new(ErrorKind::Overflow, offset, "OTF key count"))?;
                    limits.check_count(total_keys, limits.max_entries, offset, "OTF keys")?;
                    budget.reserve_one(&mut tables[*table].keys, offset)?;
                    budget.reserve_one(&mut spans[*table].keys, offset)?;
                    tables[*table].keys.push(OtfKey {
                        id: attr_i32(&id)?,
                        label: label.value,
                        value: attr_i32(&value)?,
                    });
                    spans[*table].keys.push(OtfKeySpans {
                        id: id.range,
                        label: label.range,
                        value: value.range,
                    });
                }
                if !empty {
                    budget.reserve_one(&mut stack, offset)?;
                    stack.push(table_marker);
                }
            }
            Event::End(_) => {
                if stack.pop().is_none() {
                    return Err(invalid(offset, "OTF unmatched closing element"));
                }
            }
            Event::Text(text) => {
                if (stack.is_empty() || stack.last().is_some_and(Option::is_some))
                    && text.as_ref().iter().any(|b| !b.is_ascii_whitespace())
                {
                    return Err(invalid(offset, "OTF unexpected text"));
                }
            }
            Event::CData(_) if stack.last().is_some_and(Option::is_some) => {
                return Err(invalid(offset, "OTF table CDATA is not a key"))
            }
            Event::Decl(decl) => {
                if let Some(encoding) = decl.encoding() {
                    let encoding =
                        encoding.map_err(|e| invalid(offset, format!("XML encoding: {e}")))?;
                    if !encoding.eq_ignore_ascii_case(b"utf-8")
                        && !encoding.eq_ignore_ascii_case(b"us-ascii")
                    {
                        return Err(Error::new(
                            ErrorKind::UnsupportedVersion,
                            offset,
                            "OTF supports UTF-8 XML",
                        ));
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if roots != 1 || !stack.is_empty() {
        return Err(invalid(bytes.len(), "OTF incomplete XML document"));
    }
    Ok((tables, spans))
}

pub fn decode_otf(bytes: &[u8], limits: &Limits) -> Result<Otf> {
    let (tables, _) = parse_otf(bytes, limits)?;
    Ok(Otf {
        tables,
        raw_xml: bytes.to_vec(),
    })
}

fn escaped_xml_len(value: &str, limits: &Limits) -> Result<usize> {
    limits.check_count(
        value.len(),
        limits.max_string_bytes,
        0,
        "OTF edited attribute",
    )?;
    let mut size = 0usize;
    for ch in value.chars() {
        let len = match ch {
            '&' => 5,
            '<' | '>' => 4,
            '"' | '\'' => 6,
            '\n' | '\r' | '\t' => 5,
            ch if (ch as u32) < 32 || ch == '\u{fffe}' || ch == '\u{ffff}' => {
                return Err(invalid(0, "invalid XML character"))
            }
            ch => ch.len_utf8(),
        };
        size = size
            .checked_add(len)
            .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "escaped XML attribute size"))?;
    }
    limits.check_count(
        size,
        limits.max_resource_bytes,
        0,
        "escaped XML attribute bytes",
    )?;
    limits.check_count(
        size,
        limits.max_total_decoded_bytes,
        0,
        "escaped XML attribute budget",
    )?;
    Ok(size)
}

fn escaped_xml(value: &str, limits: &Limits) -> Result<Vec<u8>> {
    let size = escaped_xml_len(value, limits)?;
    let mut writer = Writer {
        bytes: Vec::with_capacity(size),
        limits,
    };
    for ch in value.chars() {
        match ch {
            '&' => writer.put(b"&amp;")?,
            '<' => writer.put(b"&lt;")?,
            '>' => writer.put(b"&gt;")?,
            '"' => writer.put(b"&quot;")?,
            '\'' => writer.put(b"&apos;")?,
            '\n' => writer.put(b"&#10;")?,
            '\r' => writer.put(b"&#13;")?,
            '\t' => writer.put(b"&#9;")?,
            ch if (ch as u32) < 32 || ch == '\u{fffe}' || ch == '\u{ffff}' => {
                return Err(invalid(0, "invalid XML character"))
            }
            ch => {
                let mut buffer = [0u8; 4];
                writer.put(ch.encode_utf8(&mut buffer).as_bytes())?;
            }
        }
    }
    Ok(writer.bytes)
}

fn add_otf_edit(
    replacements: &mut Vec<(std::ops::Range<usize>, Vec<u8>)>,
    range: std::ops::Range<usize>,
    value: &str,
    budget: &mut Budget<'_>,
    output_size: &mut usize,
) -> Result<()> {
    let size = escaped_xml_len(value, budget.limits)?;
    *output_size = output_size
        .checked_sub(range.end - range.start)
        .and_then(|n| n.checked_add(size))
        .ok_or_else(|| Error::new(ErrorKind::Overflow, range.start, "OTF edited output size"))?;
    budget.limits.check_count(
        *output_size,
        budget.limits.max_resource_bytes,
        range.start,
        "OTF edited output bytes",
    )?;
    budget.limits.check_count(
        *output_size,
        budget.limits.max_total_decoded_bytes,
        range.start,
        "OTF edited output budget",
    )?;
    budget.reserve_one(replacements, range.start)?;
    budget.add(size, range.start)?; // aggregate bound before escaping or allocation
    replacements.push((range, escaped_xml(value, budget.limits)?));
    Ok(())
}

/// Editing IDs/names/labels/values preserves every other source XML byte.
/// Structural table/key edits require a deliberate new OTF, rather than
/// silently dropping unknown XML or changing the source child-node layout.
pub fn encode_otf(value: &Otf, limits: &Limits) -> Result<Vec<u8>> {
    let (original, spans) = parse_otf(&value.raw_xml, limits)?;
    if original.len() != value.tables.len() {
        return Err(Error::new(
            ErrorKind::UnsupportedVersion,
            0,
            "structural OTF table edits require a new XML document",
        ));
    }
    let mut replacements: Vec<(std::ops::Range<usize>, Vec<u8>)> = Vec::new();
    let mut edit_budget = Budget::new(limits);
    edit_budget.add(value.raw_xml.len(), 0)?;
    let mut output_size = value.raw_xml.len();
    for ((old, new), spans) in original.iter().zip(&value.tables).zip(spans) {
        if old.keys.len() != new.keys.len() {
            return Err(Error::new(
                ErrorKind::UnsupportedVersion,
                0,
                "structural OTF key edits require a new XML document",
            ));
        }
        if old.id != new.id {
            add_otf_edit(
                &mut replacements,
                spans.id,
                &new.id.to_string(),
                &mut edit_budget,
                &mut output_size,
            )?;
        }
        if old.name != new.name {
            add_otf_edit(
                &mut replacements,
                spans.name,
                &new.name,
                &mut edit_budget,
                &mut output_size,
            )?;
        }
        for ((old, new), key_spans) in old.keys.iter().zip(&new.keys).zip(spans.keys) {
            if old.id != new.id {
                add_otf_edit(
                    &mut replacements,
                    key_spans.id,
                    &new.id.to_string(),
                    &mut edit_budget,
                    &mut output_size,
                )?;
            }
            if old.label != new.label {
                add_otf_edit(
                    &mut replacements,
                    key_spans.label,
                    &new.label,
                    &mut edit_budget,
                    &mut output_size,
                )?;
            }
            if old.value != new.value {
                add_otf_edit(
                    &mut replacements,
                    key_spans.value,
                    &new.value.to_string(),
                    &mut edit_budget,
                    &mut output_size,
                )?;
            }
        }
    }
    replacements.sort_by_key(|(range, _)| range.start);
    let output_limits = Limits {
        max_total_decoded_bytes: limits
            .max_total_decoded_bytes
            .checked_sub(edit_budget.used)
            .ok_or_else(|| Error::new(ErrorKind::LimitExceeded, 0, "OTF editing working budget"))?,
        ..*limits
    };
    output_limits.check_count(
        output_size,
        output_limits.max_total_decoded_bytes,
        0,
        "OTF editing output plus retained edits",
    )?;
    let mut writer = Writer {
        bytes: Vec::with_capacity(output_size),
        limits: &output_limits,
    };
    let mut cursor = 0usize;
    for (range, bytes) in replacements {
        writer.put(
            value
                .raw_xml
                .get(cursor..range.start)
                .ok_or_else(|| invalid(cursor, "OTF attribute overlap"))?,
        )?;
        writer.put(&bytes)?;
        cursor = range.end;
    }
    writer.put(&value.raw_xml[cursor..])?;
    // Recheck edited XML, including decoded strings and total budgets.
    parse_otf(&writer.bytes, limits)?;
    Ok(writer.bytes)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TtabVariant {
    Standard,
    Tsbo,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TtabMotive {
    pub minimum: i16,
    pub delta: i16,
    pub personality_modifier: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TtabInteraction {
    pub action_function: u16,
    pub test_function: u16,
    pub motives: Vec<TtabMotive>,
    pub flags: u32,
    pub string_index: u32,
    pub attenuation_code: u32,
    pub attenuation_value_bits: u32,
    pub autonomy_threshold: u32,
    pub joining_index: i32,
    pub flags2: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ttab {
    /// Source returns immediately for a zero count, without reading a version.
    pub version: Option<u16>,
    pub variant: TtabVariant,
    pub compression_code: Option<u8>,
    pub interactions: Vec<TtabInteraction>,
    pub padding_bits: u8,
    pub trailing: Vec<u8>,
    #[serde(skip)]
    original: Option<Vec<u8>>,
}

struct FieldInput<'a> {
    reader: Reader<'a>,
    compressed: bool,
    current: u8,
    remaining_bits: u8,
}

impl FieldInput<'_> {
    fn bits(&mut self, count: u8) -> Result<u32> {
        let mut value = 0u32;
        for _ in 0..count {
            if self.remaining_bits == 0 {
                self.current = self.reader.u8()?;
                self.remaining_bits = 8;
            }
            self.remaining_bits -= 1;
            value = (value << 1) | u32::from((self.current >> self.remaining_bits) & 1);
        }
        Ok(value)
    }
    fn field(&mut self, widths: [u8; 4]) -> Result<u32> {
        if self.bits(1)? == 0 {
            return Ok(0);
        }
        let width = widths[self.bits(2)? as usize];
        let raw = self.bits(width)?;
        Ok(if width < 32 && raw & (1 << (width - 1)) != 0 {
            raw | (u32::MAX << width)
        } else {
            raw
        })
    }
    fn u16(&mut self) -> Result<u16> {
        if self.compressed {
            Ok(self.field([5, 8, 13, 16])? as u16)
        } else {
            self.reader.u16_le()
        }
    }
    fn u32(&mut self) -> Result<u32> {
        if self.compressed {
            self.field([6, 11, 21, 32])
        } else {
            self.reader.u32_le()
        }
    }
}

struct FieldOutput<'a> {
    writer: Writer<'a>,
    compressed: bool,
    current: u8,
    used_bits: u8,
}

impl FieldOutput<'_> {
    fn bits(&mut self, value: u32, count: u8) -> Result<()> {
        for bit in (0..count).rev() {
            self.current = (self.current << 1) | ((value >> bit) & 1) as u8;
            self.used_bits += 1;
            if self.used_bits == 8 {
                self.writer.u8(self.current)?;
                self.current = 0;
                self.used_bits = 0;
            }
        }
        Ok(())
    }
    fn field(&mut self, signed: i64, widths: [u8; 4]) -> Result<()> {
        if signed == 0 {
            return self.bits(0, 1);
        }
        let code = widths
            .iter()
            .position(|&w| signed >= -(1i64 << (w - 1)) && signed < (1i64 << (w - 1)))
            .ok_or_else(|| invalid(0, "TTAB signed field width"))?;
        self.bits(1, 1)?;
        self.bits(code as u32, 2)?;
        self.bits(signed as u32, widths[code])
    }
    fn u16(&mut self, v: u16) -> Result<()> {
        if self.compressed {
            self.field(i64::from(v as i16), [5, 8, 13, 16])
        } else {
            self.writer.u16(v)
        }
    }
    fn u32(&mut self, v: u32) -> Result<()> {
        if self.compressed {
            self.field(i64::from(v as i32), [6, 11, 21, 32])
        } else {
            self.writer.u32(v)
        }
    }
}

pub fn decode_ttab(bytes: &[u8], limits: &Limits) -> Result<Ttab> {
    decode_ttab_with_variant(bytes, TtabVariant::Standard, limits)
}

pub fn decode_ttab_with_variant(
    bytes: &[u8],
    variant: TtabVariant,
    limits: &Limits,
) -> Result<Ttab> {
    resource_input(bytes, limits)?;
    let mut reader = Reader::new(bytes);
    let count = usize::from(reader.u16_le()?);
    let mut budget = Budget::new(limits);
    budget.add(bytes.len(), 0)?; // original field-width/padding choices survive unchanged encodes
    budget.entries::<TtabInteraction>(count, 0)?;
    if count == 0 {
        return Ok(Ttab {
            version: None,
            variant,
            compression_code: None,
            interactions: Vec::new(),
            padding_bits: 0,
            trailing: budget.tail(&mut reader)?,
            original: Some(bytes.to_vec()),
        });
    }
    let version = reader.u16_le()?;
    if version <= 3 {
        return Err(Error::new(
            ErrorKind::UnsupportedVersion,
            2,
            format!("TTAB {version}"),
        ));
    }
    // TTAB.Read: versions above 10 are normal fields in the standard
    // baseline, but still carry a compression selector for the TSBO variant.
    let has_compression_code =
        (9..=10).contains(&version) || (version > 10 && variant == TtabVariant::Tsbo);
    let compression_code = if has_compression_code {
        Some(reader.u8()?)
    } else {
        None
    };
    let mut input = FieldInput {
        reader,
        compressed: compression_code == Some(1),
        current: 0,
        remaining_bits: 0,
    };
    let mut interactions = Vec::with_capacity(count);
    let mut total_motives = 0usize;
    let mut indices = std::collections::BTreeSet::new();
    for _ in 0..count {
        let action_function = input.u16()?;
        let test_function = input.u16()?;
        let motive_count = input.u32()? as usize;
        total_motives = total_motives.checked_add(motive_count).ok_or_else(|| {
            Error::new(ErrorKind::Overflow, input.reader.position(), "TTAB motives")
        })?;
        limits.check_count(
            total_motives,
            limits.max_entries,
            input.reader.position(),
            "total TTAB motives",
        )?;
        budget.entries::<TtabMotive>(motive_count, input.reader.position())?;
        let flags = input.u32()?;
        let string_index = input.u32()?;
        if !indices.insert(string_index) {
            return Err(Error::new(
                ErrorKind::Duplicate,
                input.reader.position(),
                "TTAB pie-menu index",
            ));
        }
        let attenuation_code = if version > 6 { input.u32()? } else { 0 };
        let attenuation_value_bits = input.u32()?;
        let autonomy_threshold = input.u32()?;
        let joining_index = input.u32()? as i32;
        let mut motives = Vec::with_capacity(motive_count);
        for _ in 0..motive_count {
            motives.push(TtabMotive {
                minimum: if version > 6 { input.u16()? as i16 } else { 0 },
                delta: input.u16()? as i16,
                personality_modifier: if version > 6 { input.u16()? } else { 0 },
            });
        }
        let flags2 = if version > 9 && variant == TtabVariant::Standard {
            input.u32()?
        } else {
            0x1e
        };
        interactions.push(TtabInteraction {
            action_function,
            test_function,
            motives,
            flags,
            string_index,
            attenuation_code,
            attenuation_value_bits,
            autonomy_threshold,
            joining_index,
            flags2,
        });
    }
    let padding_bits = if input.remaining_bits == 0 {
        0
    } else {
        input.current & ((1u8 << input.remaining_bits) - 1)
    };
    Ok(Ttab {
        version: Some(version),
        variant,
        compression_code,
        interactions,
        padding_bits,
        trailing: budget.tail(&mut input.reader)?,
        original: Some(bytes.to_vec()),
    })
}

pub fn encode_ttab(value: &Ttab, limits: &Limits) -> Result<Vec<u8>> {
    let count = count_u16(value.interactions.len(), limits, "TTAB interaction count")?;
    if let Some(raw) = &value.original {
        let original = decode_ttab_with_variant(raw, value.variant, limits)?;
        if original.version == value.version
            && original.compression_code == value.compression_code
            && original.interactions == value.interactions
            && original.padding_bits == value.padding_bits
            && original.trailing == value.trailing
        {
            return Ok(raw.clone());
        }
    }
    let mut writer = Writer::new(limits);
    writer.u16(count)?;
    if count == 0 {
        if value.version.is_some() || value.compression_code.is_some() {
            return Err(invalid(0, "empty TTAB has no parsed version"));
        }
        writer.put(&value.trailing)?;
        return Ok(writer.bytes);
    }
    let version = value
        .version
        .ok_or_else(|| invalid(0, "TTAB missing version"))?;
    if version <= 3 {
        return Err(Error::new(ErrorKind::UnsupportedVersion, 2, "TTAB version"));
    }
    writer.u16(version)?;
    let has_compression_code =
        (9..=10).contains(&version) || (version > 10 && value.variant == TtabVariant::Tsbo);
    if has_compression_code {
        writer.u8(value
            .compression_code
            .ok_or_else(|| invalid(4, "TTAB missing compression code"))?)?;
    } else if value.compression_code.is_some() {
        return Err(invalid(4, "TTAB layout has no compression code"));
    }
    let mut output = FieldOutput {
        writer,
        compressed: value.compression_code == Some(1),
        current: 0,
        used_bits: 0,
    };
    let mut indices = std::collections::BTreeSet::new();
    let mut total_motives = 0usize;
    for item in &value.interactions {
        if !indices.insert(item.string_index) {
            return Err(Error::new(ErrorKind::Duplicate, 0, "TTAB pie-menu index"));
        }
        total_motives = total_motives
            .checked_add(item.motives.len())
            .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "TTAB motives"))?;
        limits.check_count(total_motives, limits.max_entries, 0, "total TTAB motives")?;
        output.u16(item.action_function)?;
        output.u16(item.test_function)?;
        output.u32(item.motives.len() as u32)?;
        output.u32(item.flags)?;
        output.u32(item.string_index)?;
        if version > 6 {
            output.u32(item.attenuation_code)?;
        } else if item.attenuation_code != 0 {
            return Err(invalid(0, "old TTAB has no attenuation code"));
        }
        output.u32(item.attenuation_value_bits)?;
        output.u32(item.autonomy_threshold)?;
        output.u32(item.joining_index as u32)?;
        for motive in &item.motives {
            if version > 6 {
                output.u16(motive.minimum as u16)?;
            } else if motive.minimum != 0 || motive.personality_modifier != 0 {
                return Err(invalid(0, "old TTAB motive fields"));
            }
            output.u16(motive.delta as u16)?;
            if version > 6 {
                output.u16(motive.personality_modifier)?;
            }
        }
        if version > 9 && value.variant == TtabVariant::Standard {
            output.u32(item.flags2)?;
        } else if item.flags2 != 0x1e {
            return Err(invalid(0, "TTAB variant has no Flags2"));
        }
    }
    if output.used_bits != 0 {
        let remaining = 8 - output.used_bits;
        let mask = (1u8 << remaining) - 1;
        output
            .writer
            .u8((output.current << remaining) | (value.padding_bits & mask))?;
    }
    output.writer.put(&value.trailing)?;
    Ok(output.writer.bytes)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecodedSemantic {
    Bhav(Bhav),
    Objd(Objd),
    Ttab(Ttab),
    Bcon(Bcon),
    Strings(Strings),
    Glob(Glob),
    Slot(SlotResource),
    Piff(Piff),
    Unknown { kind: [u8; 4] },
    Objf(Objf),
}

impl DecodedSemantic {
    /// Retained dynamic storage, using capacities rather than lengths. The
    /// containing enum/entry arrays are charged by their owner separately.
    pub fn retained_heap_bytes(&self) -> Result<usize> {
        fn add(total: &mut usize, bytes: usize) -> Result<()> {
            *total = total
                .checked_add(bytes)
                .ok_or_else(|| Error::new(ErrorKind::Overflow, 0, "semantic retained bytes"))?;
            Ok(())
        }
        fn vector<T>(total: &mut usize, value: &Vec<T>) -> Result<()> {
            add(
                total,
                value
                    .capacity()
                    .checked_mul(std::mem::size_of::<T>())
                    .ok_or_else(|| {
                        Error::new(ErrorKind::Overflow, 0, "semantic retained vector")
                    })?,
            )
        }
        fn string_items(total: &mut usize, items: &Vec<StringItem>) -> Result<()> {
            vector(total, items)?;
            for item in items {
                vector(total, &item.value.bytes)?;
                vector(total, &item.comment.bytes)?;
            }
            Ok(())
        }
        let mut bytes = 0usize;
        match self {
            Self::Bhav(v) => {
                vector(&mut bytes, &v.reserved)?;
                vector(&mut bytes, &v.instructions)?;
                vector(&mut bytes, &v.trailing)?;
            }
            Self::Objd(v) => {
                vector(&mut bytes, &v.fields)?;
                vector(&mut bytes, &v.trailing)?;
            }
            Self::Objf(v) => {
                vector(&mut bytes, &v.functions)?;
                vector(&mut bytes, &v.trailing)?;
            }
            Self::Bcon(v) => {
                vector(&mut bytes, &v.constants)?;
                vector(&mut bytes, &v.trailing)?;
            }
            Self::Strings(v) => {
                vector(&mut bytes, &v.sets)?;
                vector(&mut bytes, &v.order)?;
                vector(&mut bytes, &v.trailing)?;
                string_items(&mut bytes, &v.unassigned)?;
                for set in &v.sets {
                    string_items(&mut bytes, set)?;
                }
            }
            Self::Glob(v) => {
                vector(&mut bytes, &v.name.bytes)?;
                vector(&mut bytes, &v.trailing)?;
            }
            Self::Slot(v) => {
                vector(&mut bytes, &v.slots)?;
                vector(&mut bytes, &v.trailing)?;
            }
            Self::Ttab(v) => {
                vector(&mut bytes, &v.interactions)?;
                vector(&mut bytes, &v.trailing)?;
                for interaction in &v.interactions {
                    vector(&mut bytes, &interaction.motives)?;
                }
                if let Some(raw) = &v.original {
                    vector(&mut bytes, raw)?;
                }
            }
            Self::Piff(v) => {
                vector(&mut bytes, &v.source.bytes)?;
                vector(&mut bytes, &v.comment.bytes)?;
                vector(&mut bytes, &v.entries)?;
                vector(&mut bytes, &v.trailing)?;
                for entry in &v.entries {
                    vector(&mut bytes, &entry.comment.bytes)?;
                    if let PiffOperation::Patch { label, patches, .. } = &entry.operation {
                        vector(&mut bytes, &label.bytes)?;
                        vector(&mut bytes, patches)?;
                        for patch in patches {
                            vector(&mut bytes, &patch.data)?;
                        }
                    }
                }
            }
            Self::Unknown { .. } => {}
        }
        Ok(bytes)
    }
}

pub fn decode_semantic(chunk: &crate::iff::IffChunk, limits: &Limits) -> Result<DecodedSemantic> {
    let data = &chunk.data;
    match &chunk.key.kind {
        b"BHAV" => Ok(DecodedSemantic::Bhav(decode_bhav(data, limits)?)),
        b"OBJD" => Ok(DecodedSemantic::Objd(decode_objd(data, limits)?)),
        b"OBJf" => Ok(DecodedSemantic::Objf(decode_objf(data, limits)?)),
        b"TTAB" => Ok(DecodedSemantic::Ttab(decode_ttab(data, limits)?)),
        b"BCON" => Ok(DecodedSemantic::Bcon(decode_bcon(data, limits)?)),
        b"STR#" | b"TTAs" | b"CTSS" => Ok(DecodedSemantic::Strings(decode_strings(data, limits)?)),
        b"GLOB" => Ok(DecodedSemantic::Glob(decode_glob(data, limits)?)),
        b"SLOT" => Ok(DecodedSemantic::Slot(decode_slot(data, limits)?)),
        b"PIFF" => Ok(DecodedSemantic::Piff(decode_piff(data, limits)?)),
        _ => {
            resource_input(data, limits)?;
            Ok(DecodedSemantic::Unknown {
                kind: chunk.key.kind,
            })
        }
    }
}
