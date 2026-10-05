use super::{dword, uword, word};
use crate::ids::{EntityRef, ObjectId};
use crate::numeric;
use crate::vm::*;

const OUT_OF_WORLD: VmPosition = VmPosition {
    x: i16::MIN,
    y: i16::MIN,
    level: 1,
};
fn same_tile(a: VmPosition, b: VmPosition) -> bool {
    a.level == b.level && (a.x >> 4) == (b.x >> 4) && (a.y >> 4) == (b.y >> 4)
}
fn resolve<H: VmHost + ?Sized>(host: &H, id: i16) -> Result<EntityRef, VmFault> {
    host.resolve_entity(ObjectId(id))
        .ok_or(VmFault::MissingEntity(ObjectId(id)))
}
fn bool_operation<H: VmHost + ?Sized>(
    host: &mut H,
    operation: EntityOperation,
) -> Result<bool, VmFault> {
    match host.entity_operation(operation)? {
        EntityOperationResult::Bool(result) => Ok(result),
        _ => Err(VmFault::HostUnsupported(
            "Entity operation response type".into(),
        )),
    }
}
fn slot<H: VmHost + ?Sized>(
    host: &H,
    entity: EntityRef,
    slot: i16,
) -> Result<Option<EntityRef>, VmFault> {
    let id = host.read_memory(&MemoryAddress::Entity {
        entity,
        field: EntityField::Slot,
        index: slot as u16,
    })?;
    if id == 0 {
        Ok(None)
    } else {
        Ok(Some(resolve(host, id)?))
    }
}
fn infos<H: VmHost + ?Sized>(host: &H) -> Result<Vec<EntityInfo>, VmFault> {
    let mut ids = host.entity_ids()?;
    ids.sort();
    ids.dedup();
    let mut result = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(reference) = host.resolve_entity(id) {
            result.push(host.entity_info(reference)?);
        }
    }
    Ok(result)
}
fn write_target<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
    target: Variable,
    value: i16,
) -> Result<PrimitiveExit, VmFault> {
    write_variable(thread, host, target, value)?;
    Ok(PrimitiveExit::GotoTrue)
}

pub fn test_object_type<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
    bytes: [u8; 8],
) -> Result<PrimitiveExit, VmFault> {
    let target = Variable {
        data: word(&bytes, 4),
        scope: bytes[6] as u16,
    };
    let id = read_variable(thread, host, target)?;
    let object = if target.scope == 10 {
        match stack_entity(thread, host) {
            Ok(value) => Some(value),
            Err(VmFault::MissingEntity(_)) => None,
            Err(error) => return Err(error),
        }
    } else {
        host.resolve_entity(ObjectId(id))
    };
    let Some(object) = object else {
        return Ok(PrimitiveExit::Error);
    };
    let info = host.entity_info(object)?;
    let guid = dword(&bytes, 0);
    Ok(PrimitiveExit::branch(
        info.guid == guid || info.master_guid == Some(guid),
    ))
}

pub fn remove<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
    bytes: [u8; 8],
) -> Result<PrimitiveExit, VmFault> {
    let caller = thread.top()?.context.caller;
    let target = if word(&bytes, 0) == 0 {
        Some(caller)
    } else {
        if thread.top()?.context.stack_object_ref.is_some() {
            Some(stack_entity(thread, host)?)
        } else {
            None
        }
    };
    if let Some(target) = target {
        bool_operation(
            host,
            EntityOperation::Delete {
                target,
                cleanup_all: bytes[2] & 2 != 0,
                return_immediately: bytes[2] & 1 != 0,
            },
        )?;
    }
    Ok(if target == Some(caller) {
        PrimitiveExit::GotoTrueNextTick
    } else {
        PrimitiveExit::GotoTrue
    })
}
pub fn notify<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
) -> Result<PrimitiveExit, VmFault> {
    if thread.top()?.context.stack_object_ref.is_some() {
        let target = stack_entity(thread, host)?;
        if target == thread.owner {
            thread.interrupt = true;
        }
        bool_operation(host, EntityOperation::Notify { target })?;
    }
    Ok(PrimitiveExit::GotoTrue)
}
pub fn grab<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
) -> Result<PrimitiveExit, VmFault> {
    let container = thread.top()?.context.caller;
    let object = if thread.top()?.context.stack_object_ref.is_some() {
        Some(stack_entity(thread, host)?)
    } else {
        None
    };
    Ok(PrimitiveExit::branch(bool_operation(
        host,
        EntityOperation::PlaceInSlot {
            container,
            object,
            slot: 0,
            clean_old: true,
        },
    )?))
}
pub fn drop_onto<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
    bytes: [u8; 8],
) -> Result<PrimitiveExit, VmFault> {
    let frame = thread.top()?;
    let src = if uword(&bytes, 0) == 1 {
        frame.args[index(
            "drop source parameter",
            uword(&bytes, 2) as i32,
            frame.args.len(),
        )?]
    } else {
        word(&bytes, 2)
    };
    let dest = if uword(&bytes, 4) == 1 {
        frame.args[index(
            "drop destination parameter",
            uword(&bytes, 6) as i32,
            frame.args.len(),
        )?]
    } else {
        word(&bytes, 6)
    };
    let Some(object) = slot(host, frame.context.caller, src)? else {
        return Ok(PrimitiveExit::GotoFalse);
    };
    let container = stack_entity(thread, host)?;
    if slot(host, container, dest)?.is_some() {
        return Ok(PrimitiveExit::GotoFalse);
    }
    Ok(PrimitiveExit::branch(bool_operation(
        host,
        EntityOperation::PlaceInSlot {
            container,
            object: Some(object),
            slot: dest,
            clean_old: true,
        },
    )?))
}
pub fn drop_object<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
) -> Result<PrimitiveExit, VmFault> {
    let caller = thread.top()?.context.caller;
    let Some(target) = slot(host, caller, 0)? else {
        return Ok(PrimitiveExit::GotoFalse);
    };
    let info = host.entity_info(caller)?;
    if info.direction > 7 {
        return Err(VmFault::InvalidOperand {
            opcode: 5,
            detail: "Direction exceeds the eight canonical notches".into(),
        });
    }
    let base = VmPosition {
        x: ((info.position.x >> 4) << 4).wrapping_add(8),
        y: ((info.position.y >> 4) << 4).wrapping_add(8),
        level: info.position.level,
    };
    let directions = [
        (0, -16),
        (16, -16),
        (16, 0),
        (16, 16),
        (0, 16),
        (-16, 16),
        (-16, 0),
        (-16, -16),
    ];
    let dir = info.direction as usize;
    for offset in [0, 7, 1, 6, 2, 5, 3, 4] {
        let (x, y) = directions[(dir + offset) % 8];
        let position = VmPosition {
            x: base.x.wrapping_add(x),
            y: base.y.wrapping_add(y),
            level: base.level,
        };
        if bool_operation(
            host,
            EntityOperation::ChangePosition {
                target,
                position,
                direction: info.direction,
            },
        )? {
            return Ok(PrimitiveExit::GotoTrue);
        }
    }
    Ok(PrimitiveExit::GotoFalse)
}

pub fn distance<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
    bytes: [u8; 8],
) -> Result<PrimitiveExit, VmFault> {
    let target = stack_entity(thread, host)?;
    // Decoder normalizes old flag=0 into MyObject.ObjectId, then sets flag bit 1.
    let source = if bytes[2] & 1 == 0 {
        Variable::new(Scope::MyObject, 11)
    } else {
        Variable {
            scope: bytes[3] as u16,
            data: word(&bytes, 4),
        }
    };
    let source = resolve(host, read_variable(thread, host, source)?)?;
    let pos1 = host.entity_info(target)?.position;
    let pos2 = host.entity_info(source)?.position;
    let dx = pos1.x as i64 - pos2.x as i64;
    let dy = pos1.y as i64 - pos2.y as i64;
    let square = (dx * dx + dy * dy) as u64;
    // Integer floor(sqrt(square))/16 is identical to source floor(sqrt(square)/16).
    let mut low = 0u64;
    let mut high = 92682u64;
    while low < high {
        let mid = (low + high).div_ceil(2);
        if mid * mid <= square {
            low = mid
        } else {
            high = mid - 1
        }
    }
    let result = (low / 16) as i32;
    let levels = (pos2.level as i32 - pos1.level as i32).abs();
    let result = if thread.mode == VmMode::Ts1 {
        (20 * levels).max(result + 5 * levels)
    } else {
        result + 20 * levels
    };
    let at = index("distance temp", uword(&bytes, 0) as i32, 20)?;
    thread.temps[at] = result as i16;
    Ok(PrimitiveExit::GotoTrue)
}
pub fn direction<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
    bytes: [u8; 8],
) -> Result<PrimitiveExit, VmFault> {
    let target = stack_entity(thread, host)?;
    let source = if bytes[4] & 1 == 0 {
        Variable::new(Scope::MyObject, 11)
    } else {
        Variable {
            scope: bytes[5] as u16,
            data: word(&bytes, 6),
        }
    };
    let source = resolve(host, read_variable(thread, host, source)?)?;
    let pos1 = host.entity_info(source)?.position;
    let pos2 = host.entity_info(target)?.position;
    let angle = (pos2.x as f64 - pos1.x as f64).atan2(pos1.y as f64 - pos2.y as f64);
    let tau = std::f64::consts::PI * 2.0;
    let normalized = (angle % tau + tau) % tau;
    let normalized = if normalized > std::f64::consts::PI {
        normalized - tau
    } else {
        normalized
    };
    let positive = (normalized % tau + tau) % tau;
    let result = numeric::round_ties_even((positive / std::f64::consts::PI) * 4.0) as i16;
    write_variable(
        thread,
        host,
        Variable {
            scope: uword(&bytes, 2),
            data: word(&bytes, 0),
        },
        result,
    )?;
    Ok(PrimitiveExit::GotoTrue)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetToNextOperand {
    pub guid: u32,
    pub search: u8,
    pub target: Variable,
    pub local: u8,
}
impl SetToNextOperand {
    pub fn decode(bytes: [u8; 8]) -> Self {
        Self {
            guid: dword(&bytes, 0),
            search: bytes[4] & 127,
            target: if bytes[4] & 128 == 0 {
                Variable::new(Scope::StackObjectId, 0)
            } else {
                Variable {
                    scope: bytes[5] as u16,
                    data: bytes[7] as i16,
                }
            },
            local: bytes[6],
        }
    }
}
pub fn set_to_next<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
    bytes: [u8; 8],
) -> Result<PrimitiveExit, VmFault> {
    let op = SetToNextOperand::decode(bytes);
    let target = read_variable(thread, host, op.target)?;
    let pointer = host
        .resolve_entity(ObjectId(target))
        .map(|entity| host.entity_info(entity))
        .transpose()?;
    if op.search == 3 {
        let Some(pointer) = pointer else {
            return Ok(PrimitiveExit::GotoFalse);
        };
        if !pointer.multi_tile {
            return Ok(PrimitiveExit::GotoFalse);
        }
        let next = pointer
            .group
            .iter()
            .copied()
            .filter(|id| id.0 > target)
            .min()
            .or_else(|| pointer.group.iter().copied().min());
        return if let Some(next) = next {
            write_target(thread, host, op.target, next.0)
        } else {
            Ok(PrimitiveExit::GotoFalse)
        };
    }
    if [5, 7, 10].contains(&op.search) {
        return match host.resolve_neighbor_or_career(thread.mode, op.search, target, op.guid)? {
            Some(next) => write_target(thread, host, op.target, next),
            None => Ok(PrimitiveExit::GotoFalse),
        };
    }
    if op.search == 9 {
        let frame = thread.top()?;
        let anchor = resolve(
            host,
            frame.locals[index("adjacent anchor local", op.local as i32, frame.locals.len())?],
        )?;
        let anchor = host.entity_info(anchor)?;
        let start = if let Some(pointer) = pointer {
            let dx = (pointer.position.x >> 4) - (anchor.position.x >> 4);
            let dy = (pointer.position.y >> 4) - (anchor.position.y >> 4);
            (if dx == 0 {
                if dy < 0 {
                    0
                } else {
                    2
                }
            } else if dx < 0 {
                3
            } else {
                1
            }) + 1
        } else {
            0
        };
        let all = infos(host)?;
        for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)].into_iter().skip(start) {
            if let Some(next) = all.iter().find(|e| {
                e.position.level == anchor.position.level
                    && (e.position.x >> 4) == (anchor.position.x >> 4) + dx
                    && (e.position.y >> 4) == (anchor.position.y >> 4) + dy
            }) {
                return write_target(thread, host, op.target, next.reference.object_id.0);
            }
        }
        return Ok(PrimitiveExit::GotoFalse);
    }
    if op.search == 11 {
        return Ok(PrimitiveExit::GotoFalse);
    }
    if op.search == 8 && pointer.is_none() {
        return Ok(PrimitiveExit::GotoFalse);
    }
    if op.search == 12 && pointer.is_none() {
        return Err(VmFault::MissingEntity(ObjectId(target)));
    }
    let semiglobal = if op.search == 100 {
        host.semiglobal_for_guid(op.guid)?
    } else {
        None
    };
    if op.search == 100 && semiglobal.is_none() {
        return Ok(PrimitiveExit::GotoFalse);
    }
    let category = if op.search == 6 {
        {
            let frame = thread.top()?;
            frame.args[index("set-to-next family parameter", 0, frame.args.len())?]
        }
    } else {
        0
    };
    let all = infos(host)?;
    let candidates = all
        .iter()
        .filter(|entity| match op.search {
            1 | 12 => entity.is_avatar,
            4 => entity.guid == op.guid,
            6 => entity.category == category,
            8 => same_tile(entity.position, pointer.as_ref().expect("checked").position),
            100 => entity.semiglobal == semiglobal,
            _ => true,
        })
        .collect::<Vec<_>>();
    let acceptable = |entity: &&EntityInfo| match op.search {
        2 => !entity.is_avatar,
        12 => entity.family == pointer.as_ref().expect("checked").family,
        _ => true,
    };
    if let Some(next) = candidates
        .iter()
        .copied()
        .filter(|e| e.reference.object_id.0 > target)
        .find(acceptable)
    {
        return write_target(thread, host, op.target, next.reference.object_id.0);
    }
    if op.search == 8 || op.search == 12 {
        let first = if op.search == 12 {
            candidates
                .iter()
                .copied()
                .find(acceptable)
                .or_else(|| candidates.first().copied())
        } else {
            candidates.first().copied()
        };
        if candidates.iter().any(|e| e.reference.object_id.0 == target) {
            if let Some(next) = first {
                return write_target(thread, host, op.target, next.reference.object_id.0);
            }
        }
    }
    Ok(PrimitiveExit::GotoFalse)
}

pub fn create<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
    bytes: [u8; 8],
) -> Result<PrimitiveExit, VmFault> {
    let mode = bytes[4];
    let flags = bytes[5];
    // These require nested callbacks or durable authority; reject before allocating/mutating.
    if flags & (4 | 64 | 128) != 0 || mode == 5 {
        return Err(VmFault::UnsupportedPrimitive {
            opcode: 42,
            case: "Neighbor/callback/persistence/underneath collision-retry create mode".into(),
        });
    }
    let frame = thread.top()?.clone();
    let caller = host.entity_info(frame.context.caller)?;
    let mut position = OUT_OF_WORLD;
    let mut direction = 0u8;
    let base = match mode {
        0 | 1 | 2 | 9 => Some(caller.clone()),
        3 | 4 => Some(host.entity_info(stack_entity(thread, host)?)?),
        6 => None,
        7 => Some(host.entity_info(resolve(
            host,
            frame.args[index("create object parameter", 0, frame.args.len())?],
        )?)?),
        8 => Some(host.entity_info(resolve(
            host,
            frame.locals[index("create local", bytes[6] as i32, frame.locals.len())?],
        )?)?),
        _ => {
            return Err(VmFault::InvalidOperand {
                opcode: 42,
                detail: format!("Unknown position {mode}"),
            })
        }
    };
    if let Some(base) = base {
        direction = base.direction;
        if mode != 2 && mode != 4 {
            position = base.position;
        }
    }
    if direction > 7 {
        return Err(VmFault::InvalidOperand {
            opcode: 42,
            detail: "Direction exceeds the eight canonical notches".into(),
        });
    }
    if mode == 0 || mode == 3 {
        match direction {
            0 => position.y = position.y.wrapping_sub(16),
            2 => position.x = position.x.wrapping_add(16),
            4 => position.y = position.y.wrapping_add(16),
            6 => position.x = position.x.wrapping_sub(16),
            _ => {}
        }
    }
    if mode == 9 {
        let local = frame.locals[index(
            "create direction local",
            bytes[6] as i32,
            frame.locals.len(),
        )?];
        let caller_direction = host.read_memory(&MemoryAddress::Entity {
            entity: frame.context.caller,
            field: EntityField::ObjectData,
            index: 1,
        })?;
        let local = local.wrapping_add((caller_direction / 2) * 2) % 8;
        direction = 0;
        match local {
            0 => position.y = position.y.wrapping_sub(16),
            2 => {
                direction = 2;
                position.x = position.x.wrapping_add(16)
            }
            4 => {
                direction = 4;
                position.y = position.y.wrapping_add(16)
            }
            6 => {
                direction = 6;
                position.x = position.x.wrapping_sub(16)
            }
            _ => {}
        }
    }
    let guid = dword(&bytes, 0);
    if flags & 1 != 0
        && mode != 2
        && mode != 4
        && infos(host)?
            .iter()
            .any(|e| same_tile(e.position, position) && e.guid == guid)
    {
        return Ok(PrimitiveExit::GotoFalse);
    }
    let main_parameter = if flags & 2 != 0 && frame.context.stack_object_ref.is_some() {
        frame.context.stack_object
    } else {
        ObjectId(0)
    };
    let main_stack_object = if flags & 16 != 0 {
        ObjectId(thread.temps[0])
    } else if flags & 2 != 0 {
        frame.context.caller.object_id
    } else {
        ObjectId(0)
    };
    let created = match host.entity_operation(EntityOperation::Create {
        guid,
        position,
        direction,
        main_parameter,
        main_stack_object,
    })? {
        EntityOperationResult::Created(created) => created,
        _ => return Err(VmFault::HostUnsupported("Create response type".into())),
    };
    let Some(created) = created else {
        return Ok(PrimitiveExit::GotoFalse);
    };
    if mode == 2 || mode == 4 {
        let container = if mode == 2 {
            frame.context.caller
        } else {
            stack_entity(thread, host)?
        };
        bool_operation(
            host,
            EntityOperation::PlaceInSlot {
                container,
                object: Some(created),
                slot: 0,
                clean_old: true,
            },
        )?;
    }
    if mode != 6 && host.entity_info(created)?.position == OUT_OF_WORLD {
        let container = host.read_memory(&MemoryAddress::Entity {
            entity: created,
            field: EntityField::ObjectData,
            index: 2,
        })?;
        if container == 0 {
            bool_operation(
                host,
                EntityOperation::Delete {
                    target: created,
                    cleanup_all: true,
                    return_immediately: false,
                },
            )?;
            return Ok(PrimitiveExit::GotoFalse);
        }
    }
    let frame = thread.top_mut()?;
    frame.context.stack_object = created.object_id;
    frame.context.stack_object_ref = Some(created);
    Ok(PrimitiveExit::GotoTrue)
}

pub fn terrain_info<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
    bytes: [u8; 8],
) -> Result<PrimitiveExit, VmFault> {
    let target = if bytes[2] & 2 != 0 {
        thread.top()?.context.caller
    } else {
        stack_entity(thread, host)?
    };
    let position = host.entity_info(target)?.position;
    if position == OUT_OF_WORLD {
        return Ok(PrimitiveExit::GotoFalse);
    }
    match bytes[0] {
        0 | 1 => {
            return Err(VmFault::UnsupportedPrimitive {
                opcode: 63,
                case: "Terrain height/slope adapter required".into(),
            })
        }
        2 => {
            thread.temps[0] = position.x >> 4;
            thread.temps[1] = position.x % 16;
            thread.temps[2] = position.y >> 4;
            thread.temps[3] = position.y % 16;
            thread.temps[4] = position.level as i16;
        }
        3 | 4 => thread.temps[0] = 0,
        _ => {}
    }
    Ok(PrimitiveExit::GotoTrue)
}
