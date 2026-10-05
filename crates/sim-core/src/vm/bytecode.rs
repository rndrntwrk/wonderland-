use super::VmFault;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const BRANCH_ERROR: u8 = 253;
pub const BRANCH_TRUE: u8 = 254;
pub const BRANCH_FALSE: u8 = 255;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RoutineScope {
    Global,
    Private(u32),
    SemiGlobal(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RoutineKey {
    pub scope: RoutineScope,
    pub id: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmInstruction {
    pub opcode: u16,
    pub true_pointer: u8,
    pub false_pointer: u8,
    pub operand: [u8; 8],
}

impl VmInstruction {
    pub const fn new(opcode: u16, true_pointer: u8, false_pointer: u8, operand: [u8; 8]) -> Self {
        Self {
            opcode,
            true_pointer,
            false_pointer,
            operand,
        }
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, VmFault> {
        if bytes.len() != 12 {
            return Err(VmFault::InvalidContent(
                "BHAV instruction must have 12 bytes".into(),
            ));
        }
        let mut operand = [0; 8];
        operand.copy_from_slice(&bytes[4..12]);
        Ok(Self::new(
            u16::from_le_bytes([bytes[0], bytes[1]]),
            bytes[2],
            bytes[3],
            operand,
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BhavLimits {
    pub max_bytes: usize,
    pub max_instructions: usize,
    pub max_locals: usize,
    pub max_arguments: usize,
}
impl Default for BhavLimits {
    fn default() -> Self {
        Self {
            max_bytes: 1_048_576,
            max_instructions: 253,
            max_locals: 1024,
            max_arguments: 255,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmRoutine {
    id: u16,
    locals: u16,
    arguments: u16,
    pub routine_type: u8,
    pub format_version: u16,
    pub version: u16,
    instructions: Vec<VmInstruction>,
}
impl VmRoutine {
    pub fn new(
        id: u16,
        locals: u16,
        arguments: u16,
        instructions: Vec<VmInstruction>,
    ) -> Result<Self, VmFault> {
        let routine = Self {
            id,
            locals,
            arguments,
            routine_type: 0,
            format_version: 0x8003,
            version: 0,
            instructions,
        };
        routine.validate(BhavLimits::default())?;
        Ok(routine)
    }
    pub fn id(&self) -> u16 {
        self.id
    }
    pub fn locals(&self) -> u16 {
        self.locals
    }
    pub fn arguments(&self) -> u16 {
        self.arguments
    }
    pub fn instructions(&self) -> &[VmInstruction] {
        &self.instructions
    }
    pub fn instruction(&self, index: u8) -> Result<&VmInstruction, VmFault> {
        self.instructions
            .get(index as usize)
            .ok_or_else(|| VmFault::Bounds {
                area: "instruction".into(),
                index: index as i32,
                len: self.instructions.len(),
            })
    }
    pub fn validate(&self, limits: BhavLimits) -> Result<(), VmFault> {
        if self.instructions.len() > limits.max_instructions.min(253)
            || self.locals as usize > limits.max_locals
            || self.arguments as usize > limits.max_arguments
        {
            return Err(VmFault::InvalidContent(
                "BHAV exceeds interpreter limits".into(),
            ));
        }
        for instruction in &self.instructions {
            for pointer in [instruction.true_pointer, instruction.false_pointer] {
                if pointer < BRANCH_ERROR && pointer as usize >= self.instructions.len() {
                    return Err(VmFault::InvalidContent(format!(
                        "BHAV branch {pointer} outside {} instructions",
                        self.instructions.len()
                    )));
                }
            }
        }
        Ok(())
    }
    /// Decode a BHAV chunk body, excluding its IFF container header (owned by content import).
    /// Source: tso.files/Formats/IFF/Chunks/BHAV.cs, versions 0x8000 through 0x8003.
    pub fn decode(id: u16, bytes: &[u8], limits: BhavLimits) -> Result<Self, VmFault> {
        if bytes.len() > limits.max_bytes || bytes.len() < 12 {
            return Err(VmFault::InvalidContent(
                "BHAV byte limit or truncated header".into(),
            ));
        }
        let u16_at = |offset: usize| u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        let format_version = u16_at(0);
        let (count, routine_type, arguments, locals, version, header) = match format_version {
            0x8000 | 0x8001 => (u16_at(2) as usize, 0, 0, 0, 0, 12),
            0x8002 => (
                u16_at(2) as usize,
                bytes[4],
                bytes[5] as u16,
                u16_at(6),
                u16_at(8),
                12,
            ),
            0x8003 => {
                if bytes.len() < 13 {
                    return Err(VmFault::InvalidContent("Truncated 0x8003 header".into()));
                }
                (
                    u32::from_le_bytes([bytes[9], bytes[10], bytes[11], bytes[12]]) as usize,
                    bytes[2],
                    bytes[3] as u16,
                    bytes[4] as u16,
                    u16_at(7),
                    13,
                )
            }
            _ => {
                return Err(VmFault::InvalidContent(format!(
                    "Unknown BHAV format {format_version:#06x}"
                )))
            }
        };
        if count > limits.max_instructions.min(253) {
            return Err(VmFault::InvalidContent(
                "BHAV instruction count limit".into(),
            ));
        }
        let expected = header
            + count
                .checked_mul(12)
                .ok_or_else(|| VmFault::InvalidContent("BHAV size overflow".into()))?;
        if bytes.len() != expected {
            return Err(VmFault::InvalidContent(
                "BHAV truncated or trailing data".into(),
            ));
        }
        let instructions = bytes[header..]
            .chunks_exact(12)
            .map(VmInstruction::decode)
            .collect::<Result<Vec<_>, _>>()?;
        let routine = Self {
            id,
            locals,
            arguments,
            routine_type,
            format_version,
            version,
            instructions,
        };
        routine.validate(limits)?;
        Ok(routine)
    }
}

/// Immutable after loading: all mutation methods reject overwriting an existing content identity.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutineStore {
    routines: BTreeMap<RoutineKey, VmRoutine>,
    semiglobals: BTreeMap<u32, u32>,
}
impl RoutineStore {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn insert(&mut self, key: RoutineKey, routine: VmRoutine) -> Result<(), VmFault> {
        routine.validate(BhavLimits::default())?;
        if key.id != routine.id || self.routines.contains_key(&key) {
            return Err(VmFault::InvalidContent(
                "Routine identity mismatch or duplicate".into(),
            ));
        }
        self.routines.insert(key, routine);
        Ok(())
    }
    pub fn bind_semiglobal(&mut self, code_owner: u32, resource: u32) -> Result<(), VmFault> {
        if self.semiglobals.contains_key(&code_owner) {
            return Err(VmFault::InvalidContent(
                "Duplicate semiglobal binding".into(),
            ));
        }
        self.semiglobals.insert(code_owner, resource);
        Ok(())
    }
    pub fn get(&self, key: RoutineKey) -> Option<&VmRoutine> {
        self.routines.get(&key)
    }
    pub fn resolve(&self, code_owner: u32, id: u16) -> Option<RoutineKey> {
        let scope = if id >= 8192 {
            RoutineScope::SemiGlobal(*self.semiglobals.get(&code_owner)?)
        } else if id >= 4096 {
            RoutineScope::Private(code_owner)
        } else {
            RoutineScope::Global
        };
        let key = RoutineKey { scope, id };
        self.routines.contains_key(&key).then_some(key)
    }
    pub fn semiglobal(&self, code_owner: u32) -> Option<u32> {
        self.semiglobals.get(&code_owner).copied()
    }
    pub fn validate(&self) -> Result<(), VmFault> {
        for (key, routine) in &self.routines {
            if key.id != routine.id {
                return Err(VmFault::InvalidContent("Routine key mismatch".into()));
            }
            routine.validate(BhavLimits::default())?;
        }
        Ok(())
    }
    pub fn len(&self) -> usize {
        self.routines.len()
    }
    pub fn is_empty(&self) -> bool {
        self.routines.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubroutineOperand {
    pub arguments: [i16; 4],
    pub use_temps: bool,
}
impl SubroutineOperand {
    pub fn decode(bytes: [u8; 8]) -> Self {
        let arguments =
            std::array::from_fn(|i| i16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]]));
        Self {
            use_temps: arguments[1..].iter().any(|value| *value != 0),
            arguments,
        }
    }
}
