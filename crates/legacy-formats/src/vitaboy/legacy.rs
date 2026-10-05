// SPDX-License-Identifier: MPL-2.0
//! Original TS1 BCF/CMX, BMF/SKN and CFP layouts. Sources: BCF.cs, CFP.cs,
//! Animation.cs, Skeleton.cs, Mesh.cs, Appearance.cs and BCFReadProxy.cs at
//! the pinned original revision. Binary integers are little-endian here.
//! Text output is canonical; preserve original text bytes for a lexical no-op.

use super::encode::{encode, measure, Writer};
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LegacyEncoding {
    Binary,
    Text,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LegacyTextPolicy {
    /// Reject values that the original framework's text reader cannot retain.
    Exact,
    /// Convert wire negative zero to positive zero and report every conversion.
    NormalizeSignedZero,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyTextOutput {
    pub bytes: Vec<u8>,
    pub normalized_signed_zeros: usize,
}

struct Input<'a> {
    binary: Reader<'a>,
    text: Option<&'a str>,
    text_offset: usize,
    numbers: &'a str,
    max_numeric_bytes: usize,
}
impl<'a> Input<'a> {
    fn new(
        bytes: &'a [u8],
        encoding: LegacyEncoding,
        budget: &mut DecodeBudget<'_>,
    ) -> Result<Self> {
        let text = if encoding == LegacyEncoding::Text {
            // A numeric token containing invariant thousands separators needs
            // one temporary copy. Admit its maximum possible size once before
            // parsing any nested object, rather than allocate outside budget.
            budget.reserve::<u8>(
                bytes.len().min(budget.limits.max_string_bytes),
                budget.limits.max_string_bytes,
                0,
                "legacy numeric scratch",
            )?;
            Some(
                std::str::from_utf8(bytes)
                    .map_err(|_| unsupported(0, "legacy text requires UTF-8"))?
                    .trim_start_matches('\u{feff}'),
            )
        } else {
            None
        };
        Ok(Self {
            binary: Reader::new(bytes),
            text,
            text_offset: 0,
            numbers: "",
            max_numeric_bytes: budget.limits.max_string_bytes,
        })
    }
    fn line(&mut self) -> Result<&'a str> {
        let text = self
            .text
            .ok_or_else(|| invalid(0, "expected legacy text"))?;
        if self.text_offset >= text.len() {
            return Err(Error::new(
                ErrorKind::Truncated,
                self.text_offset,
                "legacy text line",
            ));
        }
        let rest = &text[self.text_offset..];
        let (line, used) = match rest.find('\n') {
            Some(n) => (&rest[..n], n + 1),
            None => (rest, rest.len()),
        };
        self.text_offset += used;
        Ok(line.strip_suffix('\r').unwrap_or(line))
    }
    fn number(&mut self) -> Result<&'a str> {
        if self.numbers.is_empty() {
            self.numbers = self.line()?.trim();
        }
        let (number, rest) = self.numbers.split_once(' ').unwrap_or((self.numbers, ""));
        self.numbers = rest;
        if number.is_empty() {
            return Err(invalid(self.text_offset, "empty legacy numeric token"));
        }
        Ok(number)
    }
    fn i32(&mut self) -> Result<i32> {
        if self.text.is_some() {
            self.number()?
                .trim()
                .parse()
                .map_err(|_| invalid(self.text_offset, "legacy i32"))
        } else {
            self.binary.i32_le()
        }
    }
    fn u32(&mut self) -> Result<u32> {
        if self.text.is_some() {
            self.number()?
                .trim()
                .parse()
                .map_err(|_| invalid(self.text_offset, "legacy u32"))
        } else {
            self.binary.u32_le()
        }
    }
    fn i16(&mut self) -> Result<i16> {
        if self.text.is_some() {
            self.number()?
                .trim()
                .parse()
                .map_err(|_| invalid(self.text_offset, "legacy i16"))
        } else {
            self.binary.i16_le()
        }
    }
    fn float(&mut self) -> Result<F32Bits> {
        let value = if self.text.is_some() {
            let token = self.number()?;
            source_text_float(token, self.max_numeric_bytes, self.text_offset)?
        } else {
            finite_le(&mut self.binary)?
        };
        if !value.is_finite() {
            return Err(invalid(self.text_offset, "non-finite legacy float"));
        }
        Ok(value)
    }
    fn string(&mut self, budget: &mut DecodeBudget<'_>) -> Result<String> {
        let raw = if self.text.is_some() {
            self.line()?.as_bytes()
        } else {
            let n = self.binary.u8()? as usize;
            let raw = self.binary.read_bytes(n)?;
            if !raw.is_ascii() {
                return Err(unsupported(
                    self.binary.position() - n,
                    "non-ASCII binary legacy string",
                ));
            }
            raw
        };
        budget.reserve::<u8>(
            raw.len(),
            budget.limits.max_string_bytes,
            self.text_offset,
            "legacy string",
        )?;
        Ok(std::str::from_utf8(raw)
            .expect("validated string")
            .to_owned())
    }
    fn count(&mut self, budget: &DecodeBudget<'_>, max: usize, label: &str) -> Result<usize> {
        let n = self.i32()?;
        if n < 0 {
            return Err(invalid(self.text_offset, label));
        }
        budget
            .limits
            .check_count(n as usize, max, self.text_offset, label)?;
        Ok(n as usize)
    }
    fn unsigned_count(
        &mut self,
        budget: &DecodeBudget<'_>,
        max: usize,
        label: &str,
    ) -> Result<usize> {
        let n = self.u32()? as usize;
        budget.limits.check_count(n, max, self.text_offset, label)?;
        Ok(n)
    }
    fn finish(&self) -> Result<()> {
        if let Some(text) = self.text {
            if !self.numbers.is_empty() || !text[self.text_offset..].trim().is_empty() {
                return Err(unsupported(self.text_offset, "trailing legacy text"));
            }
            Ok(())
        } else {
            super::finish(&self.binary)
        }
    }
    fn vector(&mut self) -> Result<Vector3Bits> {
        Ok(CoordinatePolicy::FreeSo.vector([self.float()?, self.float()?, self.float()?]))
    }
    fn quaternion(&mut self) -> Result<QuaternionBits> {
        Ok(CoordinatePolicy::FreeSo.quaternion([
            self.float()?,
            self.float()?,
            self.float()?,
            self.float()?,
        ]))
    }
}

fn source_text_float(token: &str, max_bytes: usize, offset: usize) -> Result<F32Bits> {
    if token.len() > max_bytes {
        return Err(Error::new(
            ErrorKind::LimitExceeded,
            offset,
            "legacy numeric token",
        ));
    }
    let token = token.trim();
    let normalized;
    let parse = if token.contains(',') {
        // Single.Parse(InvariantCulture) accepts repeated/non-triad comma
        // groups in the integer part, but not before the first digit or after
        // the decimal point/exponent. Rust's parser handles the remaining
        // decimal grammar once the permitted separators are removed.
        let mut digit = false;
        let mut integer = true;
        for b in token.bytes() {
            match b {
                b'0'..=b'9' => digit = true,
                b'.' | b'e' | b'E' => integer = false,
                b',' if !digit || !integer => return Err(invalid(offset, "legacy float grouping")),
                _ => {}
            }
        }
        normalized = token.replace(',', "");
        normalized.as_str()
    } else {
        token
    };
    // The pinned framework uses a double intermediate. Direct Rust f32
    // parsing differs at decimals such as 1.0000000596046448. The original
    // proxy also loses the sign of textual or underflowed zero.
    let wide = parse
        .parse::<f64>()
        .map_err(|_| invalid(offset, "legacy float"))?;
    let value = wide as f32;
    if !value.is_finite() {
        return Err(invalid(offset, "non-finite legacy float"));
    }
    Ok(F32Bits::from_f32(if value == 0.0 { 0.0 } else { value }))
}

struct Output<'a, 'b, 'c> {
    w: &'a mut Writer<'b>,
    encoding: LegacyEncoding,
    limits: &'c Limits,
    text_policy: LegacyTextPolicy,
    normalized_signed_zeros: usize,
}
impl Output<'_, '_, '_> {
    fn number(&mut self, value: impl std::fmt::Display) -> Result<()> {
        // Only bounded primitive numeric types call this helper; no caller
        // supplied Display implementation is accepted by a public function.
        self.w.put(format!("{value}\n").as_bytes())
    }
    fn i32(&mut self, n: i32) -> Result<()> {
        if self.encoding == LegacyEncoding::Text {
            self.number(n)
        } else {
            self.w.put(&n.to_le_bytes())
        }
    }
    fn u32(&mut self, n: u32) -> Result<()> {
        if self.encoding == LegacyEncoding::Text {
            self.number(n)
        } else {
            self.w.put(&n.to_le_bytes())
        }
    }
    fn i16(&mut self, n: i16) -> Result<()> {
        if self.encoding == LegacyEncoding::Text {
            self.number(n)
        } else {
            self.w.put(&n.to_le_bytes())
        }
    }
    fn float(&mut self, f: F32Bits) -> Result<()> {
        if !f.is_finite() {
            return Err(invalid(0, "non-finite legacy float"));
        }
        if self.encoding == LegacyEncoding::Text {
            let value = if f.0 == 0x80000000 {
                if self.text_policy == LegacyTextPolicy::Exact {
                    return Err(invalid(
                        0,
                        "legacy text loses negative zero; select explicit normalization",
                    ));
                }
                self.normalized_signed_zeros = self
                    .normalized_signed_zeros
                    .checked_add(1)
                    .ok_or_else(|| invalid(0, "text normalization count overflow"))?;
                0.0
            } else {
                f.get()
            };
            // Nine significant decimal digits round-trip every finite binary32
            // value through the original double-intermediate text parser.
            self.w.put(format!("{value:.8e}\n").as_bytes())
        } else {
            self.w.float(f)
        }
    }
    fn string(&mut self, s: &String) -> Result<()> {
        if self.encoding == LegacyEncoding::Text {
            self.limits.check_count(
                s.len(),
                self.limits.max_string_bytes,
                0,
                "legacy text string",
            )?;
            if s.contains(['\r', '\n']) {
                return Err(invalid(0, "legacy text string contains newline"));
            }
            self.w.retain::<u8>(s.capacity())?;
            self.w.put(s.as_bytes())?;
            self.w.put(b"\n")
        } else {
            self.w.string(s, false)
        }
    }
    fn count(&mut self, n: usize, max: usize, label: &str) -> Result<()> {
        self.limits.check_count(n, max, 0, label)?;
        self.u32(u32::try_from(n).map_err(|_| invalid(0, "legacy count exceeds u32"))?)
    }
    fn vector(&mut self, v: Vector3Bits, policy: CoordinatePolicy) -> Result<()> {
        for f in policy.vector(v) {
            self.float(f)?;
        }
        Ok(())
    }
    fn quaternion(&mut self, v: QuaternionBits, policy: CoordinatePolicy) -> Result<()> {
        for f in policy.quaternion(v) {
            self.float(f)?;
        }
        Ok(())
    }
    fn properties(&mut self, p: &PropertyList, short: bool) -> Result<()> {
        self.w.retain::<PropertyItem>(p.items.capacity())?;
        if short {
            if p.items.len() != 1 {
                return Err(invalid(
                    0,
                    "BCF motion properties require exactly one pair-list",
                ));
            }
        } else {
            self.count(p.items.len(), self.limits.max_entries, "property items")?;
        }
        for item in &p.items {
            self.w.retain::<(String, String)>(item.pairs.capacity())?;
            self.count(item.pairs.len(), self.limits.max_entries, "property pairs")?;
            for (k, v) in &item.pairs {
                self.string(k)?;
                self.string(v)?;
            }
        }
        Ok(())
    }
}

fn read_properties(
    io: &mut Input<'_>,
    budget: &mut DecodeBudget<'_>,
    short: bool,
) -> Result<PropertyList> {
    let n = if short {
        1
    } else {
        io.unsigned_count(budget, budget.limits.max_entries, "property items")?
    };
    budget.reserve::<PropertyItem>(n, budget.limits.max_entries, 0, "property items")?;
    let mut items = Vec::with_capacity(n);
    for _ in 0..n {
        let n = io.unsigned_count(budget, budget.limits.max_entries, "property pairs")?;
        budget.reserve::<(String, String)>(n, budget.limits.max_entries, 0, "property pairs")?;
        let mut pairs = Vec::with_capacity(n);
        for _ in 0..n {
            pairs.push((io.string(budget)?, io.string(budget)?));
        }
        items.push(PropertyItem { pairs });
    }
    Ok(PropertyList { items })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkippedBone {
    pub before_index: usize,
    pub parent_name: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BcfSkeleton {
    pub skeleton: Skeleton,
    pub skipped_bones: Vec<SkippedBone>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BcfBinding {
    pub bone: String,
    pub mesh_name: String,
    pub censor_flags: i32,
    pub zero: i32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BcfAppearance {
    pub name: String,
    pub appearance_type: i32,
    pub zero: i32,
    pub bindings: Vec<BcfBinding>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BcfMotion {
    pub bone_name: String,
    pub frame_count: u32,
    pub duration_ms: F32Bits,
    pub translation_flag: i32,
    pub rotation_flag: i32,
    pub first_translation_index: i32,
    pub first_rotation_index: i32,
    pub properties: Vec<PropertyList>,
    pub time_properties: Vec<TimePropertyList>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BcfAnimation {
    pub name: String,
    pub xskill_name: String,
    pub duration_ms: F32Bits,
    pub distance: F32Bits,
    pub is_moving: i32,
    pub translation_count: u32,
    pub rotation_count: u32,
    pub motions: Vec<BcfMotion>,
    pub num_frames: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bcf {
    pub text_version: Option<i32>,
    pub skeletons: Vec<BcfSkeleton>,
    pub appearances: Vec<BcfAppearance>,
    pub animations: Vec<BcfAnimation>,
}

fn read_skeleton(io: &mut Input<'_>, budget: &mut DecodeBudget<'_>) -> Result<BcfSkeleton> {
    let name = io.string(budget)?;
    let count = io.i16()?;
    if count <= 0 {
        return Err(invalid(0, "BCF skeleton needs a positive bone count"));
    }
    let count = count as usize;
    budget.reserve::<Bone>(count, budget.limits.max_entries, 0, "BCF bones")?;
    let mut bones = Vec::with_capacity(count);
    let mut skipped_bones = Vec::new();
    let mut records = 0usize;
    while bones.len() < count {
        records += 1;
        budget
            .limits
            .check_count(records, budget.limits.max_entries, 0, "BCF bone records")?;
        let bone_name = io.string(budget)?;
        let parent_name = io.string(budget)?;
        if bone_name.is_empty() {
            // The original loop reads no other fields and decrements its bone
            // index. Preserve these records with their exact insertion point.
            if skipped_bones.len() == skipped_bones.capacity() {
                let next = skipped_bones
                    .capacity()
                    .saturating_mul(2)
                    .max(4)
                    .min(budget.limits.max_entries);
                let extra = next
                    .checked_sub(skipped_bones.capacity())
                    .ok_or_else(|| invalid(0, "skipped bone capacity"))?;
                budget.reserve::<SkippedBone>(
                    extra,
                    budget.limits.max_entries,
                    0,
                    "skipped BCF bone",
                )?;
                skipped_bones
                    .try_reserve_exact(next - skipped_bones.len())
                    .map_err(|_| {
                        Error::new(ErrorKind::LimitExceeded, 0, "skipped bone allocation")
                    })?;
            }
            skipped_bones.push(SkippedBone {
                before_index: bones.len(),
                parent_name,
            });
            continue;
        }
        let properties = read_properties(io, budget, false)?;
        bones.push(Bone {
            unknown: 0,
            name: bone_name,
            parent_name,
            properties_flag: 1,
            properties,
            translation: io.vector()?,
            rotation: io.quaternion()?,
            can_translate: io.i32()?,
            can_rotate: io.i32()?,
            can_blend: io.i32()?,
            wiggle_value: io.float()?,
            wiggle_power: io.float()?,
            index: bones.len(),
            parent: None,
            children: Vec::new(),
        });
    }
    let root = super::resolve_bone_hierarchy(&mut bones, budget, 0)?;
    Ok(BcfSkeleton {
        skeleton: Skeleton {
            version: 1,
            name,
            bones,
            root,
            coordinate_policy: CoordinatePolicy::FreeSo,
        },
        skipped_bones,
    })
}
fn read_appearance(io: &mut Input<'_>, budget: &mut DecodeBudget<'_>) -> Result<BcfAppearance> {
    let name = io.string(budget)?;
    let appearance_type = io.i32()?;
    let zero = io.i32()?;
    let n = io.unsigned_count(budget, budget.limits.max_entries, "BCF bindings")?;
    budget.reserve::<BcfBinding>(n, budget.limits.max_entries, 0, "BCF bindings")?;
    let mut bindings = Vec::with_capacity(n);
    for _ in 0..n {
        bindings.push(BcfBinding {
            bone: io.string(budget)?,
            mesh_name: io.string(budget)?,
            censor_flags: io.i32()?,
            zero: io.i32()?,
        });
    }
    Ok(BcfAppearance {
        name,
        appearance_type,
        zero,
        bindings,
    })
}
fn read_animation(io: &mut Input<'_>, budget: &mut DecodeBudget<'_>) -> Result<BcfAnimation> {
    let name = io.string(budget)?;
    let xskill_name = io.string(budget)?;
    let duration_ms = io.float()?;
    let distance = io.float()?;
    if duration_ms.get() < 0.0 {
        return Err(invalid(0, "negative BCF duration"));
    }
    let is_moving = io.i32()?;
    let translation_count =
        io.unsigned_count(budget, budget.limits.max_frames, "BCF translations")? as u32;
    let rotation_count =
        io.unsigned_count(budget, budget.limits.max_frames, "BCF rotations")? as u32;
    let n = io.unsigned_count(budget, budget.limits.max_entries, "BCF motions")?;
    budget.reserve::<BcfMotion>(n, budget.limits.max_entries, 0, "BCF motions")?;
    let mut motions = Vec::with_capacity(n);
    let mut num_frames = 0;
    for _ in 0..n {
        let bone_name = io.string(budget)?;
        let frame_count =
            io.unsigned_count(budget, budget.limits.max_frames, "BCF motion frames")? as u32;
        num_frames = num_frames.max(frame_count);
        let duration_ms = io.float()?;
        if duration_ms.get() < 0.0 {
            return Err(invalid(0, "negative BCF motion duration"));
        }
        let translation_flag = io.i32()?;
        let rotation_flag = io.i32()?;
        let first_translation_index = io.i32()?;
        let first_rotation_index = io.i32()?;
        if translation_flag == 1 {
            index_range(
                first_translation_index,
                frame_count as usize,
                translation_count as usize,
                0,
                "BCF translation range",
            )?;
        }
        if rotation_flag == 1 {
            index_range(
                first_rotation_index,
                frame_count as usize,
                rotation_count as usize,
                0,
                "BCF rotation range",
            )?;
        }
        let n = io.unsigned_count(budget, budget.limits.max_entries, "BCF property lists")?;
        budget.reserve::<PropertyList>(n, budget.limits.max_entries, 0, "BCF property lists")?;
        let mut properties = Vec::with_capacity(n);
        for _ in 0..n {
            properties.push(read_properties(io, budget, true)?);
        }
        let n = io.unsigned_count(budget, budget.limits.max_entries, "BCF time property lists")?;
        budget.reserve::<TimePropertyList>(
            n,
            budget.limits.max_entries,
            0,
            "BCF time property lists",
        )?;
        let mut time_properties = Vec::with_capacity(n);
        for _ in 0..n {
            let n = io.unsigned_count(budget, budget.limits.max_entries, "BCF time properties")?;
            budget.reserve::<TimeProperty>(
                n,
                budget.limits.max_entries,
                0,
                "BCF time properties",
            )?;
            let mut items = Vec::with_capacity(n);
            for _ in 0..n {
                items.push(TimeProperty {
                    id: io.i32()?,
                    properties: read_properties(io, budget, true)?,
                });
            }
            time_properties.push(TimePropertyList { items });
        }
        motions.push(BcfMotion {
            bone_name,
            frame_count,
            duration_ms,
            translation_flag,
            rotation_flag,
            first_translation_index,
            first_rotation_index,
            properties,
            time_properties,
        });
    }
    Ok(BcfAnimation {
        name,
        xskill_name,
        duration_ms,
        distance,
        is_moving,
        translation_count,
        rotation_count,
        motions,
        num_frames,
    })
}

pub fn decode_bcf(bytes: &[u8], encoding: LegacyEncoding, limits: &Limits) -> Result<Bcf> {
    let mut budget = DecodeBudget::new(bytes, limits)?;
    let mut io = Input::new(bytes, encoding, &mut budget)?;
    let text_version = if encoding == LegacyEncoding::Text {
        let mut lines = 0usize;
        loop {
            lines += 1;
            limits.check_count(
                lines,
                limits.max_entries,
                io.text_offset,
                "CMX header lines",
            )?;
            let line = io.line()?;
            limits.check_count(
                line.len(),
                limits.max_string_bytes,
                io.text_offset,
                "CMX header line",
            )?;
            if let Some(version) = line.strip_prefix("version ") {
                break Some(
                    version
                        .trim()
                        .parse()
                        .map_err(|_| invalid(io.text_offset, "CMX version"))?,
                );
            }
        }
    } else {
        None
    };
    let n = io.count(&budget, limits.max_entries, "BCF skeletons")?;
    budget.reserve::<BcfSkeleton>(n, limits.max_entries, 0, "BCF skeletons")?;
    let mut skeletons = Vec::with_capacity(n);
    for _ in 0..n {
        skeletons.push(read_skeleton(&mut io, &mut budget)?);
    }
    let n = io.count(&budget, limits.max_entries, "BCF appearances")?;
    budget.reserve::<BcfAppearance>(n, limits.max_entries, 0, "BCF appearances")?;
    let mut appearances = Vec::with_capacity(n);
    for _ in 0..n {
        appearances.push(read_appearance(&mut io, &mut budget)?);
    }
    let n = io.count(&budget, limits.max_entries, "BCF animations")?;
    budget.reserve::<BcfAnimation>(n, limits.max_entries, 0, "BCF animations")?;
    let mut animations = Vec::with_capacity(n);
    for _ in 0..n {
        animations.push(read_animation(&mut io, &mut budget)?);
    }
    io.finish()?;
    Ok(Bcf {
        text_version,
        skeletons,
        appearances,
        animations,
    })
}

fn write_skeleton(io: &mut Output<'_, '_, '_>, s: &BcfSkeleton) -> Result<()> {
    let skeleton = &s.skeleton;
    // Shared standalone validation checks names, hierarchy, flags, field
    // bounds and f32 bits. BCF requires its absent standalone fields to remain
    // their exact defaults. This conservatively charges validation scratch.
    if skeleton.version != 1 {
        return Err(unsupported(0, "BCF skeleton uses derived version 1"));
    }
    let (_, scratch) = measure(io.limits, |w| {
        super::encode::validate_skeleton(skeleton, io.limits, w)
    })?;
    io.w.retain::<u8>(scratch)?;
    io.w.retain::<Bone>(skeleton.bones.capacity())?;
    io.string(&skeleton.name)?;
    io.i16(skeleton.bones.len() as i16)?;
    io.w.retain::<SkippedBone>(s.skipped_bones.capacity())?;
    let mut skipped = 0usize;
    let empty = String::new();
    for (i, bone) in skeleton.bones.iter().enumerate() {
        io.w.retain::<usize>(bone.children.capacity())?;
        while s
            .skipped_bones
            .get(skipped)
            .is_some_and(|s| s.before_index == i)
        {
            let item = &s.skipped_bones[skipped];
            io.string(&empty)?;
            io.string(&item.parent_name)?;
            skipped += 1;
        }
        if bone.unknown != 0 || bone.properties_flag != 1 {
            return Err(invalid(0, "BCF has no stored bone unknown/property flag"));
        }
        io.string(&bone.name)?;
        io.string(&bone.parent_name)?;
        io.properties(&bone.properties, false)?;
        io.vector(bone.translation, skeleton.coordinate_policy)?;
        io.quaternion(bone.rotation, skeleton.coordinate_policy)?;
        io.i32(bone.can_translate)?;
        io.i32(bone.can_rotate)?;
        io.i32(bone.can_blend)?;
        io.float(bone.wiggle_value)?;
        io.float(bone.wiggle_power)?;
    }
    if skipped != s.skipped_bones.len() {
        return Err(invalid(
            0,
            "BCF skipped bone positions must follow source order",
        ));
    }
    io.limits.check_count(
        skipped + skeleton.bones.len(),
        io.limits.max_entries,
        0,
        "BCF bone records",
    )?;
    Ok(())
}
fn write_appearance(io: &mut Output<'_, '_, '_>, a: &BcfAppearance) -> Result<()> {
    io.string(&a.name)?;
    io.i32(a.appearance_type)?;
    io.i32(a.zero)?;
    io.w.retain::<BcfBinding>(a.bindings.capacity())?;
    io.count(a.bindings.len(), io.limits.max_entries, "BCF bindings")?;
    for b in &a.bindings {
        io.string(&b.bone)?;
        io.string(&b.mesh_name)?;
        io.i32(b.censor_flags)?;
        io.i32(b.zero)?;
    }
    Ok(())
}
fn write_animation(io: &mut Output<'_, '_, '_>, a: &BcfAnimation) -> Result<()> {
    if a.duration_ms.get() < 0.0 {
        return Err(invalid(0, "negative BCF duration"));
    }
    io.string(&a.name)?;
    io.string(&a.xskill_name)?;
    io.float(a.duration_ms)?;
    io.float(a.distance)?;
    io.i32(a.is_moving)?;
    io.count(
        a.translation_count as usize,
        io.limits.max_frames,
        "BCF translations",
    )?;
    io.count(
        a.rotation_count as usize,
        io.limits.max_frames,
        "BCF rotations",
    )?;
    io.w.retain::<BcfMotion>(a.motions.capacity())?;
    io.count(a.motions.len(), io.limits.max_entries, "BCF motions")?;
    if a.num_frames != a.motions.iter().map(|m| m.frame_count).max().unwrap_or(0) {
        return Err(invalid(0, "BCF derived frame count mismatch"));
    }
    for m in &a.motions {
        io.string(&m.bone_name)?;
        io.count(
            m.frame_count as usize,
            io.limits.max_frames,
            "BCF motion frames",
        )?;
        if m.duration_ms.get() < 0.0 {
            return Err(invalid(0, "negative BCF motion duration"));
        }
        io.float(m.duration_ms)?;
        io.i32(m.translation_flag)?;
        io.i32(m.rotation_flag)?;
        io.i32(m.first_translation_index)?;
        io.i32(m.first_rotation_index)?;
        if m.translation_flag == 1 {
            index_range(
                m.first_translation_index,
                m.frame_count as usize,
                a.translation_count as usize,
                0,
                "BCF translation range",
            )?;
        }
        if m.rotation_flag == 1 {
            index_range(
                m.first_rotation_index,
                m.frame_count as usize,
                a.rotation_count as usize,
                0,
                "BCF rotation range",
            )?;
        }
        io.w.retain::<PropertyList>(m.properties.capacity())?;
        io.count(
            m.properties.len(),
            io.limits.max_entries,
            "BCF property lists",
        )?;
        for p in &m.properties {
            io.properties(p, true)?;
        }
        io.w.retain::<TimePropertyList>(m.time_properties.capacity())?;
        io.count(
            m.time_properties.len(),
            io.limits.max_entries,
            "BCF time property lists",
        )?;
        for list in &m.time_properties {
            io.w.retain::<TimeProperty>(list.items.capacity())?;
            io.count(
                list.items.len(),
                io.limits.max_entries,
                "BCF time properties",
            )?;
            for item in &list.items {
                io.i32(item.id)?;
                io.properties(&item.properties, true)?;
            }
        }
    }
    Ok(())
}

pub fn encode_bcf(bcf: &Bcf, encoding: LegacyEncoding, limits: &Limits) -> Result<Vec<u8>> {
    Ok(encode_bcf_policy(bcf, encoding, LegacyTextPolicy::Exact, limits)?.bytes)
}

pub fn encode_bcf_text(
    bcf: &Bcf,
    policy: LegacyTextPolicy,
    limits: &Limits,
) -> Result<LegacyTextOutput> {
    encode_bcf_policy(bcf, LegacyEncoding::Text, policy, limits)
}

fn encode_bcf_policy(
    bcf: &Bcf,
    encoding: LegacyEncoding,
    policy: LegacyTextPolicy,
    limits: &Limits,
) -> Result<LegacyTextOutput> {
    let normalized = std::cell::Cell::new(0usize);
    let bytes = encode(limits, |w| {
        // Bound the short temporary numeric formatting buffer for text output.
        w.retain::<u8>(64)?;
        let mut io = Output {
            w,
            encoding,
            limits,
            text_policy: policy,
            normalized_signed_zeros: 0,
        };
        if encoding == LegacyEncoding::Text {
            io.w.put(format!("version {}\n", bcf.text_version.unwrap_or(300)).as_bytes())?;
        }
        io.w.retain::<BcfSkeleton>(bcf.skeletons.capacity())?;
        io.count(
            bcf.skeletons.len(),
            limits.max_entries.min(i32::MAX as usize),
            "BCF skeletons",
        )?;
        for s in &bcf.skeletons {
            write_skeleton(&mut io, s)?;
        }
        io.w.retain::<BcfAppearance>(bcf.appearances.capacity())?;
        io.count(
            bcf.appearances.len(),
            limits.max_entries.min(i32::MAX as usize),
            "BCF appearances",
        )?;
        for a in &bcf.appearances {
            write_appearance(&mut io, a)?;
        }
        io.w.retain::<BcfAnimation>(bcf.animations.capacity())?;
        io.count(
            bcf.animations.len(),
            limits.max_entries.min(i32::MAX as usize),
            "BCF animations",
        )?;
        for a in &bcf.animations {
            write_animation(&mut io, a)?;
        }
        normalized.set(io.normalized_signed_zeros);
        Ok(())
    })?;
    Ok(LegacyTextOutput {
        bytes,
        normalized_signed_zeros: normalized.get(),
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyMesh {
    pub skin_name: String,
    pub texture_name: String,
    pub mesh: Mesh,
}
pub fn decode_bmf(bytes: &[u8], encoding: LegacyEncoding, limits: &Limits) -> Result<LegacyMesh> {
    let mut budget = DecodeBudget::new(bytes, limits)?;
    let mut io = Input::new(bytes, encoding, &mut budget)?;
    let skin_name = io.string(&mut budget)?;
    let texture_name = io.string(&mut budget)?;
    let n = io.count(&budget, limits.max_entries, "BMF bones")?;
    budget.reserve::<String>(n, limits.max_entries, 0, "BMF bones")?;
    let mut bone_names = Vec::with_capacity(n);
    for _ in 0..n {
        bone_names.push(io.string(&mut budget)?);
    }
    let n = io.count(&budget, limits.max_entries, "BMF faces")?;
    budget.reserve::<[i32; 3]>(n, limits.max_entries, 0, "BMF faces")?;
    let mut faces = Vec::with_capacity(n);
    for _ in 0..n {
        faces.push([io.i32()?, io.i32()?, io.i32()?]);
    }
    let n = io.count(&budget, limits.max_entries, "BMF bindings")?;
    budget.reserve::<BoneBinding>(n, limits.max_entries, 0, "BMF bindings")?;
    let mut bindings = Vec::with_capacity(n);
    for _ in 0..n {
        let b = BoneBinding {
            bone_index: io.i32()?,
            first_real_vertex: io.i32()?,
            real_vertex_count: io.i32()?,
            first_blend_vertex: io.i32()?,
            blend_vertex_count: io.i32()?,
        };
        if b.bone_index < 0
            || b.bone_index as usize >= bone_names.len()
            || b.real_vertex_count < 0
            || b.blend_vertex_count < 0
        {
            return Err(invalid(0, "invalid BMF binding"));
        }
        bindings.push(b);
    }
    let n = io.count(&budget, limits.max_vertices, "BMF real vertices")?;
    budget.reserve::<MeshVertex>(n, limits.max_vertices, 0, "BMF real vertices")?;
    let mut vertices = Vec::with_capacity(n);
    for _ in 0..n {
        vertices.push(MeshVertex {
            texture_coordinate: [io.float()?, io.float()?],
            position: [F32Bits(0); 3],
            normal: [F32Bits(0); 3],
        });
    }
    let n = io.count(&budget, limits.max_vertices, "BMF blend vertices")?;
    limits.check_count(
        n.checked_add(vertices.len())
            .ok_or_else(|| invalid(0, "BMF vertex count overflow"))?,
        limits.max_vertices,
        0,
        "BMF total vertices",
    )?;
    budget.reserve::<BlendVertex>(n, limits.max_vertices, 0, "BMF blend vertices")?;
    let mut blend_vertices = Vec::with_capacity(n);
    for _ in 0..n {
        let first = io.i32()?;
        let second = io.i32()?;
        let (raw_weight, other_vertex) = if encoding == LegacyEncoding::Binary {
            (first, second)
        } else {
            (second, first)
        };
        if other_vertex < 0 || other_vertex as usize >= vertices.len() {
            return Err(invalid(0, "BMF blend destination outside real vertices"));
        }
        blend_vertices.push(BlendVertex {
            raw_weight,
            other_vertex,
            position: [F32Bits(0); 3],
            normal: [F32Bits(0); 3],
        });
    }
    let repeated_real_vertex_count = io.i32()?;
    if repeated_real_vertex_count < 0 || repeated_real_vertex_count as usize != vertices.len() {
        return Err(invalid(0, "BMF repeated vertex count mismatch"));
    }
    for v in &mut vertices {
        v.position = io.vector()?;
        v.normal = io.vector()?;
    }
    for v in &mut blend_vertices {
        v.position = io.vector()?;
        v.normal = io.vector()?;
    }
    io.finish()?;
    for face in &faces {
        for &i in face {
            if i < 0 || i as usize >= vertices.len() {
                return Err(invalid(0, "BMF triangle index outside vertices"));
            }
        }
    }
    for b in &bindings {
        index_range(
            b.first_real_vertex,
            b.real_vertex_count as usize,
            vertices.len(),
            0,
            "BMF real binding range",
        )?;
        index_range(
            b.first_blend_vertex,
            b.blend_vertex_count as usize,
            blend_vertices.len(),
            0,
            "BMF blend binding range",
        )?;
    }
    Ok(LegacyMesh {
        skin_name,
        texture_name,
        mesh: Mesh {
            version: 2,
            bone_names,
            faces,
            bindings,
            vertices,
            blend_vertices,
            repeated_real_vertex_count,
            coordinate_policy: CoordinatePolicy::FreeSo,
        },
    })
}

pub fn encode_bmf(
    value: &LegacyMesh,
    encoding: LegacyEncoding,
    limits: &Limits,
) -> Result<Vec<u8>> {
    Ok(encode_bmf_policy(value, encoding, LegacyTextPolicy::Exact, limits)?.bytes)
}

pub fn encode_bmf_text(
    value: &LegacyMesh,
    policy: LegacyTextPolicy,
    limits: &Limits,
) -> Result<LegacyTextOutput> {
    encode_bmf_policy(value, LegacyEncoding::Text, policy, limits)
}

fn encode_bmf_policy(
    value: &LegacyMesh,
    encoding: LegacyEncoding,
    policy: LegacyTextPolicy,
    limits: &Limits,
) -> Result<LegacyTextOutput> {
    let m = &value.mesh;
    if m.version != 2 {
        return Err(unsupported(0, "legacy mesh uses derived version 2"));
    }
    let (_, retained) = measure(limits, |w| super::encode::write_mesh_geometry(w, m, limits))?;
    let normalized = std::cell::Cell::new(0usize);
    let bytes = encode(limits, |w| {
        w.retain::<u8>(
            retained
                .checked_add(64)
                .ok_or_else(|| invalid(0, "BMF authoring budget"))?,
        )?;
        let mut io = Output {
            w,
            encoding,
            limits,
            text_policy: policy,
            normalized_signed_zeros: 0,
        };
        io.string(&value.skin_name)?;
        io.string(&value.texture_name)?;
        io.w.retain::<String>(m.bone_names.capacity())?;
        io.count(
            m.bone_names.len(),
            limits.max_entries.min(i32::MAX as usize),
            "BMF bones",
        )?;
        for n in &m.bone_names {
            io.string(n)?;
        }
        io.count(m.faces.len(), limits.max_entries, "BMF faces")?;
        for f in &m.faces {
            for &i in f {
                io.i32(i)?;
            }
        }
        io.count(m.bindings.len(), limits.max_entries, "BMF bindings")?;
        for b in &m.bindings {
            for i in [
                b.bone_index,
                b.first_real_vertex,
                b.real_vertex_count,
                b.first_blend_vertex,
                b.blend_vertex_count,
            ] {
                io.i32(i)?;
            }
        }
        io.count(m.vertices.len(), limits.max_vertices, "BMF vertices")?;
        for v in &m.vertices {
            for f in v.texture_coordinate {
                io.float(f)?;
            }
        }
        io.count(
            m.blend_vertices.len(),
            limits.max_vertices,
            "BMF blend vertices",
        )?;
        for b in &m.blend_vertices {
            // The original SKN reader reverses these fields relative to BMF.
            // Its text writer has a hard-coded binary=true bug. Write the
            // layout the original text reader actually consumes.
            if encoding == LegacyEncoding::Binary {
                io.i32(b.raw_weight)?;
                io.i32(b.other_vertex)?;
            } else {
                io.i32(b.other_vertex)?;
                io.i32(b.raw_weight)?;
            }
        }
        io.i32(m.repeated_real_vertex_count)?;
        for v in &m.vertices {
            io.vector(v.position, m.coordinate_policy)?;
            io.vector(v.normal, m.coordinate_policy)?;
        }
        for v in &m.blend_vertices {
            io.vector(v.position, m.coordinate_policy)?;
            io.vector(v.normal, m.coordinate_policy)?;
        }
        normalized.set(io.normalized_signed_zeros);
        Ok(())
    })?;
    Ok(LegacyTextOutput {
        bytes,
        normalized_signed_zeros: normalized.get(),
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CfpFrames {
    pub translations: Vec<Vector3Bits>,
    pub rotations: Vec<QuaternionBits>,
    pub coordinate_policy: CoordinatePolicy,
}

fn channel(r: &mut Reader<'_>, count: usize, mut output: impl FnMut(usize, F32Bits)) -> Result<()> {
    let mut last = F32Bits(0);
    let mut i = 0;
    while i < count {
        let code = r.u8()?;
        match code {
            255 => {
                last = finite_le(r)?;
                output(i, last);
                i += 1;
            }
            254 => {
                // Original ReadNFloats emits the repeat loop, then its normal
                // output outside the switch: stored count + 1 samples.
                let repeat = r.u16_le()? as usize + 1;
                if repeat > count - i {
                    return Err(invalid(
                        r.position() - 2,
                        "CFP repeat crosses component boundary",
                    ));
                }
                for _ in 0..repeat {
                    output(i, last);
                    i += 1;
                }
            }
            253 => {
                return Err(unsupported(
                    r.position() - 1,
                    "CFP code 253 has no source delta table entry",
                ))
            }
            n => {
                let x = i64::from(n) - 126;
                let integer = x * x * x * x.abs();
                let delta = (3.9676e-10f64 * (integer as f64)) as f32;
                last = F32Bits::from_f32(last.get() + delta);
                if !last.is_finite() {
                    return Err(invalid(r.position() - 1, "CFP delta overflow"));
                }
                output(i, last);
                i += 1;
            }
        }
    }
    Ok(())
}

pub fn decode_cfp(
    bytes: &[u8],
    translation_count: u32,
    rotation_count: u32,
    limits: &Limits,
) -> Result<CfpFrames> {
    let mut budget = DecodeBudget::new(bytes, limits)?;
    let tn = translation_count as usize;
    let rn = rotation_count as usize;
    budget.reserve::<Vector3Bits>(tn, limits.max_frames, 0, "CFP translation samples")?;
    budget.reserve::<QuaternionBits>(rn, limits.max_frames, 0, "CFP rotation samples")?;
    let mut translations = vec![[F32Bits(0); 3]; tn];
    let mut rotations = vec![[F32Bits(0); 4]; rn];
    let mut r = Reader::new(bytes);
    for c in 0..3 {
        channel(&mut r, tn, |i, f| {
            translations[i][c] = if c == 0 { f.negated() } else { f }
        })?;
    }
    for c in 0..4 {
        channel(&mut r, rn, |i, f| {
            rotations[i][c] = if c == 0 { f } else { f.negated() }
        })?;
    }
    super::finish(&r)?;
    Ok(CfpFrames {
        translations,
        rotations,
        coordinate_policy: CoordinatePolicy::FreeSo,
    })
}

pub fn encode_cfp(frames: &CfpFrames, limits: &Limits) -> Result<Vec<u8>> {
    limits.check_count(
        frames.translations.len(),
        limits.max_frames,
        0,
        "CFP translations",
    )?;
    limits.check_count(
        frames.rotations.len(),
        limits.max_frames,
        0,
        "CFP rotations",
    )?;
    encode(limits, |w| {
        w.retain::<Vector3Bits>(frames.translations.capacity())?;
        w.retain::<QuaternionBits>(frames.rotations.capacity())?;
        for c in 0..7 {
            let n = if c < 3 {
                frames.translations.len()
            } else {
                frames.rotations.len()
            };
            let at = |i: usize| {
                if c < 3 {
                    frames.coordinate_policy.vector(frames.translations[i])[c]
                } else {
                    frames.coordinate_policy.quaternion(frames.rotations[i])[c - 3]
                }
            };
            let mut i = 0;
            let mut last = F32Bits(0);
            while i < n {
                let value = at(i);
                if !value.is_finite() {
                    return Err(invalid(0, "non-finite CFP sample"));
                }
                if value == last {
                    let mut count = 1usize;
                    while i + count < n && count < 65536 && at(i + count) == last {
                        count += 1;
                    }
                    w.u8(254)?;
                    w.put(&((count - 1) as u16).to_le_bytes())?;
                    i += count;
                } else {
                    w.u8(255)?;
                    w.float(value)?;
                    last = value;
                    i += 1;
                }
            }
        }
        Ok(())
    })
}

impl BcfAnimation {
    /// Bind this exact header to its separately supplied CFP samples. No file
    /// lookup or inferred dependency occurs. The returned standalone semantic
    /// value applies the original bool==1 and unchecked moving-byte conversion.
    pub fn enrich(&self, cfp: &[u8], limits: &Limits) -> Result<Animation> {
        let (_, header) = measure(limits, |w| {
            write_animation(
                &mut Output {
                    w,
                    encoding: LegacyEncoding::Text,
                    limits,
                    text_policy: LegacyTextPolicy::NormalizeSignedZero,
                    normalized_signed_zeros: 0,
                },
                self,
            )
        })?;
        let vectors = (self.translation_count as usize)
            .checked_mul(size_of::<Vector3Bits>())
            .and_then(|n| {
                (self.rotation_count as usize)
                    .checked_mul(size_of::<QuaternionBits>())
                    .and_then(|r| n.checked_add(r))
            })
            .ok_or_else(|| invalid(0, "BCF enrichment sample allocation"))?;
        let total = header
            .checked_mul(2)
            .and_then(|n| n.checked_add(vectors))
            .and_then(|n| n.checked_add(cfp.len()))
            .and_then(|n| {
                self.motions
                    .len()
                    .checked_mul(size_of::<Motion>())
                    .and_then(|m| n.checked_add(m))
            })
            .ok_or_else(|| invalid(0, "BCF enrichment allocation overflow"))?;
        limits.check_count(
            total,
            limits.max_total_decoded_bytes,
            0,
            "BCF header/sample/enrichment allocation",
        )?;
        let frames = decode_cfp(cfp, self.translation_count, self.rotation_count, limits)?;
        let motions = self
            .motions
            .iter()
            .map(|m| Motion {
                unknown: 0,
                bone_name: m.bone_name.clone(),
                frame_count: m.frame_count,
                duration_ms: m.duration_ms,
                translation_flag: u8::from(m.translation_flag == 1),
                rotation_flag: u8::from(m.rotation_flag == 1),
                first_translation_index: m.first_translation_index,
                first_rotation_index: m.first_rotation_index,
                properties_flag: 1,
                properties: m.properties.clone(),
                time_properties_flag: 1,
                time_properties: m.time_properties.clone(),
            })
            .collect();
        Ok(Animation {
            version: 2,
            name: self.name.clone(),
            duration_ms: self.duration_ms,
            distance: self.distance,
            is_moving: self.is_moving as u8,
            translations: frames.translations,
            rotations: frames.rotations,
            motions,
            num_frames: self.num_frames,
            coordinate_policy: CoordinatePolicy::FreeSo,
        })
    }
}
