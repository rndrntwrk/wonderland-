use super::uword;
use crate::ids::{EntityRef, ObjectId};
use crate::numeric;
use crate::vm::*;

pub const FUNCTION_ENTRY_POINTS: [u8; 15] =
    [18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 14, 28, 16, 17, 29];
pub const FUNCTION_SCORE_VARIABLES: [i16; 15] =
    [31, 32, 33, 36, -1, -1, 37, 38, -1, -1, 61, 39, 64, 65, 15];
fn object_value<H: VmHost + ?Sized>(
    host: &H,
    entity: EntityRef,
    index: u16,
) -> Result<i16, VmFault> {
    host.read_memory(&MemoryAddress::Entity {
        entity,
        field: EntityField::ObjectData,
        index,
    })
}
fn check<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
    context: FrameContext,
    routine: BoundRoutine,
) -> Result<bool, VmFault> {
    let result = host.evaluate_check(RoutineCheck {
        routine,
        context,
        args: vec![0; 4],
        temps: thread.temps,
        temp_xl: thread.temp_xl,
        run_in_owner: None,
        use_current_thread: false,
    })?;
    if result.copy_back {
        thread.temps = result.temps;
        thread.temp_xl = result.temp_xl;
    }
    Ok(result.exit == PrimitiveExit::ReturnTrue)
}
fn object_context(
    thread: &VmThread,
    entity: EntityRef,
    code_owner: u32,
) -> Result<FrameContext, VmFault> {
    Ok(FrameContext {
        caller: thread.top()?.context.caller,
        callee: entity,
        stack_object: entity.object_id,
        stack_object_ref: Some(entity),
        code_owner,
    })
}

pub fn run_functional<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
    bytes: [u8; 8],
) -> Result<PrimitiveOutcome, VmFault> {
    let function = uword(&bytes, 0);
    if function == 65535 {
        return Ok(PrimitiveOutcome::Exit(PrimitiveExit::GotoFalse));
    }
    let entry = FUNCTION_ENTRY_POINTS[index("functional entry", function as i32, 15)?];
    if thread.top()?.context.stack_object_ref.is_none() {
        return Ok(PrimitiveOutcome::Exit(PrimitiveExit::GotoFalse));
    }
    let entity = stack_entity(thread, host)?;
    if host.entity_info(entity)?.dead {
        return Ok(PrimitiveOutcome::Exit(PrimitiveExit::GotoFalse));
    }
    let behavior = host.behavior_entry(entity, entry)?;
    if !behavior.action_declared {
        return Ok(PrimitiveOutcome::Exit(PrimitiveExit::GotoFalse));
    }
    if let Some(condition) = behavior.condition {
        let context = object_context(thread, entity, condition.code_owner)?;
        if !check(thread, host, context, condition)? {
            return Ok(PrimitiveOutcome::Exit(PrimitiveExit::GotoFalse));
        }
    }
    let Some(action) = behavior.action else {
        return Ok(PrimitiveOutcome::Exit(PrimitiveExit::GotoFalse));
    };
    action.validate()?;
    if bytes[2] > 0 && thread.top()?.action_tree {
        host.change_interaction_icon(thread.top()?.context.caller, entity)?;
    }
    let call = RoutineCall {
        routine: action.routine,
        context: object_context(thread, entity, action.code_owner)?,
        args: vec![0; action.arguments as usize],
        action_tree: thread.top()?.action_tree,
        special_result: SpecialResult::Normal,
    };
    Ok(PrimitiveOutcome::Call(call))
}

pub fn run_named<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
    bytes: [u8; 8],
) -> Result<PrimitiveOutcome, VmFault> {
    if thread.top()?.context.stack_object_ref.is_none() {
        return Ok(PrimitiveOutcome::Exit(PrimitiveExit::GotoFalse));
    }
    let frame = thread.top()?.clone();
    let Some(routine) = host.named_tree(NameLookup {
        context: frame.context.clone(),
        current_routine: frame.routine,
        string_table: uword(&bytes, 0),
        string_scope: bytes[2],
        string_index: bytes[4] as i32 - 1,
    })?
    else {
        return Ok(PrimitiveOutcome::Exit(PrimitiveExit::GotoFalse));
    };
    routine.validate()?;
    let mut args = vec![-1; 4usize.max(routine.arguments as usize)];
    args[..4].copy_from_slice(&thread.temps[..4]);
    if bytes[5] == 2 {
        // ExecuteSubRoutine changes the code owner but preserves caller/callee/stack object.
        let mut context = frame.context;
        context.code_owner = routine.code_owner;
        Ok(PrimitiveOutcome::Call(RoutineCall {
            routine: routine.routine,
            context,
            args,
            action_tree: frame.action_tree,
            special_result: SpecialResult::Normal,
        }))
    } else {
        // Synchronous RunInMyStack is an explicit provider call; destination1 uses the target's thread.
        let target = stack_entity(thread, host)?;
        let mut context = object_context(thread, target, routine.code_owner)?;
        if bytes[5] != 0 {
            context.caller = target;
        }
        let result = host.evaluate_check(RoutineCheck {
            routine,
            context,
            args,
            temps: thread.temps,
            temp_xl: thread.temp_xl,
            run_in_owner: Some(if bytes[5] == 0 { thread.owner } else { target }),
            use_current_thread: bytes[5] == 0,
        })?;
        if result.copy_back {
            thread.temps = result.temps;
            thread.temp_xl = result.temp_xl;
            if let Some((interrupt, schedule_idle_start)) = result.thread_control {
                thread.interrupt = interrupt;
                thread.schedule_idle_start = schedule_idle_start;
                thread.last_exit = result.exit;
            }
        }
        Ok(PrimitiveOutcome::Exit(if result.aborting {
            PrimitiveExit::Error
        } else {
            PrimitiveExit::branch(result.accepted)
        }))
    }
}

pub fn find_best<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
    bytes: [u8; 8],
) -> Result<PrimitiveExit, VmFault> {
    let function = index("function", uword(&bytes, 0) as i32, 15)?;
    let entry = FUNCTION_ENTRY_POINTS[function];
    let score_var = FUNCTION_SCORE_VARIABLES[function];
    let mut ids = host.entity_ids()?;
    ids.sort();
    ids.dedup();
    let mut best = None;
    let mut best_score = i32::MIN;
    for id in ids {
        let Some(entity) = host.resolve_entity(id) else {
            continue;
        };
        if object_value(host, entity, 25)? > 0 {
            continue;
        }
        let info = host.entity_info(entity)?;
        let status = host.function_status(entity)?;
        if (!info.is_avatar && status.disabled)
            || info.position
                == (VmPosition {
                    x: i16::MIN,
                    y: i16::MIN,
                    level: 1,
                })
            || (thread.mode == VmMode::Tso && score_var != 15 && status.broken)
        {
            continue;
        }
        let behavior = host.behavior_entry(entity, entry)?;
        if !behavior.action_declared {
            continue;
        }
        let mut score = if score_var != -1 {
            object_value(host, entity, score_var as u16)? as i32
        } else {
            0
        };
        if score_var != -1 {
            if thread.mode == VmMode::Ts1 || score_var != 15 {
                if score <= 0 {
                    continue;
                }
                let threshold = match score_var {
                    39 => 800,
                    15 => 600,
                    64 => 15,
                    _ => 0,
                };
                if score < threshold {
                    continue;
                }
            } else if info.is_avatar || !status.broken {
                continue;
            }
        }
        let allowed = if let Some(condition) = behavior.condition {
            check(
                thread,
                host,
                object_context(thread, entity, condition.code_owner)?,
                condition,
            )?
        } else if !behavior.condition_declared && [14, 18, 20, 25].contains(&entry) {
            host.read_memory(&MemoryAddress::Entity {
                entity,
                field: EntityField::Slot,
                index: 0,
            })? == 0
        } else {
            true
        };
        if !allowed || host.function_status(entity)?.in_use {
            continue;
        }
        let info = host.entity_info(entity)?;
        let caller = host.entity_info(thread.top()?.context.caller)?;
        // LotTilePos subtraction narrows each component before the C# i32 distance expression.
        let x = info.position.x.wrapping_sub(caller.position.x) as i32;
        let y = info.position.y.wrapping_sub(caller.position.y) as i32;
        let level = info.position.level.wrapping_sub(caller.position.level) as i32;
        let square = x
            .wrapping_mul(x)
            .wrapping_add(y.wrapping_mul(y))
            .wrapping_add(
                level
                    .wrapping_mul(level)
                    .wrapping_mul(900)
                    .wrapping_mul(256),
            );
        let distance = numeric::legacy_f64_to_i32((square as f64).sqrt());
        score = score.wrapping_sub(distance / 3);
        if score > best_score {
            best_score = score;
            best = Some(entity);
        }
    }
    if let Some(entity) = best {
        let frame = thread.top_mut()?;
        frame.context.stack_object = entity.object_id;
        frame.context.stack_object_ref = Some(entity);
        Ok(PrimitiveExit::GotoTrue)
    } else {
        Ok(PrimitiveExit::GotoFalse)
    }
}

pub fn test_interacting<H: VmHost + ?Sized>(
    thread: &VmThread,
    host: &H,
) -> Result<PrimitiveExit, VmFault> {
    let state = host.interaction_state(thread.top()?.context.caller)?;
    Ok(PrimitiveExit::branch(
        state.action_tree && state.callee == thread.top()?.context.stack_object_ref,
    ))
}

pub fn push_interaction<H: VmHost + ?Sized>(
    thread: &VmThread,
    host: &mut H,
    bytes: [u8; 8],
) -> Result<PrimitiveExit, VmFault> {
    let frame = thread.top()?;
    let flags = bytes[3];
    let source_id = if flags & 2 != 0 {
        frame.locals[index(
            "interaction object local",
            bytes[1] as i32,
            frame.locals.len(),
        )?]
    } else {
        frame.args[index(
            "interaction object parameter",
            bytes[1] as i32,
            frame.args.len(),
        )?]
    };
    let source = host
        .resolve_entity(ObjectId(source_id))
        .ok_or(VmFault::MissingEntity(ObjectId(source_id)))?;
    let (priority, mode) = match bytes[2] {
        0 => {
            let old = if frame.action_tree && host.entity_info(frame.context.caller)?.is_avatar {
                host.read_memory(&MemoryAddress::Entity {
                    entity: frame.context.caller,
                    field: EntityField::PersonData,
                    index: 33,
                })?
            } else {
                1
            };
            (old.max(1), QueueMode::Normal)
        }
        1 => (100, QueueMode::Normal),
        2 => (2, QueueMode::Normal),
        3 => (50, QueueMode::Normal),
        4 => (40, QueueMode::ParentIdle),
        5 => (30, QueueMode::ParentExit),
        6 => (0, QueueMode::Idle),
        _ => (0, QueueMode::Normal),
    };
    let target = stack_entity(thread, host)?;
    if !host.interaction_available(source, target, bytes[0])? {
        return Ok(PrimitiveExit::GotoFalse);
    }
    let icon = if flags & 1 != 0 {
        let id = frame.locals[index(
            "interaction icon local",
            bytes[4] as i32,
            frame.locals.len(),
        )?];
        host.resolve_entity(ObjectId(id))
    } else {
        None
    };
    let other_avatars = target != frame.context.caller
        && host.entity_info(target)?.is_avatar
        && host.entity_info(frame.context.caller)?.is_avatar;
    Ok(PrimitiveExit::branch(host.push_interaction(
        PushInteractionRequest {
            context: frame.context.clone(),
            source,
            target,
            interaction: bytes[0],
            priority,
            mode,
            custom_icon: flags & 1 != 0,
            icon,
            push_head: flags & 4 != 0,
            push_tail: flags & 128 != 0,
            skip_permissions: true,
            immediate_result_chooser: other_avatars,
        },
    )?))
}

pub fn find_best_action<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
) -> Result<PrimitiveExit, VmFault> {
    let caller = thread.top()?.context.caller;
    let context = host.autonomy_context(caller)?;
    context
        .person_data
        .validate()
        .map_err(|error| VmFault::InvalidContent(format!("Autonomy person data: {error:?}")))?;
    if context.queued.len() > 65_536 {
        return Err(VmFault::InvalidContent(
            "Autonomy queue projection limit".into(),
        ));
    }
    if let Some(queued) = context
        .queued
        .iter()
        .find(|queued| queued.priority > context.person_data.values[33])
    {
        host.entity_info(queued.target)?;
        let frame = thread.top_mut()?;
        frame.context.stack_object = queued.target.object_id;
        frame.context.stack_object_ref = Some(queued.target);
        return Ok(PrimitiveExit::GotoTrue);
    }
    let offers = host.autonomy_offers(AutonomyOfferRequest {
        context: thread.top()?.context.clone(),
        temps: thread.temps,
        temp_xl: thread.temp_xl,
    })?;
    if let Some(registers) = offers.registers {
        thread.temps = registers.temps;
        thread.temp_xl = registers.temp_xl;
    }
    use crate::avatars::autonomy::{select_with_random, AutonomySelection};
    let selection = select_with_random(&context, &offers.candidates, &offers.tuning, |bound| {
        host.next_random(bound)
    })
    .map_err(|error| VmFault::InvalidContent(format!("Autonomy scoring: {error:?}")))?;
    let target = match selection {
        AutonomySelection::AlreadyQueued(target) => target,
        AutonomySelection::NoValidTarget => return Ok(PrimitiveExit::GotoFalse),
        AutonomySelection::Selected(action) => {
            host.entity_info(action.target)?;
            if !host.enqueue_autonomy(AutonomyEnqueue {
                caller,
                source: action.target,
                interaction: action.interaction_id,
                args: [action.param0, 0, 0, 0],
                priority: 2,
                mode: QueueMode::Normal,
            })? {
                return Ok(PrimitiveExit::GotoFalse);
            }
            action.target
        }
    };
    let frame = thread.top_mut()?;
    frame.context.stack_object = target.object_id;
    frame.context.stack_object_ref = Some(target);
    Ok(PrimitiveExit::GotoTrue)
}

pub fn gosub_found_action<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
) -> Result<PrimitiveOutcome, VmFault> {
    if let Some(target) = host.interaction_state(thread.owner)?.callee {
        host.entity_info(target)?;
        let frame = thread.top_mut()?;
        frame.context.stack_object = target.object_id;
        frame.context.stack_object_ref = Some(target);
    }
    if !thread.top()?.action_tree {
        if let Some(call) = host.attempt_push(thread.owner, &thread.top()?.context)? {
            return Ok(PrimitiveOutcome::Call(call));
        }
    }
    Ok(PrimitiveOutcome::Exit(PrimitiveExit::GotoFalse))
}
