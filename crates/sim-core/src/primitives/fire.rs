use crate::ids::{EntityRef, ObjectId};
use crate::vm::*;
use std::collections::VecDeque;

/// VMBurn.FIRE_GUID from the pinned source, shared by TS1 and TSO in this revision.
pub const FIRE_GUID: u32 = 0x24c95f99;
const OUT_OF_WORLD: VmPosition = VmPosition {
    x: i16::MIN,
    y: i16::MIN,
    level: 1,
};
fn value<H: VmHost + ?Sized>(
    host: &H,
    entity: EntityRef,
    field: EntityField,
    index: u16,
) -> Result<i16, VmFault> {
    host.read_memory(&MemoryAddress::Entity {
        entity,
        field,
        index,
    })
}
fn existing_fire<H: VmHost + ?Sized>(host: &H, entities: &[EntityRef]) -> Result<bool, VmFault> {
    for entity in entities {
        if host.entity_info(*entity)?.guid == FIRE_GUID {
            return Ok(true);
        }
    }
    Ok(false)
}
fn create_fire<H: VmHost + ?Sized>(host: &mut H, position: VmPosition) -> Result<(), VmFault> {
    let position = VmPosition {
        x: ((position.x >> 4) << 4).wrapping_add(8),
        y: ((position.y >> 4) << 4).wrapping_add(8),
        level: position.level,
    };
    match host.entity_operation(EntityOperation::Create {
        guid: FIRE_GUID,
        position,
        direction: 0,
        main_parameter: ObjectId(0),
        main_stack_object: ObjectId(0),
    })? {
        EntityOperationResult::Created(_) => Ok(()), // Source ignores a missing FIRE_GUID definition/allocation result.
        _ => Err(VmFault::HostUnsupported(
            "Fire creation response type".into(),
        )),
    }
}
pub fn burn<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
    bytes: [u8; 8],
) -> Result<PrimitiveExit, VmFault> {
    let state = host.fire_state()?;
    if thread.mode == VmMode::Tso && !state.enabled {
        return Ok(PrimitiveExit::GotoFalse);
    }
    let spread = bytes[0] != 0;
    if spread && host.next_random(10000) as i32 >= state.percent {
        return Ok(PrimitiveExit::GotoFalse);
    }
    if host.room_is_pool(thread.top()?.context.caller)? {
        return Ok(PrimitiveExit::GotoFalse);
    }
    let percent = if spread {
        state.percent.wrapping_sub(500)
    } else {
        state.percent
    };
    if spread || percent < 0 {
        host.set_fire_percent(percent.max(0))?;
    }
    let mut fires = vec![false; state.tile_count()?];
    let stack = host.entity_info(stack_entity(thread, host)?)?;
    let mut position = stack.position;
    if bytes[0] == 1 {
        match stack.direction {
            0 => position.y = position.y.wrapping_sub(16),
            2 => position.x = position.x.wrapping_add(16),
            4 => position.y = position.y.wrapping_add(16),
            6 => position.x = position.x.wrapping_sub(16),
            1 | 3 | 5 | 7 => {}
            _ => {
                return Err(VmFault::InvalidOperand {
                    opcode: 9,
                    detail: "Direction exceeds eight canonical notches".into(),
                })
            }
        }
    }
    if position == OUT_OF_WORLD {
        return Ok(PrimitiveExit::GotoFalse);
    }
    let mut queue = VecDeque::from([position]);
    let mut first = true;
    let mut made_fire = false;
    let mut visits = 0usize;
    while let Some(item) = queue.pop_front() {
        if item == OUT_OF_WORLD {
            continue;
        }
        let mut objects = host.objects_at_tile(item)?;
        if first && !existing_fire(host, &objects)? {
            let existing_list = !objects.is_empty();
            made_fire = true;
            create_fire(host, position)?;
            // An absent source tile uses a detached empty fallback list. Existing tile
            // lists are shared and therefore include objects created before foreach.
            if existing_list {
                objects = host.objects_at_tile(item)?;
            }
        }
        first = false;
        for object in objects {
            let info = host.entity_info(object)?;
            if host.function_status(object)?.in_use
                || (info.is_avatar && value(host, object, EntityField::PersonData, 68)? > 0)
            {
                continue;
            }
            for id in info.group {
                visits += 1;
                if visits > MAX_FIRE_GROUP_VISITS {
                    return Err(VmFault::InvalidContent(
                        "Fire traversal exceeds operation budget".into(),
                    ));
                }
                let entity = host.resolve_entity(id).ok_or(VmFault::MissingEntity(id))?;
                let info = host.entity_info(entity)?;
                if info.position.level != position.level {
                    continue;
                }
                let flags2 = value(host, entity, EntityField::ObjectData, 40)?;
                if flags2 & 32 == 0 {
                    continue;
                }
                let flags = value(host, entity, EntityField::ObjectData, 8)?;
                if flags & 2048 != 0 {
                    continue;
                }
                host.write_memory(
                    &MemoryAddress::Entity {
                        entity,
                        field: EntityField::ObjectData,
                        index: 8,
                    },
                    flags | 512,
                )?;
                let target = host.entity_info(entity)?.position;
                let at = (target.x >> 4) as i32 + ((target.y >> 4) as i32) * i32::from(state.width);
                let at = index("fire tile", at, fires.len())?;
                if fires[at] || existing_fire(host, &host.objects_at_tile(target)?)? {
                    fires[at] = true;
                    continue;
                }
                queue.push_back(target);
                made_fire = true;
                create_fire(host, target)?;
                fires[at] = true;
            }
        }
    }
    // BurnBusyObjects (byte1) is read by the source operand but never used by Execute.
    Ok(PrimitiveExit::branch(made_fire))
}
