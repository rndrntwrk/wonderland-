use super::{uword, word};
use crate::avatars::outfits::OutfitReference;
use crate::ids::ObjectId;
use crate::vm::*;

fn string_lookup(
    thread: &VmThread,
    source: StringSource,
    table: u16,
    index: i32,
) -> Result<StringLookup, VmFault> {
    let frame = thread.top()?;
    Ok(StringLookup {
        context: frame.context.clone(),
        source,
        table,
        index,
        params: frame.args.clone(),
        locals: frame.locals.clone(),
        temps: thread.temps,
        temp_xl: thread.temp_xl,
    })
}
pub fn refresh<H: VmHost + ?Sized>(
    thread: &VmThread,
    host: &mut H,
    bytes: [u8; 8],
) -> Result<PrimitiveExit, VmFault> {
    let target = match word(&bytes, 0) {
        0 => Some(thread.top()?.context.caller),
        1 => thread.top()?.context.stack_object_ref,
        _ => None,
    };
    let kind = match word(&bytes, 2) {
        0 => RefreshKind::Graphic,
        1 => RefreshKind::Light,
        2 => RefreshKind::RoomScore,
        _ => return Ok(PrimitiveExit::GotoTrue),
    };
    if kind == RefreshKind::Graphic {
        if let Some(target) = target {
            if !host.entity_info(target)?.is_avatar {
                host.presentation(PresentationRequest::Refresh { target, kind })?;
            }
        }
    } else {
        let target = target.ok_or(VmFault::MissingEntity(ObjectId(0)))?;
        host.presentation(PresentationRequest::Refresh { target, kind })?;
    }
    Ok(PrimitiveExit::GotoTrue)
}
pub fn show_string<H: VmHost + ?Sized>(
    thread: &VmThread,
    host: &mut H,
    bytes: [u8; 8],
) -> Result<PrimitiveExit, VmFault> {
    if let Some(target) = thread.top()?.context.stack_object_ref {
        if !host.entity_info(target)?.is_avatar {
            return Ok(PrimitiveExit::GotoTrue);
        }
    }
    if let Some(message) = host.resolve_dialog_string(string_lookup(
        thread,
        StringSource::CodeOwner,
        uword(&bytes, 0),
        uword(&bytes, 2) as i32 - 1,
    )?)? {
        let target = stack_entity(thread, host)?;
        host.presentation(PresentationRequest::ShowString {
            target,
            message: bounded_string(message)?,
            history: bytes[4] & 1 == 0,
        })?;
    }
    Ok(PrimitiveExit::GotoTrue)
}
pub fn balloon<H: VmHost + ?Sized>(
    thread: &VmThread,
    host: &mut H,
    bytes: [u8; 8],
) -> Result<PrimitiveExit, VmFault> {
    let flags2 = uword(&bytes, 0);
    let raw_index = bytes[2] as i8;
    let group = bytes[3];
    let duration = word(&bytes, 4);
    let flags = bytes[7];
    let target = if flags2 & 1 != 0 {
        stack_entity(thread, host)?
    } else {
        thread.top()?.context.caller
    };
    let clear = raw_index == -1 || duration == 0;
    let (icon, index) = if clear {
        (None, raw_index)
    } else if group == 7 {
        let icon = if raw_index < 2 {
            thread.top()?.context.stack_object_ref
        } else {
            let frame = thread.top()?;
            let id =
                frame.locals[index("balloon local", (flags2 >> 1) as i32, frame.locals.len())?];
            host.resolve_entity(ObjectId(id))
        };
        (icon, raw_index)
    } else {
        (
            None,
            if flags & 16 != 0 {
                (raw_index as i32 + thread.temps[0] as i32) as i8
            } else {
                raw_index
            },
        )
    };
    host.presentation(PresentationRequest::Balloon {
        target,
        icon,
        index,
        group,
        duration,
        headline_type: bytes[6],
        flags,
        clear,
        preserve_money_on_zero_duration: duration == 0,
    })?;
    Ok(PrimitiveExit::GotoTrue)
}
pub fn change_action_string<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
    bytes: [u8; 8],
) -> Result<PrimitiveExit, VmFault> {
    let source = match uword(&bytes, 2) {
        0 => StringSource::CodeOwner,
        1 => StringSource::CalleeSemiGlobal,
        2 => StringSource::Global,
        _ => return Ok(PrimitiveExit::GotoTrue),
    };
    if let Some(name) = host.resolve_dialog_string(string_lookup(
        thread,
        source,
        uword(&bytes, 0),
        bytes[4] as i32 - 1,
    )?)? {
        let name = bounded_string(name)?;
        if thread.is_check && thread.action_strings.is_some() {
            let parameter0 = thread.top()?.context.stack_object.0;
            let strings = thread.action_strings.as_mut().expect("checked");
            if strings.len() >= 1024 {
                return Err(VmFault::InvalidContent("Action string count limit".into()));
            }
            strings.push(ActionString { name, parameter0 });
        } else {
            host.presentation(PresentationRequest::ActionName {
                caller: thread.top()?.context.caller,
                name,
            })?;
        }
    }
    Ok(PrimitiveExit::GotoTrue)
}
pub fn change_suit<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
    bytes: [u8; 8],
) -> Result<PrimitiveExit, VmFault> {
    if !host.entity_info(thread.top()?.context.caller)?.is_avatar {
        return Err(VmFault::InvalidOperand {
            opcode: 6,
            detail: "ChangeSuit caller must be an avatar".into(),
        });
    }
    let original = bytes[0];
    let scope = bytes[1];
    let flags = uword(&bytes, 2);
    let default_update = scope == 2 && flags & 4 != 0;
    let data = if !default_update && flags & 2 != 0 {
        thread.temps[index("suit temp", original as i32, 20)?] as u8
    } else {
        original
    };
    let suit = host.resolve_suit(SuitLookup {
        context: thread.top()?.context.clone(),
        scope,
        resolved_data: data,
        original_data: original,
        default_update,
        update_index: thread.temps[0],
    })?;
    let caller = thread.top()?.context.caller;
    if default_update {
        let outfit = match suit {
            Some(ResolvedSuit::Reference(outfit)) => outfit,
            Some(ResolvedSuit::Id(id)) => OutfitReference::Id(id),
            _ => {
                return Err(VmFault::InvalidContent(
                    "Default suit update needs a parsed object-table outfit".into(),
                ))
            }
        };
        host.apply_appearance(AppearanceOperation::DefaultDaywear {
            target: caller,
            outfit,
        })?;
        return Ok(PrimitiveExit::GotoTrue);
    }
    let Some(suit) = suit else {
        return Ok(PrimitiveExit::GotoTrue);
    };
    let body = scope == 1 && [0, 1, 2, 3, 5, 6, 7, 20, 22, 23, 24, 25, 128].contains(&original);
    match suit {
        ResolvedSuit::Accessory(appearance) => {
            host.apply_appearance(AppearanceOperation::AccessoryName {
                target: caller,
                appearance: bounded_string(appearance)?,
                remove: flags & 1 != 0,
            })?
        }
        ResolvedSuit::Reference(outfit) => {
            write_variable(
                thread,
                host,
                Variable::new(Scope::MyPersonData, 8),
                original as i16,
            )?;
            host.apply_appearance(AppearanceOperation::Body {
                target: caller,
                outfit,
                current_outfit: original as i16,
            })?;
        }
        ResolvedSuit::Id(id) if body => {
            write_variable(
                thread,
                host,
                Variable::new(Scope::MyPersonData, 8),
                original as i16,
            )?;
            host.apply_appearance(AppearanceOperation::Body {
                target: caller,
                outfit: OutfitReference::Id(id),
                current_outfit: original as i16,
            })?;
        }
        ResolvedSuit::Id(id) => {
            // Renderer decoration toggling is represented explicitly, never as a change to stored suits.
            if flags & 1 != 0 || [8, 9, 10, 11].contains(&original) {
                host.apply_appearance(AppearanceOperation::Decoration {
                    target: caller,
                    slot: original,
                    outfit_id: id,
                    remove: flags & 1 != 0,
                })?;
            }
        }
    }
    Ok(PrimitiveExit::GotoTrue)
}
