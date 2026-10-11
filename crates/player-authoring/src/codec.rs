use crate::AuthoringError;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement {
    pub x: i16,
    pub y: i16,
    pub level: i8,
    pub direction: u8,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchitectureCommand {
    pub kind: u8,
    pub x: i32,
    pub y: i32,
    pub level: i8,
    pub x2: i32,
    pub y2: i32,
    pub pattern: u16,
    pub style: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct InventoryRestore {
    pub persist_id: u32,
    pub guid: u32,
    pub restore_type: u8,
    pub upgrade: u8,
    pub wear: i32,
    pub data: Vec<u8>,
    pub attributes: Vec<i16>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SourceCommand {
    Architecture(Vec<ArchitectureCommand>),
    Buy {
        guid: u32,
        placement: Placement,
        value: i32,
        mode: u8,
        upgrade: u8,
    },
    Move {
        object_id: i16,
        placement: Placement,
    },
    Delete {
        object_id: i16,
        persist_id: u32,
        cleanup_all: bool,
        success: bool,
        mode: u8,
    },
    SendToInventory {
        persist_id: u32,
        success: bool,
    },
    PlaceInventory {
        persist_id: u32,
        placement: Placement,
        restore: InventoryRestore,
        mode: u8,
    },
    SetRoof {
        pitch: f32,
        style: u32,
    },
    SetOutfitServer {
        avatar_id: u32,
        scope: i16,
        #[serde(with = "crate::decimal_u64")]
        asset_id: u64,
    },
}
/// Original full VMNetCommand: command type, ActorUID, then BinaryWriter body.
/// This validates syntax and excludes server-populated fields; VM.Verify remains
/// the authority for permissions, object state, placement and transactions.
pub fn encode_client_command(
    actor: u32,
    command: &SourceCommand,
) -> Result<Vec<u8>, AuthoringError> {
    let bytes = encode_source_command(actor, command)?;
    validate_client_command(&bytes, actor)?;
    Ok(bytes)
}
pub fn encode_source_command(
    actor: u32,
    command: &SourceCommand,
) -> Result<Vec<u8>, AuthoringError> {
    let tag = match command {
        SourceCommand::Architecture(_) => 2,
        SourceCommand::Buy { .. } => 3,
        SourceCommand::Move { .. } => 8,
        SourceCommand::Delete { .. } => 9,
        SourceCommand::SendToInventory { .. } => 21,
        SourceCommand::PlaceInventory { .. } => 22,
        SourceCommand::SetRoof { .. } => 32,
        SourceCommand::SetOutfitServer { .. } => 33,
    };
    let mut out = vec![tag];
    out.extend(actor.to_le_bytes());
    fn placement(out: &mut Vec<u8>, p: &Placement) {
        out.extend(p.x.to_le_bytes());
        out.extend(p.y.to_le_bytes());
        out.push(p.level as u8);
        out.push(p.direction);
    }
    match command {
        SourceCommand::Architecture(commands) => {
            out.extend(
                i32::try_from(commands.len())
                    .map_err(|_| AuthoringError::Invalid("architecture count"))?
                    .to_le_bytes(),
            );
            for c in commands {
                if c.kind > 9 {
                    return Err(AuthoringError::Invalid("architecture type"));
                }
                out.push(c.kind);
                out.extend(c.x.to_le_bytes());
                out.extend(c.y.to_le_bytes());
                out.push(c.level as u8);
                out.extend(c.x2.to_le_bytes());
                out.extend(c.y2.to_le_bytes());
                out.extend(c.pattern.to_le_bytes());
                out.extend(c.style.to_le_bytes());
            }
        }
        SourceCommand::Buy {
            guid,
            placement: p,
            value,
            mode,
            upgrade,
        } => {
            out.extend(guid.to_le_bytes());
            placement(&mut out, p);
            out.extend(value.to_le_bytes());
            out.extend([*mode, *upgrade]);
        }
        SourceCommand::Move {
            object_id,
            placement: p,
        } => {
            out.extend(object_id.to_le_bytes());
            placement(&mut out, p);
        }
        SourceCommand::Delete {
            object_id,
            persist_id,
            cleanup_all,
            success,
            mode,
        } => {
            out.extend(object_id.to_le_bytes());
            out.extend(persist_id.to_le_bytes());
            out.extend([u8::from(*cleanup_all), u8::from(*success), *mode]);
        }
        SourceCommand::SendToInventory {
            persist_id,
            success,
        } => {
            out.extend(persist_id.to_le_bytes());
            out.push(u8::from(*success));
        }
        SourceCommand::PlaceInventory {
            persist_id,
            placement: p,
            restore: r,
            mode,
        } => {
            if r.data.len() > 4096 || r.attributes.len() > 255 {
                return Err(AuthoringError::Invalid("inventory restore size"));
            }
            out.extend(persist_id.to_le_bytes());
            placement(&mut out, p);
            out.extend(r.persist_id.to_le_bytes());
            out.extend(r.guid.to_le_bytes());
            out.extend([r.restore_type, r.upgrade]);
            out.extend(r.wear.to_le_bytes());
            out.extend((r.data.len() as i32).to_le_bytes());
            out.extend(&r.data);
            out.push(r.attributes.len() as u8);
            for a in &r.attributes {
                out.extend(a.to_le_bytes());
            }
            out.push(*mode);
        }
        SourceCommand::SetRoof { pitch, style } => {
            if !pitch.is_finite() {
                return Err(AuthoringError::Invalid("roof pitch"));
            }
            out.extend(pitch.to_le_bytes());
            out.extend(style.to_le_bytes());
        }
        SourceCommand::SetOutfitServer {
            avatar_id,
            scope,
            asset_id,
        } => {
            out.extend(avatar_id.to_le_bytes());
            out.extend(scope.to_le_bytes());
            out.extend(asset_id.to_le_bytes());
        }
    }
    Ok(out)
}
pub(crate) struct Reader<'a> {
    pub(crate) bytes: &'a [u8],
    pub(crate) at: usize,
}
impl<'a> Reader<'a> {
    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8], AuthoringError> {
        let end = self
            .at
            .checked_add(n)
            .ok_or(AuthoringError::Invalid("length overflow"))?;
        let value = self
            .bytes
            .get(self.at..end)
            .ok_or(AuthoringError::Invalid("truncated command"))?;
        self.at = end;
        Ok(value)
    }
    pub(crate) fn u8(&mut self) -> Result<u8, AuthoringError> {
        Ok(self.take(1)?[0])
    }
    fn bool(&mut self) -> Result<bool, AuthoringError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(AuthoringError::Invalid("boolean")),
        }
    }
    fn u16(&mut self) -> Result<u16, AuthoringError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn i16(&mut self) -> Result<i16, AuthoringError> {
        Ok(self.u16()? as i16)
    }
    fn u32(&mut self) -> Result<u32, AuthoringError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32, AuthoringError> {
        Ok(self.u32()? as i32)
    }
    fn placement(&mut self) -> Result<Placement, AuthoringError> {
        Ok(Placement {
            x: self.i16()?,
            y: self.i16()?,
            level: self.u8()? as i8,
            direction: self.u8()?,
        })
    }
}
/// Safely consumes one supported command, including authoritative restore fields.
/// Unknown commands have no declared byte length and must invalidate tick parsing.
pub fn decode_command_prefix(bytes: &[u8]) -> Result<(u32, SourceCommand, usize), AuthoringError> {
    let mut r = Reader { bytes, at: 0 };
    let tag = r.u8()?;
    let actor = r.u32()?;
    let command = match tag {
        2 => {
            let count = r.i32()?;
            if count < 0 || count as usize > (r.bytes.len() - r.at) / 22 {
                return Err(AuthoringError::Invalid("architecture count"));
            }
            let mut commands = Vec::with_capacity(count as usize);
            for _ in 0..count {
                let kind = r.u8()?;
                if kind > 9 {
                    return Err(AuthoringError::Invalid("architecture type"));
                }
                commands.push(ArchitectureCommand {
                    kind,
                    x: r.i32()?,
                    y: r.i32()?,
                    level: r.u8()? as i8,
                    x2: r.i32()?,
                    y2: r.i32()?,
                    pattern: r.u16()?,
                    style: r.u16()?,
                });
            }
            SourceCommand::Architecture(commands)
        }
        3 => SourceCommand::Buy {
            guid: r.u32()?,
            placement: r.placement()?,
            value: r.i32()?,
            mode: r.u8()?,
            upgrade: r.u8()?,
        },
        8 => SourceCommand::Move {
            object_id: r.i16()?,
            placement: r.placement()?,
        },
        9 => SourceCommand::Delete {
            object_id: r.i16()?,
            persist_id: r.u32()?,
            cleanup_all: r.bool()?,
            success: r.bool()?,
            mode: r.u8()?,
        },
        21 => SourceCommand::SendToInventory {
            persist_id: r.u32()?,
            success: r.bool()?,
        },
        22 => {
            let persist_id = r.u32()?;
            let placement = r.placement()?;
            let mut restore = InventoryRestore {
                persist_id: r.u32()?,
                guid: r.u32()?,
                restore_type: r.u8()?,
                upgrade: r.u8()?,
                wear: r.i32()?,
                ..Default::default()
            };
            let n = r.i32()?;
            if !(0..=4096).contains(&n) {
                return Err(AuthoringError::Invalid("inventory data length"));
            }
            restore.data = r.take(n as usize)?.to_vec();
            let count = r.u8()?;
            for _ in 0..count {
                restore.attributes.push(r.i16()?);
            }
            let mode = r.u8()?;
            SourceCommand::PlaceInventory {
                persist_id,
                placement,
                restore,
                mode,
            }
        }
        32 => SourceCommand::SetRoof {
            pitch: f32::from_bits(r.u32()?),
            style: r.u32()?,
        },
        33 => SourceCommand::SetOutfitServer {
            avatar_id: r.u32()?,
            scope: r.i16()?,
            asset_id: u64::from_le_bytes(r.take(8)?.try_into().unwrap()),
        },
        _ => return Err(AuthoringError::Unsupported(tag)),
    };
    Ok((actor, command, r.at))
}
pub fn validate_client_command(bytes: &[u8], actor: u32) -> Result<(), AuthoringError> {
    if actor == 0 {
        return Err(AuthoringError::WrongActor);
    }
    let (claimed, command, consumed) = decode_command_prefix(bytes)?;
    if claimed != actor {
        return Err(AuthoringError::WrongActor);
    }
    if consumed != bytes.len() {
        return Err(AuthoringError::Invalid("trailing command bytes"));
    }
    let valid_placement = |p: &Placement| p.direction.count_ones() == 1;
    match command {
        SourceCommand::Architecture(commands) => {
            if commands.is_empty() {
                return Err(AuthoringError::Invalid("empty architecture"));
            }
        }
        SourceCommand::Buy {
            placement,
            value,
            mode,
            ..
        } => {
            if value != -1 || !(1..=2).contains(&mode) || !valid_placement(&placement) {
                return Err(AuthoringError::Invalid("client purchase fields"));
            }
        }
        SourceCommand::Move {
            object_id,
            placement,
        } => {
            if object_id == 0 || !valid_placement(&placement) {
                return Err(AuthoringError::Invalid("client move fields"));
            }
        }
        SourceCommand::Delete {
            object_id,
            persist_id,
            success,
            mode,
            ..
        } => {
            if object_id == 0 || persist_id != 0 || success || !(1..=2).contains(&mode) {
                return Err(AuthoringError::Invalid("client delete fields"));
            }
        }
        SourceCommand::SendToInventory {
            persist_id,
            success,
        } => {
            if persist_id == 0 || success {
                return Err(AuthoringError::Invalid("client inventory fields"));
            }
        }
        SourceCommand::PlaceInventory {
            persist_id,
            placement,
            restore,
            mode,
        } => {
            if persist_id == 0
                || restore != InventoryRestore::default()
                || !valid_placement(&placement)
                || !(1..=2).contains(&mode)
            {
                return Err(AuthoringError::Invalid("client restore fields"));
            }
        }
        SourceCommand::SetRoof { pitch, .. } => {
            if !pitch.is_finite() {
                return Err(AuthoringError::Invalid("roof pitch"));
            }
        }
        SourceCommand::SetOutfitServer { .. } => return Err(AuthoringError::Unsupported(33)),
    }
    Ok(())
}
