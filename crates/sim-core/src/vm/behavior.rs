use super::{FrameContext, PrimitiveExit, RoutineKey, SpecialResult, VmFault};
use crate::ids::EntityRef;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundRoutine {
    pub routine: RoutineKey,
    pub code_owner: u32,
    pub arguments: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutineCall {
    pub routine: RoutineKey,
    pub context: FrameContext,
    pub args: Vec<i16>,
    pub action_tree: bool,
    pub special_result: SpecialResult,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BehaviorEntry {
    pub action_declared: bool,
    pub action: Option<BoundRoutine>,
    pub condition_declared: bool,
    pub condition: Option<BoundRoutine>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutineCheck {
    pub routine: BoundRoutine,
    pub context: FrameContext,
    pub args: Vec<i16>,
    pub temps: [i16; 20],
    pub temp_xl: [i32; 2],
    pub run_in_owner: Option<EntityRef>,
    /// Named destination0 addresses context.Thread, which can differ from Entity.Thread in a check.
    /// EvaluateCheck and named destination1/other synchronous targets leave this false.
    pub use_current_thread: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckResult {
    pub accepted: bool,
    pub exit: PrimitiveExit,
    pub aborting: bool,
    pub temps: [i16; 20],
    pub temp_xl: [i32; 2],
    pub copy_back: bool,
    /// RunInMyStack retains the selected thread's interrupt/countdown state. EvaluateCheck is separate.
    pub thread_control: Option<(bool, u32)>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameLookup {
    pub context: FrameContext,
    pub current_routine: RoutineKey,
    pub string_table: u16,
    pub string_scope: u8,
    pub string_index: i32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionEntityState {
    pub disabled: bool,
    pub broken: bool,
    pub in_use: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdleDecision {
    Quiet,
    Notified,
    Push(RoutineCall),
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionState {
    pub action_tree: bool,
    pub callee: Option<EntityRef>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueueMode {
    Normal,
    ParentIdle,
    ParentExit,
    Idle,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushInteractionRequest {
    pub context: FrameContext,
    pub source: EntityRef,
    pub target: EntityRef,
    pub interaction: u8,
    pub priority: i16,
    pub mode: QueueMode,
    pub custom_icon: bool,
    pub icon: Option<EntityRef>,
    pub push_head: bool,
    pub push_tail: bool,
    pub skip_permissions: bool,
    pub immediate_result_chooser: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutonomyOfferRequest {
    pub context: FrameContext,
    pub temps: [i16; 20],
    pub temp_xl: [i32; 2],
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutonomyRegisters {
    pub temps: [i16; 20],
    pub temp_xl: [i32; 2],
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AutonomyOffers {
    pub candidates: Vec<crate::avatars::advertisements::InteractionCandidate>,
    pub tuning: crate::avatars::autonomy::AutonomyTuning,
    pub registers: Option<AutonomyRegisters>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutonomyEnqueue {
    pub caller: EntityRef,
    pub source: EntityRef,
    pub interaction: u8,
    pub args: [i16; 4],
    pub priority: i16,
    pub mode: QueueMode,
}

impl RoutineCall {
    pub fn validate(&self) -> Result<(), VmFault> {
        self.context.validate()?;
        if self.args.len() > 255 {
            Err(VmFault::InvalidContent(
                "Routine call argument limit".into(),
            ))
        } else {
            Ok(())
        }
    }
}
impl BoundRoutine {
    pub fn validate(&self) -> Result<(), VmFault> {
        if self.arguments > 255 {
            Err(VmFault::InvalidContent(
                "Bound routine argument limit".into(),
            ))
        } else {
            Ok(())
        }
    }
}
