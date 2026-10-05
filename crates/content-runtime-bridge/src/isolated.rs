//! A replica-role copy of the real runtime. No external effect dispatcher exists here.
use sim_core::{
    ids::EntityRef,
    runtime::{AcceptedTick, QueryOutcome, RuntimeRole, SimRuntime, TickOutcome},
    snapshot::SnapshotExpectation,
    state::ContentSet,
    vm::{FrameContext, RoutineKey, VmStop},
};

#[derive(Clone, Debug)]
pub struct RoutineQuery {
    pub actor: EntityRef,
    pub target: EntityRef,
    pub code_owner: u32,
    pub routine_id: u16,
    pub args: Vec<i16>,
    pub instruction_budget: u32,
}

pub struct IsolatedRuntime {
    runtime: SimRuntime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum WatchField {
    Attribute(u16),
    ObjectData(u16),
    Temp(u16),
    TempXl(u16),
    Stop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct StateWatch {
    pub entity: EntityRef,
    pub field: WatchField,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub enum WatchValue {
    Scalar(i64),
    Stop(VmStop),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct FramePosition {
    pub entity: EntityRef,
    pub depth: usize,
    pub routine: RoutineKey,
    pub instruction: u8,
    pub opcode: Option<u16>,
}

impl IsolatedRuntime {
    pub fn capture(runtime: &SimRuntime) -> Result<Self, String> {
        SimRuntime::from_state(
            runtime.state().clone(),
            runtime.content().clone(),
            RuntimeRole::Replica,
        )
        .map(|runtime| Self { runtime })
        .map_err(|error| error.to_string())
    }
    pub fn from_snapshot(
        bytes: &[u8],
        content: ContentSet,
        expectation: SnapshotExpectation,
    ) -> Result<Self, String> {
        let state = sim_core::snapshot::decode(bytes, &content, expectation)
            .map_err(|error| format!("snapshot: {error:?}"))?;
        SimRuntime::from_state(state, content, RuntimeRole::Replica)
            .map(|runtime| Self { runtime })
            .map_err(|error| error.to_string())
    }
    pub fn runtime(&self) -> &SimRuntime {
        &self.runtime
    }
    pub fn query(&self, request: &RoutineQuery) -> Result<QueryOutcome, String> {
        // The interpreter checks entry arguments after cloning its state, and
        // empty routines return before that check. Bound the public request first.
        if request.args.len() > 255 {
            return Err("query argument limit: maximum 255".into());
        }
        if request.instruction_budget == 0
            || request.instruction_budget
                > self.runtime.state().limits.instruction_budget_per_entity
        {
            return Err("query instruction budget is outside the runtime limit".into());
        }
        for reference in [request.actor, request.target] {
            if !self.runtime.state().ids.is_live(reference) {
                return Err("query actor or target is not a live generation".into());
            }
        }
        let routine = self
            .runtime
            .content()
            .routines()
            .resolve(request.code_owner, request.routine_id)
            .ok_or_else(|| "query routine is absent from its required namespace".to_string())?;
        let context = FrameContext {
            caller: request.actor,
            callee: request.target,
            stack_object: request.target.object_id,
            stack_object_ref: Some(request.target),
            code_owner: request.code_owner,
        };
        self.runtime
            .query_behavior(
                request.actor,
                routine,
                context,
                request.args.clone(),
                request.instruction_budget,
            )
            .map_err(|error| error.to_string())
    }
    pub fn advance(&mut self, input: &AcceptedTick) -> Result<TickOutcome, String> {
        self.runtime.step(input).map_err(|error| error.to_string())
    }
    pub fn watch(&self, watch: StateWatch) -> Result<WatchValue, String> {
        if !self.runtime.state().ids.is_live(watch.entity) {
            return Err("watch entity is not a live generation".into());
        }
        let object = &self.runtime.state().entities[&watch.entity.object_id];
        let thread = &self.runtime.state().threads[&watch.entity.object_id];
        let value = match watch.field {
            WatchField::Attribute(index) => object
                .attributes
                .get(usize::from(index))
                .map(|v| i64::from(*v)),
            WatchField::ObjectData(index) => object
                .object_data
                .get(usize::from(index))
                .map(|v| i64::from(*v)),
            WatchField::Temp(index) => thread.temps.get(usize::from(index)).map(|v| i64::from(*v)),
            WatchField::TempXl(index) => thread
                .temp_xl
                .get(usize::from(index))
                .map(|v| i64::from(*v)),
            WatchField::Stop => return Ok(WatchValue::Stop(thread.stop.clone())),
        };
        value
            .map(WatchValue::Scalar)
            .ok_or_else(|| "watch index is outside the stored register bank".into())
    }
    /// Current stored positions, not an executed-instruction history.
    pub fn frame_positions(&self) -> Vec<FramePosition> {
        self.runtime
            .state()
            .threads
            .values()
            .flat_map(|thread| {
                thread
                    .frames
                    .iter()
                    .enumerate()
                    .map(move |(depth, frame)| FramePosition {
                        entity: thread.owner,
                        depth,
                        routine: frame.routine,
                        instruction: frame.instruction_pointer,
                        opcode: self
                            .runtime
                            .content()
                            .routines()
                            .get(frame.routine)
                            .and_then(|routine| {
                                routine
                                    .instructions()
                                    .get(usize::from(frame.instruction_pointer))
                            })
                            .map(|instruction| instruction.opcode),
                    })
            })
            .collect()
    }
}
