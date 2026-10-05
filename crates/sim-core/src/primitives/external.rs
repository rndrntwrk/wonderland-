use super::{uword, word};
use crate::vm::*;

pub fn motive_change<H: VmHost + ?Sized>(
    thread: &VmThread,
    host: &H,
    bytes: [u8; 8],
) -> Result<PrimitiveOutcome, VmFault> {
    let clear_all = bytes[3] & 1 != 0;
    let (raw_rate, raw_max) = if clear_all {
        (0, 0)
    } else {
        let rate = read_variable(
            thread,
            host,
            Variable {
                scope: bytes[0] as u16,
                data: word(&bytes, 4),
            },
        )?;
        let maximum = read_variable(
            thread,
            host,
            Variable {
                scope: bytes[1] as u16,
                data: word(&bytes, 6),
            },
        )?;
        (rate, maximum)
    };
    Ok(PrimitiveOutcome::Request(HostRequest::MotiveChange(
        MotiveChangeRequest {
            context: thread.top()?.context.clone(),
            motive: bytes[2],
            raw_rate,
            raw_max,
            clear_all,
            once: bytes[3] & 2 != 0,
        },
    )))
}

pub fn animate(thread: &VmThread, bytes: [u8; 8]) -> Result<PrimitiveOutcome, VmFault> {
    let flags = bytes[5];
    let mut id = uword(&bytes, 0);
    let mut scope = bytes[4] as u32;
    if flags & 4 != 0 {
        let frame = thread.top()?;
        id = frame.args[index("animation parameter", id as i32, frame.args.len())?] as u16;
        if scope == 0 {
            scope = 65536;
        }
    }
    let mode = (flags & 1) | ((flags >> 3) & 2);
    let request = AnimationRequest {
        context: thread.top()?.context.clone(),
        animation_id: id,
        scope,
        mode,
        backwards: flags & 2 != 0,
        hurryable: flags & 64 != 0,
        expected_events: bytes[6],
        event_local: bytes[2] as u16,
        store_event_in_parameter: flags & 32 == 0,
    };
    Ok(PrimitiveOutcome::Request(HostRequest::Animation(request)))
}

pub fn route<H: VmHost + ?Sized>(
    thread: &VmThread,
    host: &H,
    opcode: u16,
    bytes: [u8; 8],
) -> Result<PrimitiveOutcome, VmFault> {
    if (opcode == 27 || opcode == 45) && thread.is_check {
        return Ok(PrimitiveOutcome::Exit(PrimitiveExit::GotoFalse));
    }
    if opcode == 22 && bytes[0] > 1 && thread.is_check {
        // LookTowards performs these dereferences before its check-tree guard.
        if !host.entity_info(thread.top()?.context.caller)?.is_avatar {
            return Err(VmFault::InvalidOperand {
                opcode,
                detail: "LookTowards caller must be an avatar".into(),
            });
        }
        if (2..=5).contains(&bytes[0]) {
            let target = host.entity_info(stack_entity(thread, host)?)?;
            if bytes[0] >= 4 {
                if target.group.is_empty() {
                    return Err(VmFault::Arithmetic(
                        "Average position divides by empty group".into(),
                    ));
                }
                for id in target.group {
                    host.entity_info(host.resolve_entity(id).ok_or(VmFault::MissingEntity(id))?)?;
                }
            }
        }
        return Ok(PrimitiveOutcome::Exit(PrimitiveExit::GotoFalse));
    }
    if opcode == 27 && thread.top()?.context.stack_object_ref.is_none() {
        return Ok(PrimitiveOutcome::Exit(PrimitiveExit::GotoFalse));
    }
    if opcode == 27 {
        let target = host.entity_info(stack_entity(thread, host)?)?;
        if target.position
            == (VmPosition {
                x: i16::MIN,
                y: i16::MIN,
                level: 1,
            })
        {
            return Ok(PrimitiveOutcome::Exit(PrimitiveExit::GotoFalse));
        }
    }
    if opcode == 45 && uword(&bytes, 2) == 0 {
        let frame = thread.top()?;
        index(
            "routing slot parameter",
            uword(&bytes, 0) as i32,
            frame.args.len(),
        )?;
    }
    if opcode == 45 {
        let scope = uword(&bytes, 2);
        if scope > 2 {
            return Ok(PrimitiveOutcome::Exit(PrimitiveExit::GotoFalse));
        }
        if thread.top()?.context.stack_object_ref.is_none() {
            if scope == 2 {
                return Ok(PrimitiveOutcome::Exit(PrimitiveExit::GotoFalse));
            }
            return Err(VmFault::MissingEntity(thread.top()?.context.stack_object));
        }
    }
    if opcode == 47 && uword(&bytes, 0) > 1 {
        return Err(VmFault::UnsupportedPrimitive {
            opcode,
            case: "Source throws for reach-to-mouth and modes above 1".into(),
        });
    }
    let kind = match opcode {
        16 => RouteKind::FindLocation,
        22 => RouteKind::LookTowards,
        27 => RouteKind::RelativePosition,
        45 => RouteKind::RoutingSlot,
        46 => RouteKind::Snap,
        47 => RouteKind::Reach,
        _ => {
            return Err(VmFault::InvalidOperand {
                opcode,
                detail: "Not a routing primitive".into(),
            })
        }
    };
    let frame = thread.top()?;
    Ok(PrimitiveOutcome::Request(HostRequest::Route(
        RouteRequest {
            kind,
            context: frame.context.clone(),
            operand: bytes,
            parameters: frame.args.clone(),
            locals: frame.locals.clone(),
            temps: thread.temps,
        },
    )))
}

pub fn external<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
    opcode: u16,
    bytes: [u8; 8],
) -> Result<PrimitiveOutcome, VmFault> {
    let mut amount = None;
    if opcode == 25 && thread.mode == VmMode::Ts1 {
        return Err(VmFault::UnsupportedPrimitive {
            opcode,
            case: "TS1 family budget authority is unported".into(),
        });
    }
    if opcode == 25 {
        // Force the exact short/XL scoped read before dispatch. A provider resolves accounts/tuning.
        let scope = match bytes[0] {
            0 => 7,
            1 => 9,
            2 => 25,
            _ => bytes[1] as u16,
        };
        amount = Some(read_big_variable(
            thread,
            host,
            Variable {
                scope,
                data: word(&bytes, 2),
            },
        )?);
        // Source reads the amount before either its mode switch or check-tree guard.
        if ![0, 2, 5, 6, 7, 9, 10, 11, 12, 13, 17, 18, 19, 20, 24].contains(&bytes[7]) {
            return Ok(PrimitiveOutcome::Exit(PrimitiveExit::GotoTrue));
        }
        if bytes[7] == 17 {
            amount = Some(0);
        }
        if thread.is_check && bytes[7] != 17 && bytes[4] & 1 == 0 {
            return Ok(PrimitiveOutcome::Exit(PrimitiveExit::GotoFalse));
        }
    }
    let kind = match opcode {
        1 if thread.mode == VmMode::Ts1 => ExternalKind::GenericTs1,
        1 => ExternalKind::GenericTso,
        23 | 48 => ExternalKind::Sound,
        25 => ExternalKind::TransferFunds,
        35 => ExternalKind::SpecialEffect,
        36 | 38 | 39 => ExternalKind::Dialog,
        40 => ExternalKind::OnlineJobs,
        62 => ExternalKind::InvokePlugin,
        67 => ExternalKind::Inventory,
        _ => ExternalKind::Other,
    };
    let frame = thread.top()?;
    Ok(PrimitiveOutcome::Request(HostRequest::External(
        ExternalRequest {
            opcode,
            kind,
            context: frame.context.clone(),
            operand: bytes,
            parameters: frame.args.clone(),
            temps: thread.temps,
            temp_xl: thread.temp_xl,
            is_check: thread.is_check,
            amount,
        },
    )))
}
