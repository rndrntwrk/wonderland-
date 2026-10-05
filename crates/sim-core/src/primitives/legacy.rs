use super::{dword, word};
use crate::ids::ObjectId;
use crate::vm::*;

pub fn family_budget<H: VmHost + ?Sized>(
    thread: &VmThread,
    host: &mut H,
    bytes: [u8; 8],
) -> Result<PrimitiveExit, VmFault> {
    let scope = match bytes[0] {
        0 => 7,
        1 => 9,
        2 => 25,
        _ => u16::from(bytes[1]),
    };
    let mut amount = read_big_variable(
        thread,
        host,
        Variable {
            scope,
            data: word(&bytes, 2),
        },
    )?;
    if bytes[4] & 2 != 0 {
        amount = amount.wrapping_neg();
    }
    let family = host.ts1_family_budget()?;
    let old = family.unwrap_or(0);
    let new = old.wrapping_sub(amount);
    if old < 0 {
        return Ok(PrimitiveExit::GotoFalse);
    }
    if bytes[4] & 1 == 0 && family.is_some() {
        host.set_ts1_family_budget(new)?;
    }
    Ok(PrimitiveExit::GotoTrue)
}

pub fn inventory<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
    bytes: [u8; 8],
) -> Result<PrimitiveExit, VmFault> {
    let mode = bytes[0];
    let token_type = i32::from(bytes[1]);
    let flags = bytes[2];
    let flags2 = bytes[3];
    let target = if flags2 & 32 != 0 {
        host.resolve_entity(ObjectId(thread.temps[4]))
            .ok_or(VmFault::MissingEntity(ObjectId(thread.temps[4])))?
    } else {
        thread.top()?.context.caller
    };
    if !host.entity_info(target)?.is_avatar {
        return Err(VmFault::InvalidOperand {
            opcode: 51,
            detail: "TS1 inventory owner must be an avatar".into(),
        });
    }
    let neighbor = host.read_memory(&MemoryAddress::Entity {
        entity: target,
        field: EntityField::PersonData,
        index: 31,
    })?;
    let mut inventory = host.ts1_inventory_read(neighbor)?;
    if inventory
        .as_ref()
        .map_or(false, |items| items.len() > MAX_TS1_INVENTORY_ITEMS)
    {
        return Err(VmFault::InvalidContent("TS1 inventory item limit".into()));
    }
    let count = if flags & 2 != 0 {
        i32::from(thread.temps[0])
    } else {
        1
    };
    let index_temp = (flags2 & 3) as usize;
    let count_temp = ((flags >> 2) & 3) as usize;
    let start = thread.temps[index_temp];
    // Source resolves a zero GUID even for operations which later ignore it.
    let raw_guid = dword(&bytes, 4);
    let guid = if raw_guid == 0 {
        host.entity_info(stack_entity(thread, host)?)?.guid
    } else {
        raw_guid
    };
    match mode {
        0 => {
            if inventory.is_none() {
                host.ts1_inventory_write(neighbor, Vec::new())?;
                inventory = Some(Vec::new());
            }
            let items = inventory.as_mut().expect("initialized");
            if let Some(item) = items
                .iter_mut()
                .find(|item| item.guid == guid && item.token_type == token_type)
            {
                item.count = item.count.wrapping_add(count as u16);
            } else {
                if items.len() >= MAX_TS1_INVENTORY_ITEMS {
                    return Err(VmFault::InvalidContent("TS1 inventory item limit".into()));
                }
                items.push(Ts1InventoryItem {
                    token_type,
                    guid,
                    count: count as u16,
                });
            }
            host.ts1_inventory_write(neighbor, inventory.expect("initialized"))?;
        }
        1 => {
            let Some(mut items) = inventory else {
                return Ok(PrimitiveExit::GotoFalse);
            };
            let Some(at) = items.iter().position(|item| {
                item.guid == guid && (token_type == 0 || item.token_type == token_type)
            }) else {
                return Ok(PrimitiveExit::GotoFalse);
            };
            if i32::from(items[at].count) < count {
                return Ok(PrimitiveExit::GotoFalse);
            }
            let amount = if count == -1 {
                items[at].count
            } else {
                count as u16
            };
            items[at].count = items[at].count.wrapping_sub(amount);
            if items[at].count == 0 {
                items.remove(at);
            }
            host.ts1_inventory_write(neighbor, items)?;
        }
        2 => {
            let Some(mut items) = inventory else {
                return Ok(PrimitiveExit::GotoFalse);
            };
            if start < 0 || start as usize >= items.len() {
                return Ok(PrimitiveExit::GotoFalse);
            }
            let at = start as usize;
            let amount = if count == -1 {
                items[at].count
            } else {
                count as u16
            };
            items[at].count = items[at].count.wrapping_sub(amount);
            if items[at].count == 0 {
                items.remove(at);
            }
            host.ts1_inventory_write(neighbor, items)?;
            if flags & 128 != 0 {
                thread.temps[index_temp] = start.wrapping_sub(1);
            }
        }
        3 => {
            let Some(items) = inventory else {
                return Ok(PrimitiveExit::GotoFalse);
            };
            let Some(at) = items.iter().position(|item| {
                item.guid == guid && (token_type == 0 || item.token_type == token_type)
            }) else {
                return Ok(PrimitiveExit::GotoFalse);
            };
            let count = items[at].count as i16;
            thread.temps[count_temp] = count;
            if flags & 16 != 0 {
                thread.temps[index_temp] = at as i16;
            }
            return Ok(PrimitiveExit::branch(count > 0));
        }
        4 => {
            let Some(items) = inventory else {
                return Ok(PrimitiveExit::GotoFalse);
            };
            let total = items
                .iter()
                .filter(|item| item.token_type == token_type)
                .try_fold(0i32, |sum, item| sum.checked_add(i32::from(item.count)))
                .ok_or_else(|| VmFault::Arithmetic("TS1 inventory Sum(int) overflow".into()))?;
            thread.temps[count_temp] = total as i16;
            let Some(at) = items.iter().enumerate().position(|(at, item)| {
                item.token_type == token_type && (at as i32) > i32::from(start)
            }) else {
                return Ok(PrimitiveExit::GotoFalse);
            };
            if flags & 128 != 0 {
                thread.temps[index_temp] = at as i16;
            }
        }
        5 | 6 => {
            if inventory.is_none() {
                host.ts1_inventory_write(neighbor, Vec::new())?;
                inventory = Some(Vec::new());
            }
            let items = inventory.as_mut().expect("initialized");
            let find_guid = u32::from(mode - 5);
            if let Some(item) = items
                .iter_mut()
                .find(|item| item.guid == find_guid && item.token_type == 2)
            {
                item.count = count as u16;
            } else {
                if items.len() >= MAX_TS1_INVENTORY_ITEMS {
                    return Err(VmFault::InvalidContent("TS1 inventory item limit".into()));
                }
                items.push(Ts1InventoryItem {
                    token_type: 2,
                    guid: find_guid + 10,
                    count: thread.temps[0] as u16,
                });
            }
            host.ts1_inventory_write(neighbor, inventory.expect("initialized"))?;
        }
        _ => {} // Source mode8 and unknown modes have no switch body after operand reads.
    }
    Ok(PrimitiveExit::GotoTrue)
}
