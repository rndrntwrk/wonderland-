//! Scope numbers and access order follow Engine/{Scopes/VMVariableScope,VMMemory}.cs.
use super::*;
use crate::ids::{EntityRef, ObjectId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum Scope {
    MyObjectAttributes = 0,
    StackObjectAttributes = 1,
    TargetObjectAttributes = 2,
    MyObject = 3,
    StackObject = 4,
    TargetObject = 5,
    Global = 6,
    Literal = 7,
    Temps = 8,
    Parameters = 9,
    StackObjectId = 10,
    TempByTemp = 11,
    TreeAdRange = 12,
    StackObjectTemp = 13,
    MyMotives = 14,
    StackObjectMotives = 15,
    StackObjectSlot = 16,
    StackObjectMotiveByTemp = 17,
    MyPersonData = 18,
    StackObjectPersonData = 19,
    MySlot = 20,
    StackObjectDefinition = 21,
    StackObjectAttributeByParameter = 22,
    RoomByTemp0 = 23,
    NeighborInStackObject = 24,
    Local = 25,
    Tuning = 26,
    DynSpriteFlagForTempOfStackObject = 27,
    TreeAdPersonalityVar = 28,
    TreeAdMin = 29,
    MyPersonDataByTemp = 30,
    StackObjectPersonDataByTemp = 31,
    NeighborPersonData = 32,
    JobData = 33,
    NeighborhoodData = 34,
    StackObjectFunction = 35,
    MyTypeAttr = 36,
    StackObjectTypeAttr = 37,
    NeighborsObjectDefinition = 38,
    Unused = 39,
    LocalByTemp = 40,
    StackObjectAttributeByTemp = 41,
    TempXl = 42,
    CityTime = 43,
    TsoStandardTime = 44,
    GameTime = 45,
    MyList = 46,
    StackObjectList = 47,
    MoneyOverHead32Bit = 48,
    MyLeadTileAttribute = 49,
    StackObjectLeadTileAttribute = 50,
    MyLeadTile = 51,
    StackObjectLeadTile = 52,
    StackObjectMasterDef = 53,
    FeatureEnableLevel = 54,
    MyAvatarId = 59,
    Invalid = 255,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Variable {
    pub scope: u16,
    pub data: i16,
}
impl Variable {
    pub const fn new(scope: Scope, data: i16) -> Self {
        Self {
            scope: scope as u16,
            data,
        }
    }
}

pub(crate) fn index(area: &str, value: i32, len: usize) -> Result<usize, VmFault> {
    if value < 0 || value as usize >= len {
        Err(VmFault::Bounds {
            area: area.into(),
            index: value,
            len,
        })
    } else {
        Ok(value as usize)
    }
}
pub(crate) fn temp(thread: &VmThread, data: i16) -> Result<i16, VmFault> {
    Ok(thread.temps[index("temp", data as i32, 20)?])
}
pub fn stack_entity<H: VmHost + ?Sized>(thread: &VmThread, host: &H) -> Result<EntityRef, VmFault> {
    let context = &thread.top()?.context;
    let entity = context
        .stack_object_ref
        .ok_or(VmFault::MissingEntity(context.stack_object))?;
    if entity.object_id != context.stack_object
        || host.resolve_entity(entity.object_id) != Some(entity)
    {
        return Err(VmFault::StaleEntity(entity));
    }
    Ok(entity)
}
fn entity_address(entity: EntityRef, field: EntityField, index: i16) -> MemoryAddress {
    MemoryAddress::Entity {
        entity,
        field,
        index: index as u16,
    }
}
fn lead_entity<H: VmHost + ?Sized>(host: &H, entity: EntityRef) -> Result<EntityRef, VmFault> {
    let id = host.entity_info(entity)?.base_object;
    host.resolve_entity(id).ok_or(VmFault::MissingEntity(id))
}
fn neighborhood(thread: &VmThread, scope: u16, data: i16) -> Result<MemoryAddress, VmFault> {
    if thread.mode != VmMode::Ts1 && [24, 32, 33, 38].contains(&scope) {
        return Err(VmFault::UnsupportedScope {
            scope,
            write: false,
        });
    }
    Ok(MemoryAddress::Neighborhood {
        scope,
        object_id: thread.top()?.context.stack_object,
        data,
        temp0: thread.temps[0],
        temp1: thread.temps[1],
    })
}
pub fn list_entity<H: VmHost + ?Sized>(
    thread: &VmThread,
    host: &H,
    scope: u16,
) -> Result<EntityRef, VmFault> {
    match scope {
        46 => Ok(thread.top()?.context.caller),
        47 => stack_entity(thread, host),
        _ => Err(VmFault::UnsupportedScope {
            scope,
            write: false,
        }),
    }
}

pub fn read_variable<H: VmHost + ?Sized>(
    thread: &VmThread,
    host: &H,
    var: Variable,
) -> Result<i16, VmFault> {
    let data = var.data;
    let scope = var.scope;
    let frame = thread.top()?;
    let caller = frame.context.caller;
    let address = match scope {
        0 => entity_address(caller, EntityField::Attribute, data),
        1 => entity_address(stack_entity(thread, host)?, EntityField::Attribute, data),
        2 | 5 => return Err(VmFault::DeprecatedScope(scope)),
        3 => entity_address(caller, EntityField::ObjectData, data),
        4 => entity_address(stack_entity(thread, host)?, EntityField::ObjectData, data),
        6 => MemoryAddress::Global(data as u16),
        7 => return Ok(data),
        8 => return temp(thread, data),
        9 => return Ok(frame.args[index("parameter", data as i32, frame.args.len())?]),
        10 => return Ok(frame.context.stack_object.0),
        11 => return temp(thread, temp(thread, data)?),
        12 | 28 | 29 => {
            let kind = match scope {
                12 => 1,
                28 => 2,
                _ => 0,
            };
            if let Some(value) = thread.tree_advertisements.get(&(kind, data as u16)) {
                return Ok(*value);
            }
            MemoryAddress::TreeAdvertisement {
                kind,
                index: data as u16,
            }
        }
        13 => {
            let entity = stack_entity(thread, host)?;
            if entity == thread.owner && host.entity_temps_alias(thread.owner) {
                return temp(thread, data);
            }
            entity_address(entity, EntityField::Temp, data)
        }
        14 => entity_address(caller, EntityField::Motive, data),
        15 => {
            if frame.context.stack_object_ref.is_none() {
                return Ok(0);
            }
            let entity = stack_entity(thread, host)?;
            if !host.entity_info(entity)?.is_avatar {
                return Ok(0);
            }
            entity_address(entity, EntityField::Motive, data)
        }
        16 => entity_address(stack_entity(thread, host)?, EntityField::Slot, data),
        17 => entity_address(
            stack_entity(thread, host)?,
            EntityField::Motive,
            temp(thread, data)?,
        ),
        18 => entity_address(caller, EntityField::PersonData, data),
        19 => entity_address(stack_entity(thread, host)?, EntityField::PersonData, data),
        20 => entity_address(caller, EntityField::Slot, data),
        21 => entity_address(stack_entity(thread, host)?, EntityField::Definition, data),
        22 => entity_address(
            stack_entity(thread, host)?,
            EntityField::Attribute,
            frame.args[index("parameter", data as i32, frame.args.len())?],
        ),
        23 => {
            if !(0..=4).contains(&data) {
                return Err(VmFault::UnsupportedScope {
                    scope,
                    write: false,
                });
            }
            if data == 0 {
                return Ok(100);
            }
            MemoryAddress::Room {
                room: thread.temps[0] as i32 + 1,
                index: data,
            }
        }
        24 | 32 | 33 | 38 => neighborhood(thread, scope, data)?,
        25 => return Ok(frame.locals[index("local", data as i32, frame.locals.len())?]),
        26 => {
            let mut table = (data as u16) >> 7;
            let resource_mode = if table < 64 {
                0
            } else if table < 128 {
                table -= 64;
                1
            } else if table < 192 {
                table -= 128;
                2
            } else {
                0
            };
            table = table.wrapping_add([4096, 8192, 256][resource_mode as usize]);
            MemoryAddress::Tuning {
                callee: frame.context.callee,
                code_owner: frame.context.code_owner,
                table_id: table,
                key_id: (data as u16) & 127,
                resource_mode,
            }
        }
        27 => entity_address(
            stack_entity(thread, host)?,
            EntityField::DynamicSpriteFlag,
            temp(thread, data)?,
        ),
        30 => entity_address(caller, EntityField::PersonData, temp(thread, data)?),
        31 => entity_address(
            stack_entity(thread, host)?,
            EntityField::PersonData,
            temp(thread, data)?,
        ),
        34 => return Ok(0),
        35 => entity_address(stack_entity(thread, host)?, EntityField::Function, data),
        36 | 37 => {
            if thread.mode == VmMode::Tso {
                return Ok(0);
            }
            entity_address(
                if scope == 36 {
                    caller
                } else {
                    stack_entity(thread, host)?
                },
                EntityField::TypeAttribute,
                data,
            )
        }
        39 => MemoryAddress::MotiveLimit(data),
        40 => {
            return Ok(frame.locals[index("local", temp(thread, data)? as i32, frame.locals.len())?])
        }
        41 => entity_address(
            stack_entity(thread, host)?,
            EntityField::Attribute,
            temp(thread, data)?,
        ),
        42 => {
            return Err(VmFault::UnsupportedScope {
                scope,
                write: false,
            })
        }
        43..=45 => {
            if scope == 44 && !(0..=5).contains(&data) {
                return Ok(0);
            }
            if scope != 44 && !(0..=6).contains(&data) {
                return Err(VmFault::UnsupportedScope {
                    scope,
                    write: false,
                });
            }
            MemoryAddress::Clock {
                kind: match scope {
                    43 => ClockKind::City,
                    44 => ClockKind::Standard,
                    _ => ClockKind::Game,
                },
                index: data,
            }
        }
        46 | 47 => {
            if scope == 47 && frame.context.stack_object_ref.is_none() {
                return Ok(0);
            }
            let list = host.read_list(list_entity(thread, host, scope)?)?;
            if data == 2 {
                return Ok(list.len() as i16);
            }
            let at = match data {
                0 => 0,
                1 => list.len() as i32 - 1,
                _ => thread.temps[0] as i32,
            };
            return Ok(list[index("list", at, list.len())?]);
        }
        48 => return Ok(0),
        49 | 50 => entity_address(
            lead_entity(
                host,
                if scope == 49 {
                    caller
                } else {
                    stack_entity(thread, host)?
                },
            )?,
            EntityField::Attribute,
            data,
        ),
        51 | 52 => {
            return Ok(host
                .entity_info(if scope == 51 {
                    caller
                } else {
                    stack_entity(thread, host)?
                })?
                .base_object
                .0)
        }
        53 => entity_address(
            stack_entity(thread, host)?,
            EntityField::MasterDefinition,
            data,
        ),
        54 => return Ok(1),
        59 => {
            let entity = if data < 2 {
                caller
            } else {
                stack_entity(thread, host)?
            };
            let pid = host.entity_info(entity)?.persistent_id;
            return Ok(match data {
                0 | 2 => pid as i16,
                1 | 3 => (pid >> 16) as i16,
                _ => 0,
            });
        }
        _ => return Err(VmFault::UnknownScope(scope)),
    };
    host.read_memory(&address)
}

pub fn read_big_variable<H: VmHost + ?Sized>(
    thread: &VmThread,
    host: &H,
    var: Variable,
) -> Result<i32, VmFault> {
    if var.scope == 42 {
        Ok(thread.temp_xl[index("temp_xl", var.data as i32, 2)?])
    } else {
        Ok(read_variable(thread, host, var)? as i32)
    }
}

pub fn write_variable<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
    var: Variable,
    value: i16,
) -> Result<bool, VmFault> {
    let data = var.data;
    let scope = var.scope;
    let caller = thread.top()?.context.caller;
    let address = match scope {
        0 => entity_address(caller, EntityField::Attribute, data),
        1 => entity_address(stack_entity(thread, host)?, EntityField::Attribute, data),
        2 | 5 => return Err(VmFault::DeprecatedScope(scope)),
        3 => entity_address(caller, EntityField::ObjectData, data),
        4 => entity_address(stack_entity(thread, host)?, EntityField::ObjectData, data),
        6 => MemoryAddress::Global(data as u16),
        7 | 21 | 26 | 35 | 38 | 43..=45 | 51..=53 | 59 => return Ok(false),
        8 => {
            let at = index("temp", data as i32, 20)?;
            thread.temps[at] = value;
            return Ok(true);
        }
        9 => {
            let frame = thread.top_mut()?;
            let at = index("parameter", data as i32, frame.args.len())?;
            frame.args[at] = value;
            return Ok(true);
        }
        10 => {
            let frame = thread.top_mut()?;
            frame.context.stack_object = ObjectId(value);
            frame.context.stack_object_ref = host.resolve_entity(ObjectId(value));
            return Ok(true);
        }
        11 => {
            let at = index("temp", temp(thread, data)? as i32, 20)?;
            thread.temps[at] = value;
            return Ok(true);
        }
        12 | 28 | 29 => {
            let kind = match scope {
                12 => 1,
                28 => 2,
                _ => 0,
            };
            thread
                .tree_advertisements
                .insert((kind, data as u16), value);
            return Ok(true);
        }
        13 => {
            let entity = stack_entity(thread, host)?;
            if entity == thread.owner && host.entity_temps_alias(thread.owner) {
                let at = index("temp", data as i32, 20)?;
                thread.temps[at] = value;
                return Ok(true);
            }
            entity_address(entity, EntityField::Temp, data)
        }
        14 => entity_address(caller, EntityField::Motive, data),
        15 => {
            if thread.top()?.context.stack_object_ref.is_none() {
                return Ok(false);
            }
            let entity = stack_entity(thread, host)?;
            if !host.entity_info(entity)?.is_avatar {
                return Ok(false);
            }
            entity_address(entity, EntityField::Motive, data)
        }
        16 | 20 | 23 | 24 | 32..=34 | 39 | 42 | 46 | 47 | 54 => {
            return Err(VmFault::UnsupportedScope { scope, write: true })
        }
        17 => entity_address(
            stack_entity(thread, host)?,
            EntityField::Motive,
            temp(thread, data)?,
        ),
        18 => entity_address(caller, EntityField::PersonData, data),
        19 => entity_address(stack_entity(thread, host)?, EntityField::PersonData, data),
        22 => {
            let frame = thread.top()?;
            entity_address(
                stack_entity(thread, host)?,
                EntityField::Attribute,
                frame.args[index("parameter", data as i32, frame.args.len())?],
            )
        }
        25 => {
            let frame = thread.top_mut()?;
            let at = index("local", data as i32, frame.locals.len())?;
            frame.locals[at] = value;
            return Ok(true);
        }
        27 => entity_address(
            stack_entity(thread, host)?,
            EntityField::DynamicSpriteFlag,
            temp(thread, data)?,
        ),
        30 => entity_address(caller, EntityField::PersonData, temp(thread, data)?),
        31 => entity_address(
            stack_entity(thread, host)?,
            EntityField::PersonData,
            temp(thread, data)?,
        ),
        36 | 37 => {
            if thread.mode == VmMode::Tso {
                return Ok(true);
            }
            entity_address(
                if scope == 36 {
                    caller
                } else {
                    stack_entity(thread, host)?
                },
                EntityField::TypeAttribute,
                data,
            )
        }
        40 => {
            let at = temp(thread, data)?;
            let frame = thread.top_mut()?;
            let at = index("local", at as i32, frame.locals.len())?;
            frame.locals[at] = value;
            return Ok(true);
        }
        41 => entity_address(
            stack_entity(thread, host)?,
            EntityField::Attribute,
            temp(thread, data)?,
        ),
        48 => return Ok(true),
        49 | 50 => entity_address(
            lead_entity(
                host,
                if scope == 49 {
                    caller
                } else {
                    stack_entity(thread, host)?
                },
            )?,
            EntityField::Attribute,
            data,
        ),
        _ => return Err(VmFault::UnknownScope(scope)),
    };
    // Source dynamic-sprite setters take value > 0, not value != 0.
    let value = if scope == 27 {
        i16::from(value > 0)
    } else {
        value
    };
    host.write_memory(&address, value)
}
pub fn write_big_variable<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
    var: Variable,
    value: i32,
) -> Result<bool, VmFault> {
    match var.scope {
        42 => {
            let at = index("temp_xl", var.data as i32, 2)?;
            thread.temp_xl[at] = value;
            Ok(true)
        }
        48 => {
            host.show_money_headline(thread.top()?.context.caller, value)?;
            Ok(true)
        }
        _ => write_variable(thread, host, var, value as i16),
    }
}
