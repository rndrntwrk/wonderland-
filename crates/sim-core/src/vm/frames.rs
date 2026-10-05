use super::{HostRequest, RoutineKey, VmFault, VmResolution};
use crate::ids::{EntityRef, ObjectId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VmMode {
    Tso,
    Ts1,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum PrimitiveExit {
    GotoTrue = 0,
    GotoFalse = 1,
    GotoTrueNextTick = 2,
    GotoFalseNextTick = 3,
    ReturnTrue = 4,
    ReturnFalse = 5,
    Error = 6,
    ContinueNextTick = 7,
    Continue = 8,
    Interrupt = 9,
    ContinueFutureTick = 10,
}
impl PrimitiveExit {
    pub fn branch(value: bool) -> Self {
        if value {
            Self::GotoTrue
        } else {
            Self::GotoFalse
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameContext {
    pub caller: EntityRef,
    pub callee: EntityRef,
    pub stack_object: ObjectId,
    pub stack_object_ref: Option<EntityRef>,
    pub code_owner: u32,
}
impl FrameContext {
    pub fn for_entity(entity: EntityRef, code_owner: u32) -> Self {
        Self {
            caller: entity,
            callee: entity,
            stack_object: entity.object_id,
            stack_object_ref: Some(entity),
            code_owner,
        }
    }
    pub fn validate(&self) -> Result<(), VmFault> {
        let valid = |entity: EntityRef| entity.object_id.0 > 0 && entity.generation != 0;
        if !valid(self.caller) || !valid(self.callee) {
            return Err(VmFault::InvalidContent(
                "Invalid frame caller/callee reference".into(),
            ));
        }
        if let Some(entity) = self.stack_object_ref {
            if !valid(entity) || entity.object_id != self.stack_object {
                return Err(VmFault::InvalidContent(
                    "Invalid cached stack-object reference".into(),
                ));
            }
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpecialResult {
    Normal,
    Interaction { run_immediately: bool },
    Retry,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmFrame {
    pub routine: RoutineKey,
    pub instruction_pointer: u8,
    pub context: FrameContext,
    pub locals: Vec<i16>,
    pub args: Vec<i16>,
    pub special_result: SpecialResult,
    pub action_tree: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrimitiveContinuation {
    pub request_id: u64,
    pub request: HostRequest,
    pub frame_depth: usize,
    pub instruction_pointer: u8,
    pub resolution: Option<VmResolution>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VmStop {
    Ready,
    Sleeping { until_tick: u32 },
    Waiting { request_id: u64 },
    Completed(PrimitiveExit),
    Faulted(VmFault),
    BudgetExhausted,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VmDiagnostic {
    MissingPrimitive { opcode: u16 },
    MissingRoutine { code_owner: u32, id: u16 },
    EmptyRoutine { routine: RoutineKey },
    InteractionReturned { run_immediately: bool },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmThread {
    pub owner: EntityRef,
    pub mode: VmMode,
    pub frames: Vec<VmFrame>,
    pub temps: [i16; 20],
    pub temp_xl: [i32; 2],
    pub interrupt: bool,
    pub is_check: bool,
    pub schedule_idle_start: u32,
    pub last_exit: PrimitiveExit,
    pub stop: VmStop,
    pub continuation: Option<PrimitiveContinuation>,
    pub tree_advertisements: BTreeMap<(u8, u16), i16>,
    pub diagnostics: Vec<VmDiagnostic>,
    pub action_strings: Option<Vec<super::ActionString>>,
}
impl VmThread {
    pub fn new(owner: EntityRef, mode: VmMode) -> Self {
        Self {
            owner,
            mode,
            frames: Vec::new(),
            temps: [0; 20],
            temp_xl: [0; 2],
            interrupt: false,
            is_check: false,
            schedule_idle_start: 0,
            last_exit: PrimitiveExit::GotoFalse,
            stop: VmStop::Ready,
            continuation: None,
            tree_advertisements: BTreeMap::new(),
            diagnostics: Vec::new(),
            action_strings: None,
        }
    }
    pub fn top(&self) -> Result<&VmFrame, VmFault> {
        self.frames
            .last()
            .ok_or_else(|| VmFault::InvalidContent("No active VM frame".into()))
    }
    pub fn top_mut(&mut self) -> Result<&mut VmFrame, VmFault> {
        self.frames
            .last_mut()
            .ok_or_else(|| VmFault::InvalidContent("No active VM frame".into()))
    }
    pub fn diagnostic(&mut self, value: VmDiagnostic) {
        if self.diagnostics.len() < 128 {
            self.diagnostics.push(value);
        }
    }
    pub fn take_diagnostics(&mut self) -> Vec<VmDiagnostic> {
        std::mem::take(&mut self.diagnostics)
    }
}
