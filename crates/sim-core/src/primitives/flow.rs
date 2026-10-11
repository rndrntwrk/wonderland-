use super::uword;
use super::word;

pub fn idle_for_input<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
    bytes: [u8; 8],
) -> Result<PrimitiveOutcome, VmFault> {
    let param = word(&bytes, 0);
    let allow_push = uword(&bytes, 2) == 1;
    let tick = host.current_tick();
    let start = thread.schedule_idle_start;
    thread.schedule_idle_start = 0;
    let elapsed = if start != 0 && start < tick {
        tick - start
    } else {
        1
    };
    let frame = thread.top_mut()?;
    let at = index("idle parameter", param as i32, frame.args.len())?;
    frame.args[at] = frame.args[at].wrapping_sub(elapsed as i16);
    let remaining = frame.args[at];
    let context = frame.context.clone();
    let action_tree = frame.action_tree;
    let decision = host.idle_for_input(
        &context,
        allow_push && (thread.mode == VmMode::Ts1 || !action_tree),
        action_tree,
        thread.mode,
    )?;
    match decision {
        IdleDecision::Push(call) => return Ok(PrimitiveOutcome::Call(call)),
        IdleDecision::Notified => {
            let flags = read_variable(thread, host, Variable::new(Scope::MyObject, 8))?;
            write_variable(thread, host, Variable::new(Scope::MyObject, 8), flags | 64)?;
            return Ok(PrimitiveOutcome::Exit(PrimitiveExit::GotoTrue));
        }
        IdleDecision::Quiet => {}
    }
    if thread.interrupt {
        thread.interrupt = false;
        return Ok(PrimitiveOutcome::Exit(PrimitiveExit::GotoTrue));
    }
    if remaining < 0 {
        return Ok(PrimitiveOutcome::Exit(PrimitiveExit::GotoTrue));
    }
    if allow_push {
        Ok(PrimitiveOutcome::Exit(PrimitiveExit::ContinueNextTick))
    } else {
        thread.schedule_idle_start = tick;
        Ok(PrimitiveOutcome::SleepUntil(
            tick.wrapping_add(remaining as u32 + 1),
        ))
    }
}
use crate::vm::*;

/// Scheduler-aware source sleep. Sleep(0) still consumes NextRandom(1) at completion.
pub fn sleep<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
    bytes: [u8; 8],
) -> Result<PrimitiveOutcome, VmFault> {
    let param = word(&bytes, 0);
    let tick = host.current_tick();
    let start = thread.schedule_idle_start;
    let elapsed = if start != 0 && start < tick {
        tick - start
    } else {
        1
    };
    let frame = thread.top_mut()?;
    let at = index("sleep parameter", param as i32, frame.args.len())?;
    frame.args[at] = frame.args[at].wrapping_sub(elapsed as i16);
    let remaining = frame.args[at];
    if thread.interrupt {
        thread.schedule_idle_start = 0;
        thread.interrupt = false;
        return Ok(PrimitiveOutcome::Exit(PrimitiveExit::GotoTrue));
    }
    if remaining < 0 {
        thread.schedule_idle_start = 0;
        host.next_random(1);
        let dead = host.entity_info(thread.top()?.context.caller)?.dead;
        Ok(PrimitiveOutcome::Exit(if dead {
            PrimitiveExit::GotoTrueNextTick
        } else {
            PrimitiveExit::GotoTrue
        }))
    } else {
        thread.schedule_idle_start = tick;
        Ok(PrimitiveOutcome::SleepUntil(
            tick.wrapping_add(remaining as u32 + 1),
        ))
    }
}
