use super::{FrameContext, PrimitiveExit, VmMode};
use crate::ids::{EntityRef, ObjectId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VmFault {
    InvalidContent(String),
    Bounds {
        area: String,
        index: i32,
        len: usize,
    },
    MissingEntity(ObjectId),
    StaleEntity(EntityRef),
    UnsupportedScope {
        scope: u16,
        write: bool,
    },
    DeprecatedScope(u16),
    UnknownScope(u16),
    UnsupportedPrimitive {
        opcode: u16,
        case: String,
    },
    HostUnsupported(String),
    InvalidOperand {
        opcode: u16,
        detail: String,
    },
    Arithmetic(String),
    InvalidContinuation(String),
    StackLimit,
}
impl std::fmt::Display for VmFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for VmFault {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmPosition {
    pub x: i16,
    pub y: i16,
    pub level: i8,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityInfo {
    pub reference: EntityRef,
    pub guid: u32,
    pub master_guid: Option<u32>,
    pub semiglobal: Option<u32>,
    pub persistent_id: u32,
    pub is_avatar: bool,
    pub dead: bool,
    pub category: i16,
    pub family: i16,
    pub position: VmPosition,
    /// Canonical clockwise notch: N=0, NE=1, E=2, SE=3, S=4, SW=5, W=6, NW=7.
    /// The C# Direction bit mask is converted at this boundary, never exposed to providers.
    pub direction: u8,
    pub level_offset: i8,
    pub base_object: ObjectId,
    pub multi_tile: bool,
    pub group: Vec<ObjectId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EntityField {
    Attribute,
    ObjectData,
    Temp,
    Motive,
    PersonData,
    Slot,
    Definition,
    DynamicSpriteFlag,
    Function,
    TypeAttribute,
    MasterDefinition,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ClockKind {
    City,
    Standard,
    Game,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryAddress {
    Entity {
        entity: EntityRef,
        field: EntityField,
        index: u16,
    },
    Global(u16),
    Tuning {
        callee: EntityRef,
        code_owner: u32,
        table_id: u16,
        key_id: u16,
        resource_mode: u8,
    },
    Room {
        room: i32,
        index: i16,
    },
    Clock {
        kind: ClockKind,
        index: i16,
    },
    TreeAdvertisement {
        kind: u8,
        index: u16,
    },
    Neighborhood {
        scope: u16,
        object_id: ObjectId,
        data: i16,
        temp0: i16,
        temp1: i16,
    },
    MotiveLimit(i16),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityOperation {
    Delete {
        target: EntityRef,
        cleanup_all: bool,
        return_immediately: bool,
    },
    Notify {
        target: EntityRef,
    },
    PlaceInSlot {
        container: EntityRef,
        object: Option<EntityRef>,
        slot: i16,
        clean_old: bool,
    },
    ChangePosition {
        target: EntityRef,
        position: VmPosition,
        /// Canonical clockwise direction notch (0..=7), as in EntityInfo.
        direction: u8,
    },
    Create {
        guid: u32,
        position: VmPosition,
        /// Canonical clockwise direction notch (0..=7), as in EntityInfo.
        direction: u8,
        main_parameter: ObjectId,
        main_stack_object: ObjectId,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityOperationResult {
    Bool(bool),
    Created(Option<EntityRef>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimationRequest {
    pub context: FrameContext,
    pub animation_id: u16,
    pub scope: u32,
    pub mode: u8,
    pub backwards: bool,
    pub hurryable: bool,
    pub expected_events: u8,
    pub event_local: u16,
    pub store_event_in_parameter: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MotiveChangeRequest {
    pub context: FrameContext,
    pub motive: u8,
    pub raw_rate: i16,
    pub raw_max: i16,
    pub clear_all: bool,
    pub once: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteKind {
    RoutingSlot,
    RelativePosition,
    LookTowards,
    FindLocation,
    Snap,
    Reach,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteRequest {
    pub kind: RouteKind,
    pub context: FrameContext,
    pub operand: [u8; 8],
    pub parameters: Vec<i16>,
    pub locals: Vec<i16>,
    pub temps: [i16; 20],
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExternalKind {
    TransferFunds,
    Inventory,
    InvokePlugin,
    OnlineJobs,
    GenericTso,
    GenericTs1,
    Dialog,
    Sound,
    SpecialEffect,
    Other,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalRequest {
    pub opcode: u16,
    pub kind: ExternalKind,
    pub context: FrameContext,
    pub operand: [u8; 8],
    pub parameters: Vec<i16>,
    pub temps: [i16; 20],
    pub temp_xl: [i32; 2],
    pub is_check: bool,
    pub amount: Option<i32>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostRequest {
    Animation(AnimationRequest),
    Route(RouteRequest),
    External(ExternalRequest),
    MotiveChange(MotiveChangeRequest),
}
impl HostRequest {
    /// Bind a saved request to the immutable instruction which produced it.
    /// Resolved world-memory values remain latched; they are not read again on restore.
    pub fn validate_at(
        &self,
        thread: &super::VmThread,
        instruction: &super::VmInstruction,
    ) -> Result<(), VmFault> {
        self.validate()?;
        let frame = thread.top()?;
        let bytes = instruction.operand;
        let opcode = instruction.opcode;
        if self.context() != &frame.context {
            return Err(VmFault::InvalidContinuation(
                "Request/frame context mismatch".into(),
            ));
        }
        let matches = match self {
            Self::Animation(request) => {
                let flags = bytes[5];
                let raw = u16::from_le_bytes([bytes[0], bytes[1]]);
                let id = if flags & 4 != 0 {
                    frame.args[super::index(
                        "saved animation parameter",
                        i32::from(raw),
                        frame.args.len(),
                    )?] as u16
                } else {
                    raw
                };
                let scope = if flags & 4 != 0 && bytes[4] == 0 {
                    65536
                } else {
                    u32::from(bytes[4])
                };
                opcode == 44
                    && request.animation_id == id
                    && request.scope == scope
                    && request.mode == ((flags & 1) | ((flags >> 3) & 2))
                    && request.backwards == (flags & 2 != 0)
                    && request.hurryable == (flags & 64 != 0)
                    && request.expected_events == bytes[6]
                    && request.event_local == u16::from(bytes[2])
                    && request.store_event_in_parameter == (flags & 32 == 0)
            }
            Self::Route(request) => {
                let kind = match opcode {
                    16 => Some(RouteKind::FindLocation),
                    22 => Some(RouteKind::LookTowards),
                    27 => Some(RouteKind::RelativePosition),
                    45 => Some(RouteKind::RoutingSlot),
                    46 => Some(RouteKind::Snap),
                    47 => Some(RouteKind::Reach),
                    _ => None,
                };
                let mode = u16::from_le_bytes([bytes[0], bytes[1]]);
                let scope = u16::from_le_bytes([bytes[2], bytes[3]]);
                let can_request = !((opcode == 27 || opcode == 45) && thread.is_check)
                    && !(opcode == 22 && bytes[0] > 1 && thread.is_check)
                    && !(opcode == 47 && mode > 1)
                    && !(opcode == 45 && scope > 2);
                kind.as_ref() == Some(&request.kind)
                    && request.operand == bytes
                    && request.parameters == frame.args
                    && request.locals == frame.locals
                    && can_request
            }
            Self::External(request) => {
                let kind = match opcode {
                    1 if thread.mode == VmMode::Ts1 => Some(ExternalKind::GenericTs1),
                    1 => Some(ExternalKind::GenericTso),
                    23 | 48 => Some(ExternalKind::Sound),
                    25 if thread.mode == VmMode::Tso => Some(ExternalKind::TransferFunds),
                    35 => Some(ExternalKind::SpecialEffect),
                    36 | 38 | 39 => Some(ExternalKind::Dialog),
                    40 => Some(ExternalKind::OnlineJobs),
                    62 => Some(ExternalKind::InvokePlugin),
                    67 => Some(ExternalKind::Inventory),
                    _ => None,
                };
                let amount_valid = if opcode == 25 {
                    let scope = match bytes[0] {
                        0 => 7,
                        1 => 9,
                        2 => 25,
                        _ => u16::from(bytes[1]),
                    };
                    let valid_mode =
                        [0, 2, 5, 6, 7, 9, 10, 11, 12, 13, 17, 18, 19, 20, 24].contains(&bytes[7]);
                    let allowed_check = !thread.is_check || bytes[7] == 17 || bytes[4] & 1 != 0;
                    valid_mode
                        && allowed_check
                        && if bytes[7] == 17 {
                            request.amount == Some(0)
                        } else if scope == 7 {
                            request.amount
                                == Some(i32::from(i16::from_le_bytes([bytes[2], bytes[3]])))
                        } else {
                            request.amount.is_some()
                        }
                } else {
                    request.amount.is_none()
                };
                request.opcode == opcode
                    && request.operand == bytes
                    && request.parameters == frame.args
                    && kind.as_ref() == Some(&request.kind)
                    && request.is_check == thread.is_check
                    && amount_valid
            }
            Self::MotiveChange(request) => {
                opcode == 29
                    && request.motive == bytes[2]
                    && request.clear_all == (bytes[3] & 1 != 0)
                    && request.once == (bytes[3] & 2 != 0)
                    && (!request.clear_all || (request.raw_rate == 0 && request.raw_max == 0))
            }
        };
        if matches {
            Ok(())
        } else {
            Err(VmFault::InvalidContinuation(
                "Request does not match its immutable BHAV instruction".into(),
            ))
        }
    }
    pub fn context(&self) -> &FrameContext {
        match self {
            Self::Animation(request) => &request.context,
            Self::Route(request) => &request.context,
            Self::External(request) => &request.context,
            Self::MotiveChange(request) => &request.context,
        }
    }
    pub fn validate(&self) -> Result<(), VmFault> {
        self.context().validate()?;
        let oversized = match self {
            Self::Animation(request) => {
                request.mode > 3
                    || request.event_local > 255
                    || (request.scope > 255 && request.scope != 65536)
            }
            Self::Route(request) => request.parameters.len() > 255 || request.locals.len() > 1024,
            Self::External(request) => request.parameters.len() > 255 || request.opcode >= 256,
            Self::MotiveChange(request) => !request.clear_all && request.motive >= 16,
        };
        if oversized {
            Err(VmFault::InvalidContinuation(
                "Request payload exceeds bounds".into(),
            ))
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostResponse {
    Complete(PrimitiveExit),
    /// Synchronous providers can update registers without reaching into an executing thread.
    CompleteWithWrites {
        exit: PrimitiveExit,
        writes: Vec<RegisterWrite>,
    },
    NextTick,
    Pending {
        request_id: u64,
    },
    AnimationEvent(i16),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegisterTarget {
    Temp,
    TempXl,
    Local,
    Parameter,
    StackObjectId,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterWrite {
    pub target: RegisterTarget,
    pub index: u16,
    pub value: i32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmResolution {
    pub response: HostResponse,
    pub writes: Vec<RegisterWrite>,
}
impl VmResolution {
    pub fn complete(exit: PrimitiveExit) -> Self {
        Self {
            response: HostResponse::Complete(exit),
            writes: Vec::new(),
        }
    }
}

/// Authoritative runtime adapter. Defaults fail explicitly instead of silently approving a primitive.
pub trait VmHost {
    fn autonomy_context(
        &self,
        _caller: EntityRef,
    ) -> Result<crate::avatars::autonomy::AutonomyContext, VmFault> {
        Err(VmFault::HostUnsupported("autonomy_context".into()))
    }
    fn autonomy_offers(
        &mut self,
        _request: super::AutonomyOfferRequest,
    ) -> Result<super::AutonomyOffers, VmFault> {
        Err(VmFault::HostUnsupported(
            "autonomy_offers: B checked interaction offers required".into(),
        ))
    }
    fn enqueue_autonomy(&mut self, _request: super::AutonomyEnqueue) -> Result<bool, VmFault> {
        Err(VmFault::HostUnsupported(
            "enqueue_autonomy: B interaction queue required".into(),
        ))
    }
    fn attempt_push(
        &mut self,
        _owner: EntityRef,
        _context: &FrameContext,
    ) -> Result<Option<super::RoutineCall>, VmFault> {
        Err(VmFault::HostUnsupported(
            "attempt_push: B interaction queue required".into(),
        ))
    }
    fn ts1_family_budget(&self) -> Result<Option<i32>, VmFault> {
        Err(VmFault::HostUnsupported("ts1_family_budget".into()))
    }
    fn set_ts1_family_budget(&mut self, _budget: i32) -> Result<(), VmFault> {
        Err(VmFault::HostUnsupported("set_ts1_family_budget".into()))
    }
    fn ts1_inventory_read(
        &self,
        _neighbor: i16,
    ) -> Result<Option<Vec<super::Ts1InventoryItem>>, VmFault> {
        Err(VmFault::HostUnsupported("ts1_inventory_read".into()))
    }
    fn ts1_inventory_write(
        &mut self,
        _neighbor: i16,
        _items: Vec<super::Ts1InventoryItem>,
    ) -> Result<(), VmFault> {
        Err(VmFault::HostUnsupported("ts1_inventory_write".into()))
    }
    fn fire_state(&self) -> Result<super::FireState, VmFault> {
        Err(VmFault::HostUnsupported("fire_state".into()))
    }
    fn set_fire_percent(&mut self, _percent: i32) -> Result<(), VmFault> {
        Err(VmFault::HostUnsupported("set_fire_percent".into()))
    }
    fn room_is_pool(&self, _entity: EntityRef) -> Result<bool, VmFault> {
        Err(VmFault::HostUnsupported("room_is_pool".into()))
    }
    /// Source GetObjectsAt uses tile coordinates, ignoring fractional tile offsets.
    fn objects_at_tile(&self, position: VmPosition) -> Result<Vec<EntityRef>, VmFault> {
        let mut ids = self.entity_ids()?;
        ids.sort();
        ids.dedup();
        let mut found = Vec::new();
        for id in ids {
            if let Some(entity) = self.resolve_entity(id) {
                let other = self.entity_info(entity)?.position;
                if other.level == position.level
                    && other.x >> 4 == position.x >> 4
                    && other.y >> 4 == position.y >> 4
                {
                    found.push(entity);
                }
            }
        }
        Ok(found)
    }
    /// Read-only observer for hosts which temporarily remove the executing thread from their map.
    fn observe_thread(&mut self, _thread: &super::VmThread) {}
    /// Flush writes made through another entity's Temp memory view into this owner's live bank.
    /// The interpreter accepts at most 20 entries, checks every index, then applies them in order.
    fn take_temp_writes(&mut self, _owner: EntityRef) -> Vec<(u16, i16)> {
        Vec::new()
    }
    /// XL writebacks share the short-bank validation transaction; at most two entries are accepted.
    fn take_temp_xl_writes(&mut self, _owner: EntityRef) -> Vec<(u16, i32)> {
        Vec::new()
    }
    /// False for a separate outside-tick EvaluateCheck: its own bank is a clone, while scope13
    /// still addresses the selected entity's actual thread (VMThread.cs:96-106; VMMemory.cs:64/515).
    fn entity_temps_alias(&self, _owner: EntityRef) -> bool {
        true
    }
    /// Nested RunInMyStack can alter a removed ancestor. Apply this before the current primitive's
    /// result, so that its subsequent return or next-tick schedule can supersede the child's values.
    fn take_thread_control(&mut self, _owner: EntityRef) -> Option<(bool, u32, PrimitiveExit)> {
        None
    }
    fn take_reset_request(&mut self, _owner: EntityRef) -> bool {
        false
    }
    /// Interrupts can target a running owner whose thread is temporarily outside the host map.
    fn take_interrupt_request(&mut self, _owner: EntityRef) -> bool {
        false
    }
    fn relationship_read(&self, _key: super::RelationshipKey) -> Result<Option<Vec<i16>>, VmFault> {
        Err(VmFault::HostUnsupported("relationship_read".into()))
    }
    fn relationship_write(
        &mut self,
        _key: super::RelationshipKey,
        _values: Vec<i16>,
    ) -> Result<(), VmFault> {
        Err(VmFault::HostUnsupported("relationship_write".into()))
    }
    fn relationship_mark(
        &mut self,
        _key: super::RelationshipKey,
        _persistent_dirty: bool,
    ) -> Result<(), VmFault> {
        Err(VmFault::HostUnsupported("relationship_mark".into()))
    }
    fn relationship_multiplier(&self) -> Result<f32, VmFault> {
        Err(VmFault::HostUnsupported("relationship_multiplier".into()))
    }
    fn behavior_entry(
        &self,
        _entity: EntityRef,
        _entry: u8,
    ) -> Result<super::BehaviorEntry, VmFault> {
        Err(VmFault::HostUnsupported("behavior_entry".into()))
    }
    fn evaluate_check(
        &mut self,
        _request: super::RoutineCheck,
    ) -> Result<super::CheckResult, VmFault> {
        Err(VmFault::HostUnsupported("evaluate_check".into()))
    }
    fn named_tree(
        &self,
        _request: super::NameLookup,
    ) -> Result<Option<super::BoundRoutine>, VmFault> {
        Err(VmFault::HostUnsupported("named_tree".into()))
    }
    fn function_status(&self, _entity: EntityRef) -> Result<super::FunctionEntityState, VmFault> {
        Err(VmFault::HostUnsupported("function_status".into()))
    }
    fn idle_for_input(
        &mut self,
        _context: &FrameContext,
        _allow_push: bool,
        _action_tree: bool,
        _mode: VmMode,
    ) -> Result<super::IdleDecision, VmFault> {
        Err(VmFault::HostUnsupported("idle_for_input".into()))
    }
    fn interaction_state(&self, _entity: EntityRef) -> Result<super::InteractionState, VmFault> {
        Err(VmFault::HostUnsupported("interaction_state".into()))
    }
    fn push_interaction(
        &mut self,
        _request: super::PushInteractionRequest,
    ) -> Result<bool, VmFault> {
        Err(VmFault::HostUnsupported("push_interaction".into()))
    }
    fn interaction_available(
        &self,
        _source: EntityRef,
        _target: EntityRef,
        _interaction: u8,
    ) -> Result<bool, VmFault> {
        Err(VmFault::HostUnsupported("interaction_available".into()))
    }
    fn change_interaction_icon(
        &mut self,
        _caller: EntityRef,
        _icon: EntityRef,
    ) -> Result<(), VmFault> {
        Err(VmFault::HostUnsupported("change_interaction_icon".into()))
    }
    fn resolve_dialog_string(
        &self,
        _request: super::StringLookup,
    ) -> Result<Option<String>, VmFault> {
        Err(VmFault::HostUnsupported("resolve_dialog_string".into()))
    }
    fn presentation(&mut self, _request: super::PresentationRequest) -> Result<(), VmFault> {
        Err(VmFault::HostUnsupported("presentation".into()))
    }
    fn resolve_suit(
        &self,
        _request: super::SuitLookup,
    ) -> Result<Option<super::ResolvedSuit>, VmFault> {
        Err(VmFault::HostUnsupported("resolve_suit".into()))
    }
    fn apply_appearance(&mut self, _operation: super::AppearanceOperation) -> Result<(), VmFault> {
        Err(VmFault::HostUnsupported("apply_appearance".into()))
    }
    fn current_tick(&self) -> u32;
    fn next_random(&mut self, bound: u64) -> u64;
    fn resolve_entity(&self, id: ObjectId) -> Option<EntityRef>;
    fn entity_info(&self, _entity: EntityRef) -> Result<EntityInfo, VmFault> {
        Err(VmFault::HostUnsupported("entity_info".into()))
    }
    fn entity_ids(&self) -> Result<Vec<ObjectId>, VmFault> {
        Err(VmFault::HostUnsupported("entity_ids".into()))
    }
    fn read_memory(&self, _address: &MemoryAddress) -> Result<i16, VmFault> {
        Err(VmFault::HostUnsupported("read_memory".into()))
    }
    fn write_memory(&mut self, _address: &MemoryAddress, _value: i16) -> Result<bool, VmFault> {
        Err(VmFault::HostUnsupported("write_memory".into()))
    }
    fn read_list(&self, _entity: EntityRef) -> Result<Vec<i16>, VmFault> {
        Err(VmFault::HostUnsupported("read_list".into()))
    }
    fn replace_list(&mut self, _entity: EntityRef, _values: Vec<i16>) -> Result<(), VmFault> {
        Err(VmFault::HostUnsupported("replace_list".into()))
    }
    fn entity_operation(
        &mut self,
        _operation: EntityOperation,
    ) -> Result<EntityOperationResult, VmFault> {
        Err(VmFault::HostUnsupported("entity_operation".into()))
    }
    fn request(&mut self, _request: HostRequest) -> Result<HostResponse, VmFault> {
        Err(VmFault::HostUnsupported("request".into()))
    }
    fn show_money_headline(&mut self, _caller: EntityRef, _amount: i32) -> Result<(), VmFault> {
        Err(VmFault::HostUnsupported("show_money_headline".into()))
    }
    fn resolve_neighbor_or_career(
        &self,
        _mode: VmMode,
        _search: u8,
        _after: i16,
        _guid: u32,
    ) -> Result<Option<i16>, VmFault> {
        Err(VmFault::HostUnsupported("neighbor/career query".into()))
    }
    fn semiglobal_for_guid(&self, _guid: u32) -> Result<Option<u32>, VmFault> {
        Err(VmFault::HostUnsupported("semiglobal_for_guid".into()))
    }
}
