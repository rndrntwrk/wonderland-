use super::*;
use crate::primitives;
use serde::{Deserialize, Serialize};

pub const MAX_STACK_DEPTH: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrimitiveOutcome {
    Exit(PrimitiveExit),
    SleepUntil(u32),
    Request(HostRequest),
    Call(RoutineCall),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunReport {
    pub instructions: u32,
    pub stop: VmStop,
}

impl VmThread {
    pub fn push_entry(
        &mut self,
        store: &RoutineStore,
        key: RoutineKey,
        context: FrameContext,
        args: Vec<i16>,
    ) -> Result<bool, VmFault> {
        context.validate()?;
        let routine = store
            .get(key)
            .ok_or_else(|| VmFault::InvalidContent("Entry routine missing".into()))?;
        if routine.instructions().is_empty() {
            self.diagnostic(VmDiagnostic::EmptyRoutine { routine: key });
            return Ok(false);
        }
        if self.frames.len() >= MAX_STACK_DEPTH {
            return Err(VmFault::StackLimit);
        }
        if args.len() > 255 {
            return Err(VmFault::InvalidContent("Entry argument limit".into()));
        }
        self.frames.push(VmFrame {
            routine: key,
            instruction_pointer: 0,
            context,
            locals: vec![0; (routine.locals().max(routine.arguments())) as usize],
            args,
            special_result: SpecialResult::Normal,
            action_tree: false,
        });
        self.stop = VmStop::Ready;
        Ok(true)
    }
    /// Validate imported/checkpoint state before it becomes executable.
    pub fn validate(&self, store: &RoutineStore) -> Result<(), VmFault> {
        if self.owner.object_id.0 <= 0 || self.owner.generation == 0 {
            return Err(VmFault::InvalidContent("Invalid thread owner".into()));
        }
        if let Some(strings) = &self.action_strings {
            if strings.len() > 1024 || strings.iter().any(|value| value.name.len() > 65536) {
                return Err(VmFault::InvalidContent(
                    "Action string collection bounds".into(),
                ));
            }
        }
        if self.frames.len() > MAX_STACK_DEPTH
            || self.diagnostics.len() > 128
            || self.tree_advertisements.len() > 196608
        {
            return Err(VmFault::InvalidContent(
                "Thread state exceeds bounds".into(),
            ));
        }
        for frame in &self.frames {
            frame.context.validate()?;
            let routine = store
                .get(frame.routine)
                .ok_or_else(|| VmFault::InvalidContent("Checkpoint routine missing".into()))?;
            routine.instruction(frame.instruction_pointer)?;
            if frame.locals.len() != routine.locals().max(routine.arguments()) as usize
                || frame.args.len() > 255
            {
                return Err(VmFault::InvalidContent(
                    "Checkpoint locals/arguments mismatch".into(),
                ));
            }
            if let Some(reference) = frame.context.stack_object_ref {
                if reference.object_id != frame.context.stack_object {
                    return Err(VmFault::InvalidContent(
                        "Stack reference/raw ID mismatch".into(),
                    ));
                }
            }
        }
        if let Some(continuation) = &self.continuation {
            if continuation.frame_depth != self.frames.len()
                || self.top()?.instruction_pointer != continuation.instruction_pointer
            {
                return Err(VmFault::InvalidContinuation(
                    "Continuation frame moved".into(),
                ));
            }
            let frame = self.top()?;
            let instruction = store
                .get(frame.routine)
                .ok_or_else(|| VmFault::InvalidContent("Continuation routine missing".into()))?
                .instruction(frame.instruction_pointer)?;
            continuation.request.validate_at(self, instruction)?;
            if continuation.request.context() != &self.top()?.context {
                return Err(VmFault::InvalidContinuation(
                    "Continuation entity context differs from frame".into(),
                ));
            }
            if let Some(resolution) = &continuation.resolution {
                self.validate_resolution(resolution, &continuation.request)?;
            }
            match self.stop {
                VmStop::Waiting { request_id }
                    if request_id == continuation.request_id
                        && continuation.resolution.is_none() => {}
                VmStop::Ready | VmStop::BudgetExhausted if continuation.resolution.is_some() => {}
                _ => {
                    return Err(VmFault::InvalidContinuation(
                        "Continuation status mismatch".into(),
                    ))
                }
            }
        } else if matches!(self.stop, VmStop::Waiting { .. }) {
            return Err(VmFault::InvalidContinuation("Missing continuation".into()));
        }
        if self.frames.is_empty()
            && matches!(self.stop, VmStop::Sleeping { .. } | VmStop::Waiting { .. })
        {
            return Err(VmFault::InvalidContent("Empty thread suspended".into()));
        }
        if !self.frames.is_empty() && matches!(self.stop, VmStop::Completed(_)) {
            return Err(VmFault::InvalidContent(
                "Completed thread has active frames".into(),
            ));
        }
        Ok(())
    }
    pub fn resume(&mut self, request_id: u64, resolution: VmResolution) -> Result<(), VmFault> {
        let continuation = self
            .continuation
            .as_ref()
            .ok_or_else(|| VmFault::InvalidContinuation("No pending request".into()))?;
        if continuation.request_id != request_id
            || continuation.resolution.is_some()
            || self.stop != (VmStop::Waiting { request_id })
        {
            return Err(VmFault::InvalidContinuation(
                "Stale or duplicate response".into(),
            ));
        }
        self.validate_resolution(&resolution, &continuation.request)?;
        self.continuation
            .as_mut()
            .expect("checked continuation")
            .resolution = Some(resolution);
        self.stop = VmStop::Ready;
        Ok(())
    }
    fn validate_register_writes(&self, writes: &[RegisterWrite]) -> Result<(), VmFault> {
        if writes.len() > 1024 {
            return Err(VmFault::InvalidContinuation(
                "Too many register writes".into(),
            ));
        }
        for write in writes {
            let (area, len) = match write.target {
                RegisterTarget::Temp => ("response temp", 20),
                RegisterTarget::TempXl => ("response XL temp", 2),
                RegisterTarget::Local => ("response local", self.top()?.locals.len()),
                RegisterTarget::Parameter => ("response parameter", self.top()?.args.len()),
                RegisterTarget::StackObjectId => ("response stack object ID", 1),
            };
            index(area, i32::from(write.index), len)?;
        }
        Ok(())
    }
    fn validate_resolution(
        &self,
        resolution: &VmResolution,
        request: &HostRequest,
    ) -> Result<(), VmFault> {
        if matches!(resolution.response, HostResponse::Pending { .. }) {
            return Err(VmFault::InvalidContinuation(
                "Pending response cannot resolve a request".into(),
            ));
        }
        self.validate_register_writes(&resolution.writes)?;
        if let HostResponse::CompleteWithWrites { writes, .. } = &resolution.response {
            if resolution.writes.len().saturating_add(writes.len()) > 1024 {
                return Err(VmFault::InvalidContinuation(
                    "Too many register writes".into(),
                ));
            }
            self.validate_register_writes(writes)?;
        }
        if matches!(resolution.response, HostResponse::AnimationEvent(_))
            && !matches!(request, HostRequest::Animation(_))
        {
            return Err(VmFault::InvalidContinuation(
                "Animation response for a non-animation request".into(),
            ));
        }
        Ok(())
    }
    pub fn wake(&mut self) {
        if matches!(self.stop, VmStop::Sleeping { .. } | VmStop::BudgetExhausted) {
            self.stop = VmStop::Ready;
        }
    }
    pub fn run<H: VmHost + ?Sized>(
        &mut self,
        store: &RoutineStore,
        host: &mut H,
        budget: u32,
    ) -> RunReport {
        // The scheduler decides when a sleeping thread is due (including source u32 tick wrap).
        if !matches!(self.stop, VmStop::Ready | VmStop::BudgetExhausted) {
            return RunReport {
                instructions: 0,
                stop: self.stop.clone(),
            };
        }
        self.stop = VmStop::Ready;
        let mut instructions = 0;
        while instructions < budget && self.stop == VmStop::Ready {
            if self.frames.is_empty() {
                self.stop = VmStop::Completed(self.last_exit);
                break;
            }
            match step_instruction(self, store, host) {
                Ok(()) => {}
                Err(fault) => {
                    self.stop = VmStop::Faulted(fault);
                }
            }
            instructions += 1;
            if let Err(fault) = self.apply_host_register_writes(
                host.take_temp_writes(self.owner),
                host.take_temp_xl_writes(self.owner),
            ) {
                self.continuation = None;
                self.stop = VmStop::Faulted(fault);
            }
            if host.take_interrupt_request(self.owner) {
                self.interrupt = true;
            }
            if host.take_reset_request(self.owner) {
                self.frames.clear();
                self.continuation = None;
                self.last_exit = PrimitiveExit::Interrupt;
                self.stop = VmStop::Completed(PrimitiveExit::Interrupt);
            }
        }
        if self.stop == VmStop::Ready && instructions == budget {
            self.stop = VmStop::BudgetExhausted;
        }
        RunReport {
            instructions,
            stop: self.stop.clone(),
        }
    }
    fn apply_host_register_writes(
        &mut self,
        writes: Vec<(u16, i16)>,
        xl_writes: Vec<(u16, i32)>,
    ) -> Result<(), VmFault> {
        if writes.len() > self.temps.len() || xl_writes.len() > self.temp_xl.len() {
            return Err(VmFault::InvalidContent(
                "Host register write batch exceeds register count".into(),
            ));
        }
        for (target, _) in &writes {
            index("host temp write", i32::from(*target), self.temps.len())?;
        }
        for (target, _) in &xl_writes {
            index("host XL write", i32::from(*target), self.temp_xl.len())?;
        }
        for (target, value) in writes {
            self.temps[usize::from(target)] = value;
        }
        for (target, value) in xl_writes {
            self.temp_xl[usize::from(target)] = value;
        }
        Ok(())
    }
    fn apply_host_thread_control<H: VmHost + ?Sized>(&mut self, host: &mut H) {
        if let Some((interrupt, schedule_idle_start, last_exit)) =
            host.take_thread_control(self.owner)
        {
            self.interrupt = interrupt;
            self.schedule_idle_start = schedule_idle_start;
            self.last_exit = last_exit;
        }
    }
    fn call(
        &mut self,
        store: &RoutineStore,
        opcode: u16,
        operand: [u8; 8],
        tick: u32,
    ) -> Result<(), VmFault> {
        let parent = self.top()?.clone();
        let Some(key) = store.resolve(parent.context.code_owner, opcode) else {
            self.diagnostic(VmDiagnostic::MissingRoutine {
                code_owner: parent.context.code_owner,
                id: opcode,
            });
            // ExecuteSubRoutine(null) calls Pop(ERROR): it discards the caller and propagates error.
            return self.pop_frame(store, PrimitiveExit::Error, tick);
        };
        let routine = store.get(key).expect("resolved routine exists");
        let operand = SubroutineOperand::decode(operand);
        let count = (routine.arguments() as usize).max(4);
        let mut args = Vec::with_capacity(count);
        for i in 0..count {
            let mut value = if i < 4 { operand.arguments[i] } else { -1 };
            if value == -1 && operand.use_temps {
                value = self.temps[index("subroutine temp argument", i as i32, 20)?];
            }
            args.push(value);
        }
        let pushed = self.push_entry(store, key, parent.context, args)?;
        if pushed {
            self.top_mut()?.action_tree = parent.action_tree;
        }
        // Empty BHAV: source Push returns false and leaves caller on its call instruction.
        Ok(())
    }
    fn branch(&mut self, store: &RoutineStore, pointer: u8, tick: u32) -> Result<(), VmFault> {
        match pointer {
            BRANCH_TRUE => self.pop_frame(store, PrimitiveExit::ReturnTrue, tick),
            BRANCH_FALSE => self.pop_frame(store, PrimitiveExit::ReturnFalse, tick),
            BRANCH_ERROR => {
                let frame = self.top()?;
                let instruction = store
                    .get(frame.routine)
                    .ok_or_else(|| VmFault::InvalidContent("Routine missing".into()))?
                    .instruction(frame.instruction_pointer)?;
                let alternate = if instruction.true_pointer != BRANCH_ERROR {
                    instruction.true_pointer
                } else {
                    instruction.false_pointer
                };
                if alternate == BRANCH_ERROR {
                    self.pop_frame(store, PrimitiveExit::Error, tick)
                } else {
                    self.branch(store, alternate, tick)
                }
            }
            _ => {
                let frame = self.top_mut()?;
                store
                    .get(frame.routine)
                    .ok_or_else(|| VmFault::InvalidContent("Routine missing".into()))?
                    .instruction(pointer)?;
                frame.instruction_pointer = pointer;
                Ok(())
            }
        }
    }
    fn pop_frame(
        &mut self,
        store: &RoutineStore,
        mut exit: PrimitiveExit,
        tick: u32,
    ) -> Result<(), VmFault> {
        let popped = self
            .frames
            .pop()
            .ok_or_else(|| VmFault::InvalidContent("Pop on empty stack".into()))?;
        self.last_exit = exit;
        match popped.special_result {
            SpecialResult::Normal => {}
            SpecialResult::Retry => exit = PrimitiveExit::Continue,
            SpecialResult::Interaction { run_immediately } => {
                self.diagnostic(VmDiagnostic::InteractionReturned { run_immediately });
                self.interaction_returns.push((exit, run_immediately));
                exit = if run_immediately {
                    PrimitiveExit::Continue
                } else {
                    PrimitiveExit::ContinueNextTick
                };
            }
        }
        if self.frames.is_empty() {
            self.stop = VmStop::Completed(self.last_exit);
            return Ok(());
        }
        exit = match exit {
            PrimitiveExit::ReturnTrue => PrimitiveExit::GotoTrue,
            PrimitiveExit::ReturnFalse => PrimitiveExit::GotoFalse,
            value => value,
        };
        self.apply_exit(store, exit, tick)
    }
    fn apply_exit(
        &mut self,
        store: &RoutineStore,
        exit: PrimitiveExit,
        tick: u32,
    ) -> Result<(), VmFault> {
        match exit {
            PrimitiveExit::GotoTrue
            | PrimitiveExit::GotoFalse
            | PrimitiveExit::GotoTrueNextTick
            | PrimitiveExit::GotoFalseNextTick => {
                let frame = self.top()?;
                let instruction = store
                    .get(frame.routine)
                    .ok_or_else(|| VmFault::InvalidContent("Routine missing".into()))?
                    .instruction(frame.instruction_pointer)?;
                let pointer = if matches!(
                    exit,
                    PrimitiveExit::GotoTrue | PrimitiveExit::GotoTrueNextTick
                ) {
                    instruction.true_pointer
                } else {
                    instruction.false_pointer
                };
                self.branch(store, pointer, tick)?;
                if matches!(
                    exit,
                    PrimitiveExit::GotoTrueNextTick | PrimitiveExit::GotoFalseNextTick
                ) && self.stop == VmStop::Ready
                {
                    self.schedule_idle_start = tick;
                    self.stop = VmStop::Sleeping {
                        until_tick: tick.wrapping_add(1),
                    };
                }
            }
            PrimitiveExit::ReturnTrue | PrimitiveExit::ReturnFalse | PrimitiveExit::Error => {
                self.pop_frame(store, exit, tick)?
            }
            PrimitiveExit::ContinueNextTick => {
                self.schedule_idle_start = tick;
                self.stop = VmStop::Sleeping {
                    until_tick: tick.wrapping_add(1),
                };
            }
            PrimitiveExit::Continue => {}
            PrimitiveExit::Interrupt => {
                self.frames.clear();
                self.last_exit = exit;
                self.stop = VmStop::Completed(exit);
            }
            PrimitiveExit::ContinueFutureTick => {
                return Err(VmFault::InvalidContinuation(
                    "Future tick requires explicit wake deadline".into(),
                ))
            }
        }
        Ok(())
    }
    fn write_register<H: VmHost + ?Sized>(
        &mut self,
        host: &mut H,
        write: RegisterWrite,
    ) -> Result<(), VmFault> {
        let scope = match write.target {
            RegisterTarget::Temp => 8,
            RegisterTarget::TempXl => 42,
            RegisterTarget::Local => 25,
            RegisterTarget::Parameter => 9,
            RegisterTarget::StackObjectId => 10,
        };
        write_big_variable(
            self,
            host,
            Variable {
                scope,
                data: write.index as i16,
            },
            write.value,
        )?;
        Ok(())
    }
    fn handle_response<H: VmHost + ?Sized>(
        &mut self,
        store: &RoutineStore,
        host: &mut H,
        request: HostRequest,
        response: HostResponse,
    ) -> Result<(), VmFault> {
        match response {
            HostResponse::Complete(exit) => self.apply_exit(store, exit, host.current_tick()),
            HostResponse::CompleteWithWrites { exit, writes } => {
                self.validate_register_writes(&writes)?;
                let mut staged = self.clone();
                for write in writes {
                    staged.write_register(host, write)?;
                }
                *self = staged;
                self.apply_exit(store, exit, host.current_tick())
            }
            HostResponse::NextTick => {
                self.apply_exit(store, PrimitiveExit::ContinueNextTick, host.current_tick())
            }
            HostResponse::AnimationEvent(value) => {
                let HostRequest::Animation(animation) = request else {
                    return Err(VmFault::InvalidContinuation(
                        "Animation event for non-animation request".into(),
                    ));
                };
                let target = if animation.store_event_in_parameter {
                    Variable::new(Scope::Parameters, 0)
                } else {
                    Variable::new(Scope::Local, animation.event_local as i16)
                };
                write_variable(self, host, target, value)?;
                self.apply_exit(store, PrimitiveExit::GotoFalse, host.current_tick())
            }
            HostResponse::Pending { request_id } => {
                let instruction_pointer = self.top()?.instruction_pointer;
                self.continuation = Some(PrimitiveContinuation {
                    request_id,
                    request,
                    frame_depth: self.frames.len(),
                    instruction_pointer,
                    resolution: None,
                });
                self.stop = VmStop::Waiting { request_id };
                Ok(())
            }
        }
    }
}

/// Execute exactly one instruction (or consume its pending response). No hidden tick loop.
pub fn step_instruction<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    store: &RoutineStore,
    host: &mut H,
) -> Result<(), VmFault> {
    let result = step_instruction_inner(thread, store, host);
    let returns = std::mem::take(&mut thread.interaction_returns);
    if let Some(diagnostic) = thread.unsupported_diagnostic.take() {
        if host.requires_supported_behavior(thread.owner) {
            return Err(VmFault::HostUnsupported(format!(
                "source behavior cannot complete: {diagnostic:?}"
            )));
        }
    }
    result?;
    // Queue completion happens before the parent frame executes another
    // instruction, including source RunImmediately continuations.
    for (exit, run_immediately) in returns {
        host.observe_thread(thread);
        host.interaction_returned(thread.owner, exit, run_immediately)?;
    }
    Ok(())
}

fn step_instruction_inner<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    store: &RoutineStore,
    host: &mut H,
) -> Result<(), VmFault> {
    host.observe_thread(thread);
    if let Some(continuation) = thread.continuation.take() {
        if continuation.frame_depth != thread.frames.len()
            || continuation.instruction_pointer != thread.top()?.instruction_pointer
        {
            return Err(VmFault::InvalidContinuation(
                "Continuation frame moved".into(),
            ));
        }
        let frame = thread.top()?;
        let instruction = store
            .get(frame.routine)
            .ok_or_else(|| VmFault::InvalidContent("Continuation routine missing".into()))?
            .instruction(frame.instruction_pointer)?;
        continuation.request.validate_at(thread, instruction)?;
        let Some(mut resolution) = continuation.resolution else {
            thread.continuation = Some(continuation);
            return Ok(());
        };
        thread.validate_resolution(&resolution, &continuation.request)?;
        if let HostResponse::CompleteWithWrites { exit, writes } = &mut resolution.response {
            resolution.writes.append(writes);
            resolution.response = HostResponse::Complete(*exit);
        }
        // Apply response writes transactionally: malformed later indices cannot leave a half-write.
        let mut staged = thread.clone();
        for write in resolution.writes {
            staged.write_register(host, write)?;
        }
        *thread = staged;
        thread.apply_host_thread_control(host);
        return thread.handle_response(store, host, continuation.request, resolution.response);
    }
    let frame = thread.top()?;
    let instruction = store
        .get(frame.routine)
        .ok_or_else(|| VmFault::InvalidContent("Routine missing".into()))?
        .instruction(frame.instruction_pointer)?
        .clone();
    if instruction.opcode >= 256 {
        thread.apply_host_thread_control(host);
        return thread.call(
            store,
            instruction.opcode,
            instruction.operand,
            host.current_tick(),
        );
    }
    let outcome = primitives::execute(thread, host, &instruction);
    // A nested host call can update an actually removed ancestor. Preserve source ordering:
    // HandleResult may subsequently overwrite this child's exit or countdown origin.
    thread.apply_host_thread_control(host);
    match outcome? {
        PrimitiveOutcome::Call(call) => {
            call.validate()?;
            if thread.push_entry(store, call.routine, call.context, call.args)? {
                let frame = thread.top_mut()?;
                frame.action_tree = call.action_tree;
                frame.special_result = call.special_result;
            }
            Ok(())
        }
        PrimitiveOutcome::Exit(exit) => thread.apply_exit(store, exit, host.current_tick()),
        PrimitiveOutcome::SleepUntil(until_tick) => {
            thread.stop = VmStop::Sleeping { until_tick };
            Ok(())
        }
        PrimitiveOutcome::Request(request) => {
            request.validate_at(thread, &instruction)?;
            let response = host.request(request.clone());
            thread.apply_host_thread_control(host);
            thread.handle_response(store, host, request, response?)
        }
    }
}
