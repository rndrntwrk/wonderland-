use super::{uword, word};
use crate::ids::{EntityRef, ObjectId};
use crate::numeric;
use crate::vm::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelationshipOperand {
    pub variable: Variable,
    pub relationship_variable: u8,
    pub mode: u8,
    pub set_mode: u8,
    pub local: u8,
    pub flags: u8,
    pub old: bool,
}
impl RelationshipOperand {
    pub fn decode(opcode: u16, bytes: [u8; 8]) -> Self {
        if opcode == 24 {
            Self {
                variable: Variable::new(Scope::Parameters, bytes[2] as i16),
                relationship_variable: bytes[1],
                mode: bytes[3],
                set_mode: bytes[0],
                local: 0,
                flags: bytes[4],
                old: true,
            }
        } else {
            Self {
                variable: Variable {
                    scope: uword(&bytes, 4),
                    data: word(&bytes, 6),
                },
                relationship_variable: bytes[0],
                mode: bytes[1],
                set_mode: ((bytes[2] >> 2) & 1) | ((bytes[2] >> 4) & 2),
                local: bytes[3],
                flags: bytes[2],
                old: false,
            }
        }
    }
}
fn optional_stack<H: VmHost + ?Sized>(
    thread: &VmThread,
    host: &H,
) -> Result<Option<EntityRef>, VmFault> {
    if thread.top()?.context.stack_object_ref.is_some() {
        Ok(Some(stack_entity(thread, host)?))
    } else {
        Ok(None)
    }
}
fn neighbor_id<H: VmHost + ?Sized>(host: &H, entity: Option<EntityRef>) -> Result<i16, VmFault> {
    let Some(entity) = entity else {
        return Ok(0);
    };
    if !host.entity_info(entity)?.is_avatar {
        return Ok(0);
    }
    host.read_memory(&MemoryAddress::Entity {
        entity,
        field: EntityField::PersonData,
        index: 31,
    })
}
pub fn relationship<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
    opcode: u16,
    bytes: [u8; 8],
) -> Result<PrimitiveExit, VmFault> {
    let op = RelationshipOperand::decode(opcode, bytes);
    let frame = thread.top()?;
    let caller = frame.context.caller;
    let local = if op.mode == 2 || op.mode == 3 {
        if op.old {
            frame.args[index("old relationship parameter", 0, frame.args.len())?]
        } else {
            frame.locals[index("relationship local", op.local as i32, frame.locals.len())?]
        }
    } else {
        0
    };
    let stack = optional_stack(thread, host)?;
    let (from, to) = match op.mode {
        0 => (Some(caller), stack),
        1 => (stack, Some(caller)),
        2 => (stack, host.resolve_entity(ObjectId(local))),
        3 => (host.resolve_entity(ObjectId(local)), stack),
        _ => {
            return Err(VmFault::InvalidOperand {
                opcode,
                detail: "Invalid relationship mode".into(),
            })
        }
    };
    let use_neighbor = op.flags & 2 != 0;
    let fail_small = op.flags & 1 != 0;
    if (from.is_none() || to.is_none()) && !(thread.mode == VmMode::Ts1 && use_neighbor) {
        return Ok(PrimitiveExit::GotoTrue);
    }
    let mut from_nid = neighbor_id(host, from)?;
    let mut to_nid = neighbor_id(host, to)?;
    let persistent = to
        .map(|entity| host.entity_info(entity).map(|info| info.persistent_id))
        .transpose()?
        .unwrap_or(0);
    let key = if thread.mode == VmMode::Tso && persistent > 0 && op.flags & 128 == 0 {
        RelationshipKey {
            owner: RelationshipOwner::Entity(from.expect("non-null checked")),
            target: RelationshipTarget::Persistent(persistent),
        }
    } else if thread.mode == VmMode::Ts1 && (use_neighbor || (from_nid > 0 && to_nid > 0)) {
        if use_neighbor {
            if op.mode < 2 && !host.entity_info(caller)?.is_avatar {
                return Err(VmFault::InvalidOperand {
                    opcode,
                    detail: "TS1 neighbor relationship caller must be an avatar".into(),
                });
            }
            let caller_nid = if op.mode < 2 {
                neighbor_id(host, Some(caller))?
            } else {
                0
            };
            (from_nid, to_nid) = match op.mode {
                0 => (caller_nid, frame.context.stack_object.0),
                1 => (frame.context.stack_object.0, caller_nid),
                2 => (frame.context.stack_object.0, local),
                3 => (local, frame.context.stack_object.0),
                _ => unreachable!(),
            };
        }
        RelationshipKey {
            owner: RelationshipOwner::Neighbor(from_nid),
            target: RelationshipTarget::Neighbor(to_nid),
        }
    } else {
        RelationshipKey {
            owner: RelationshipOwner::Entity(from.expect("non-null checked")),
            target: RelationshipTarget::Local(to.expect("non-null checked")),
        }
    };
    let mut values = match host.relationship_read(key)? {
        Some(values) => values,
        None => {
            if fail_small {
                return Ok(PrimitiveExit::GotoFalse);
            }
            host.relationship_write(key, Vec::new())?;
            Vec::new()
        }
    };
    host.relationship_mark(key, op.set_mode > 0)?;
    let at = op.relationship_variable as usize;
    if values.len() <= at {
        if fail_small {
            return Ok(PrimitiveExit::GotoFalse);
        }
        values.resize(at + 1, 0);
        host.relationship_write(key, values.clone())?;
    }
    // Source obtains this tuning value even for get/set modes; require a finite typed provider value.
    let multiplier = host.relationship_multiplier()?;
    if !multiplier.is_finite() {
        return Err(VmFault::InvalidContent(
            "Non-finite relationship multiplier".into(),
        ));
    }
    match op.set_mode {
        0 => {
            write_variable(thread, host, op.variable, values[at])?;
        }
        1 | 2 => {
            let value = read_variable(thread, host, op.variable)?;
            values[at] = if op.set_mode == 1 {
                value
            } else {
                values[at].wrapping_add(numeric::legacy_f64_to_i16(
                    (value as f32 * multiplier) as f64,
                ))
            };
            if op.flags & 64 == 0 {
                values[at] = values[at].clamp(-100, 100);
            }
            host.relationship_write(key, values)?;
        }
        _ => {}
    }
    Ok(PrimitiveExit::GotoTrue)
}
