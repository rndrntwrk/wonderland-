//! Exact field layouts from NetPlay/Model/Commands at FreeSO 4c6b3e8.
//! Unknown layout aborts the whole packet. There is no per-command byte length.
use crate::{
    reader::Reader, snapshot, Command, CommandBody, EodMessage, EodPayload, ErrorKind, Result,
    SyncTraceTick,
};
pub(crate) fn decode(r: &mut Reader<'_, '_>) -> Result<Command> {
    let offset = r.at;
    let kind = r.u8()?;
    if kind > 48 {
        return Err(r.error(ErrorKind::UnsupportedCommand(kind), "VMCommandType"));
    }
    let source_start = r.at;
    // These three source classes deliberately omit VMNetCommandBodyAbstract.
    let actor_uid = if [5, 12, 13].contains(&kind) {
        None
    } else {
        Some(r.u32()?)
    };
    let body = match kind {
        18 => {
            let actor_uid = actor_uid.unwrap();
            let plugin_id = r.u32()?;
            let event_name = r.text()?;
            let binary = r.boolean()?;
            let payload = if binary {
                let n = usize::from(r.u16()?);
                EodPayload::Binary(r.take(n)?.to_vec())
            } else {
                EodPayload::Text(r.text()?)
            };
            CommandBody::EodMessage(EodMessage {
                actor_uid,
                plugin_id,
                event_name,
                payload,
            })
        }
        12 => {
            let snapshot = snapshot::read_snapshot(r)?;
            let traces = if r.boolean()? {
                let n = r.count(8)?; // trace tick ID and string count
                let mut traces = Vec::with_capacity(n);
                for _ in 0..n {
                    let tick_id = r.u32()?;
                    let n = r.count(1)?; // empty .NET string still has a length byte
                    let mut trace = Vec::with_capacity(n);
                    for _ in 0..n {
                        trace.push(r.text()?);
                    }
                    traces.push(SyncTraceTick { tick_id, trace });
                }
                Some(traces)
            } else {
                None
            };
            CommandBody::StateSync {
                snapshot: Box::new(snapshot),
                traces,
            }
        }
        _ => {
            fields(r, kind)?;
            CommandBody::SourceFields {
                bytes: r.bytes[source_start..r.at].to_vec(),
            }
        }
    };
    Ok(Command {
        kind,
        actor_uid,
        offset,
        consumed: r.at - offset,
        body,
    })
}
fn fields(r: &mut Reader<'_, '_>, kind: u8) -> Result<()> {
    match kind {
        0 => avatar_join(r)?,
        1 => {
            r.u16()?;
            r.i16()?;
            r.i16()?;
            r.boolean()?;
            r.i16()?;
        }
        2 => {
            let n = r.count(22)?; // VMArchitectureCommand
            for _ in 0..n {
                let kind = r.u8()?;
                if kind > 9 {
                    return Err(r.error(ErrorKind::Invalid, "architecture command type"));
                }
                r.i32()?;
                r.i32()?;
                r.i8()?;
                r.i32()?;
                r.i32()?;
                r.u16()?;
                r.u16()?;
            }
        }
        3 => {
            r.u32()?;
            r.i16()?;
            r.i16()?;
            r.i8()?;
            r.u8()?;
            r.i32()?;
            r.u8()?;
            r.u8()?;
        }
        4 => {
            r.text()?;
            r.u8()?;
        }
        5 => {
            let n = r.byte_length()?;
            if n == 0 {
                return Err(r.error(
                    ErrorKind::UnsupportedCommand(5),
                    "null BlueprintRestore serializer omits following fields",
                ));
            }
            r.take(n)?;
            r.i16()?;
            for _ in 0..7 {
                r.i32()?;
            }
        }
        6 | 41 => {}
        7 => {
            r.u16()?;
        }
        8 => {
            r.i16()?;
            r.i16()?;
            r.i16()?;
            r.i8()?;
            r.u8()?;
        }
        9 => {
            r.i16()?;
            r.u32()?;
            r.boolean()?;
            r.boolean()?;
            r.u8()?;
        }
        10 => {
            r.u16()?;
            r.i16()?;
            r.i16()?;
            r.i16()?;
            r.i8()?;
        }
        11 => {
            r.u8()?;
            r.text()?;
        }
        13 => {
            r.u32()?;
            r.u32()?;
        }
        14 => {
            r.text()?;
            r.text()?;
        }
        15 => {
            r.i16()?;
            snapshot::async_state(r)?;
        }
        16 => {
            r.u32()?;
            r.u32()?;
            r.u8()?;
            r.u8()?;
        }
        17 => {
            r.i16()?;
            snapshot::eod_object_event(r)?;
        }
        19 => {
            r.i16()?;
            r.u32()?;
        }
        20 => {
            let n = r.count(1)?; // nullable adjacent-lot byte arrays
            for _ in 0..n {
                if r.boolean()? {
                    r.bytes_i32()?;
                }
            }
        }
        21 | 28 => {
            r.u32()?;
            r.boolean()?;
        }
        22 => inventory_place(r)?,
        23 => {
            let n = r.count(36)?; // inventory item with empty name/attributes
            for _ in 0..n {
                inventory_item(r)?;
            }
        }
        24 => {
            for _ in 0..2 {
                let n = r.count_max(40, 4)?;
                for _ in 0..n {
                    r.u32()?;
                }
            }
        }
        25 => {
            r.u8()?;
            r.u8()?;
        }
        26 => {
            r.u16()?;
            r.boolean()?;
        }
        27 => {
            r.u32()?;
            r.i32()?;
        }
        29 | 35 => {
            r.i16()?;
        }
        30 => {
            r.u8()?;
            r.i16()?;
        }
        31 => {
            r.u32()?;
            r.boolean()?;
        }
        32 => {
            r.f32()?;
            r.u32()?;
        }
        33 => {
            r.u32()?;
            r.i16()?;
            r.u64()?;
        }
        34 => {
            r.i32()?;
        }
        36 => {
            r.i32()?;
            r.i32()?;
            r.i32()?;
            r.i64()?;
        }
        37 => {
            snapshot::tuning(r)?;
        }
        38 => {
            let n = r.count(3)?; // each Int16 object ID has a corresponding graphic byte
            r.short_values(n)?;
            r.take(n)?;
        }
        39 => {
            r.i8()?;
            r.u32()?;
        }
        40 => {
            snapshot::chat_channel(r)?;
        }
        42 => {
            r.u32()?;
            r.u8()?;
            r.i32()?;
        }
        43 => {
            r.u8()?;
            r.i32()?;
            r.boolean()?;
            r.u8()?;
        }
        44 => {
            let partial = r.boolean()?;
            if !partial {
                r.i32()?;
                r.i16()?;
                r.i32()?;
                r.i16()?;
            }
            for _ in 0..3 {
                r.f32()?;
            }
            if !partial {
                r.boolean()?;
            }
        }
        45 => {
            r.boolean()?;
        }
        46 => {
            r.boolean()?;
            r.i32()?;
            for _ in 0..6 {
                r.f32()?;
            }
            for _ in 0..6 {
                r.i16()?;
            }
            r.u16()?;
            r.i32()?;
            r.i32()?;
        }
        47 => {
            r.i32()?;
            r.u32()?;
        }
        48 => {
            r.bytes_i32()?;
        }
        _ => {
            return Err(r.error(
                ErrorKind::UnsupportedCommand(kind),
                "VM command source layout",
            ))
        }
    };
    Ok(())
}
fn avatar_join(r: &mut Reader<'_, '_>) -> Result<()> {
    let version = r.u16()?;
    if version != 0xffee {
        return Err(r.error(
            ErrorKind::UnsupportedVersion(i32::from(version)),
            "VMNetSimJoin version",
        ));
    }
    let version = r.i32()?;
    if version != 5 {
        return Err(r.error(
            ErrorKind::UnsupportedVersion(version),
            "VMNetAvatarPersistState",
        ));
    }
    r.text()?;
    r.u32()?;
    for _ in 0..3 {
        snapshot::outfit(r)?;
    }
    r.u8()?;
    r.u32()?;
    r.u32()?;
    r.u64()?;
    r.u64()?;
    r.u8()?;
    r.boolean()?;
    r.u32()?;
    r.short_values(16)?;
    r.short_values(27)?;
    snapshot::persistent_relationships(r)?;
    snapshot::jobs(r)?;
    let n = r.count(4)?;
    for _ in 0..n {
        r.u32()?;
    }
    Ok(())
}
fn inventory_place(r: &mut Reader<'_, '_>) -> Result<()> {
    r.u32()?;
    r.i16()?;
    r.i16()?;
    r.i8()?;
    r.u8()?;
    r.u32()?;
    r.u32()?;
    r.u8()?;
    r.u8()?;
    r.i32()?;
    let n = r.byte_length()?;
    if n > 4096 {
        return Err(r.error(ErrorKind::Limit, "VM inventory restore data bytes"));
    }
    r.take(n)?;
    let n = r.byte_count(2)?;
    r.short_values(n)?;
    r.u8()?;
    Ok(())
}
fn inventory_item(r: &mut Reader<'_, '_>) -> Result<()> {
    r.u32()?;
    r.text()?;
    r.u32()?;
    r.u16()?;
    r.u32()?;
    r.u64()?;
    r.u64()?;
    r.u8()?;
    let n = r.count(4)?;
    for _ in 0..n {
        r.i32()?;
    }
    Ok(())
}
