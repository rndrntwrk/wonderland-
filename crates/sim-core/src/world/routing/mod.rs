//! Deterministic resumable subtile navigation. This is a bounded replacement for
//! VMRectRouter geometry, not a claim of rectangle/Bezier path parity.
use super::{
    footprints::{ObstacleMotion, WorldObject, WorldRect},
    placement::validate_floor,
    rooms::PortalTraversal,
    slots::SlotKey,
    tiles::{integer_sqrt, Cardinal, Diagonal, Facing, LotPosition},
    WorldQuery,
};
use crate::{ids::EntityRef, rng::SimRng};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const GRID_RESOLUTION: i32 = 4;
pub const MAX_ROUTE_GOALS: usize = 1_024;
pub const MAX_SEARCH_NODES: u32 = 65_536;
pub const MAX_ROUTE_DEPTH: u8 = 16;
pub const MAX_DISPATCH_BUDGET: u32 = 2_048;

#[repr(i16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteFailCode {
    Success = 0,
    Unknown = 1,
    NoRoomRoute = 2,
    NoPath = 3,
    Interrupted = 4,
    CantSit = 5,
    CantStand = 6,
    NoValidGoals = 7,
    DestTileOccupied = 8,
    DestChairOccupied = 9,
    NoChair = 10,
    WallInWay = 11,
    AltsDontMatch = 12,
    DestTileOccupiedPerson = 13,
}

impl RouteFailCode {
    pub fn code(self) -> i16 {
        self as i16
    }
    pub fn from_code(value: i16) -> Option<Self> {
        Some(match value {
            0 => Self::Success,
            1 => Self::Unknown,
            2 => Self::NoRoomRoute,
            3 => Self::NoPath,
            4 => Self::Interrupted,
            5 => Self::CantSit,
            6 => Self::CantStand,
            7 => Self::NoValidGoals,
            8 => Self::DestTileOccupied,
            9 => Self::DestChairOccupied,
            10 => Self::NoChair,
            11 => Self::WallInWay,
            12 => Self::AltsDontMatch,
            13 => Self::DestTileOccupiedPerson,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteGoal {
    pub position: LotPosition,
    pub facing: Option<Facing>,
    pub chair: Option<EntityRef>,
    pub slot: Option<SlotKey>,
    pub follows_target: bool,
}

impl RouteGoal {
    pub fn point(position: LotPosition) -> Self {
        Self {
            position,
            facing: None,
            chair: None,
            slot: None,
            follows_target: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteConfig {
    pub search_node_limit: u32,
    pub max_retries: u8,
    pub wait_timeout: u16,
    pub collision_wait: u16,
    pub shoo_wait: u16,
    pub max_dispatches: u32,
    pub allow_shoo: bool,
    pub call_failure_tree: bool,
}

impl Default for RouteConfig {
    fn default() -> Self {
        Self {
            search_node_limit: 16_384,
            max_retries: 5,
            wait_timeout: 300,
            collision_wait: 30,
            shoo_wait: 60,
            max_dispatches: 30_000,
            allow_shoo: true,
            call_failure_tree: false,
        }
    }
}

impl RouteConfig {
    fn valid(&self) -> bool {
        self.search_node_limit > 0
            && self.search_node_limit <= MAX_SEARCH_NODES
            && self.max_retries > 0
            && self.max_retries <= 32
            && self.wait_timeout > 0
            && self.wait_timeout <= 9_000
            && self.collision_wait > 0
            && self.collision_wait <= self.wait_timeout
            && self.shoo_wait > 0
            && self.shoo_wait <= self.wait_timeout
            && self.max_dispatches > 0
            && self.max_dispatches <= 1_000_000
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteRequest {
    pub id: u64,
    pub actor: EntityRef,
    pub target: Option<EntityRef>,
    pub goals: Vec<RouteGoal>,
    pub start: LotPosition,
    pub seated_on: Option<EntityRef>,
    pub config: RouteConfig,
}

impl RouteRequest {
    pub fn new(id: u64, actor: EntityRef, start: LotPosition, goals: Vec<RouteGoal>) -> Self {
        Self {
            id,
            actor,
            target: None,
            goals,
            start,
            seated_on: None,
            config: RouteConfig::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplanReason {
    ArchitectureChanged,
    TargetMoved,
    ActorMoved,
    PortalFailed,
    Blocked,
    AlternativeGoal,
    ShooCompleted,
    NestedRoute,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WaitReason {
    MovingAvatar,
    Shoo,
    Script,
    Blocked,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteCallbackKind {
    Portal {
        traversal: PortalTraversal,
        entry: LotPosition,
        exit: LotPosition,
    },
    Sit {
        goal_index: u16,
    },
    Stand,
    Shoo,
    Failure {
        code: RouteFailCode,
        blocker: Option<EntityRef>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteCallback {
    pub token: u64,
    pub route_id: u64,
    pub actor: EntityRef,
    pub target: EntityRef,
    pub kind: RouteCallbackKind,
    pub entrypoint: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallbackOutcome {
    pub success: bool,
    pub position: Option<LotPosition>,
    pub blocker: Option<EntityRef>,
}

impl CallbackOutcome {
    pub fn success() -> Self {
        Self {
            success: true,
            position: None,
            blocker: None,
        }
    }
    pub fn failure(blocker: Option<EntityRef>) -> Self {
        Self {
            success: false,
            position: None,
            blocker,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteStep {
    Progress {
        from: LotPosition,
        to: LotPosition,
        facing: Facing,
    },
    Searching {
        expanded: u32,
        total: u32,
    },
    Replan {
        reason: ReplanReason,
    },
    Wait {
        remaining: u16,
        reason: WaitReason,
    },
    Script(RouteCallback),
    NestedFinished {
        route_id: u64,
        result: Result<LotPosition, RouteFailCode>,
    },
    Arrived {
        position: LotPosition,
        goal_index: u16,
        chair: Option<EntityRef>,
        slot: Option<SlotKey>,
    },
    Failed {
        code: RouteFailCode,
        blocker: Option<EntityRef>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteError {
    InvalidRequest,
    MissingActor,
    MissingTarget,
    CallbackMismatch,
    CallbackAlreadyCompleted,
    NoPendingCallback,
    DepthLimit,
    InvalidState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoutePhase {
    Planning,
    Walking,
    Waiting,
    AwaitingScript,
    Done,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum PathLeg {
    Walk {
        to: LotPosition,
    },
    Portal {
        traversal: PortalTraversal,
        entry: LotPosition,
        exit: LotPosition,
        revision: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct OpenNode {
    position: LotPosition,
    cost: u64,
    score: u64,
    sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct GridSearch {
    start: LotPosition,
    goal: LotPosition,
    open: Vec<OpenNode>,
    costs: BTreeMap<LotPosition, u64>,
    parents: BTreeMap<LotPosition, (LotPosition, PathLeg)>,
    closed: BTreeSet<LotPosition>,
    sequence: u64,
    expanded: u32,
}

impl GridSearch {
    fn new(start: LotPosition, goal: LotPosition) -> Self {
        Self {
            start,
            goal,
            open: vec![OpenNode {
                position: start,
                cost: 0,
                score: 0,
                sequence: 0,
            }],
            costs: BTreeMap::from([(start, 0)]),
            parents: BTreeMap::new(),
            closed: BTreeSet::new(),
            sequence: 0,
            expanded: 0,
        }
    }
    fn insert(&mut self, position: LotPosition, cost: u64, score: u64) -> bool {
        let Some(sequence) = self.sequence.checked_add(1) else {
            return false;
        };
        self.sequence = sequence;
        let node = OpenNode {
            position,
            cost,
            score,
            sequence: self.sequence,
        };
        let index = self
            .open
            .partition_point(|n| (n.score, n.sequence) <= (node.score, node.sequence));
        self.open.insert(index, node);
        true
    }
    fn reconstruct(&self) -> Option<VecDeque<PathLeg>> {
        let mut result = VecDeque::new();
        let mut current = self.goal;
        while current != self.start {
            if result.len() > self.parents.len() {
                return None;
            }
            let (parent, edge) = self.parents.get(&current)?;
            result.push_front(edge.clone());
            current = *parent;
        }
        Some(result)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteContinuation {
    request: RouteRequest,
    lot_dimensions: (u16, u16, u8),
    phase: RoutePhase,
    position: LotPosition,
    facing: Facing,
    goal_index: usize,
    architecture_revision: u64,
    target_revision: Option<u64>,
    target_position: Option<LotPosition>,
    actor_revision: u64,
    search: Option<GridSearch>,
    path: VecDeque<PathLeg>,
    ignored_portals: BTreeSet<PortalTraversal>,
    avoided_avatars: BTreeSet<EntityRef>,
    shooed_avatars: BTreeSet<EntityRef>,
    collision_counts: BTreeMap<EntityRef, u8>,
    pending: Option<RouteCallback>,
    callback_result: Option<CallbackOutcome>,
    next_callback: u64,
    dispatches: u32,
    retries_left: u8,
    wait_remaining: u16,
    timeout_left: u16,
    wait_reason: WaitReason,
    blocker: Option<EntityRef>,
    failure: Option<RouteFailCode>,
    failure_tree_called: bool,
    parent_route_id: Option<u64>,
    depth: u8,
    nested: Vec<RouteContinuation>,
}

impl RouteContinuation {
    pub fn new<Q: WorldQuery + ?Sized>(
        request: RouteRequest,
        query: &Q,
    ) -> Result<Self, RouteError> {
        if request.id == 0
            || request.goals.is_empty()
            || request.goals.len() > MAX_ROUTE_GOALS
            || !request.config.valid()
            || !query.lot().contains_position(request.start)
            || request.goals.iter().any(|g| {
                !query.lot().contains_position(g.position)
                    || g.facing.is_some_and(|f| !f.valid())
                    || g.chair
                        .is_some_and(|r| r.object_id.0 <= 0 || r.generation == 0)
                    || g.slot
                        .is_some_and(|s| s.owner.object_id.0 <= 0 || s.owner.generation == 0)
            })
        {
            return Err(RouteError::InvalidRequest);
        }
        let actor = query
            .object(request.actor)
            .ok_or(RouteError::MissingActor)?;
        if actor.position != request.start || actor.position.is_out_of_world() {
            return Err(RouteError::InvalidRequest);
        }
        let target = if let Some(target) = request.target {
            Some(query.object(target).ok_or(RouteError::MissingTarget)?)
        } else {
            None
        };
        let config = request.config.clone();
        let position = request.start;
        let result = Self {
            lot_dimensions: (
                query.lot().width(),
                query.lot().height(),
                query.lot().levels(),
            ),
            request,
            phase: RoutePhase::Planning,
            position,
            facing: actor.facing,
            goal_index: 0,
            architecture_revision: query.lot().revision().architecture,
            target_revision: target.map(|o| o.revision),
            target_position: target.map(|o| o.position),
            actor_revision: actor.revision,
            search: None,
            path: VecDeque::new(),
            ignored_portals: BTreeSet::new(),
            avoided_avatars: BTreeSet::new(),
            shooed_avatars: BTreeSet::new(),
            collision_counts: BTreeMap::new(),
            pending: None,
            callback_result: None,
            next_callback: 0,
            dispatches: 0,
            retries_left: config.max_retries,
            wait_remaining: 0,
            timeout_left: config.wait_timeout,
            wait_reason: WaitReason::Blocked,
            blocker: None,
            failure: None,
            failure_tree_called: false,
            parent_route_id: None,
            depth: 0,
            nested: Vec::new(),
        };
        result.validate_against(query)?;
        Ok(result)
    }
    pub fn id(&self) -> u64 {
        self.request.id
    }
    pub fn actor(&self) -> EntityRef {
        self.request.actor
    }
    pub fn target(&self) -> Option<EntityRef> {
        self.request.target
    }
    pub fn position(&self) -> LotPosition {
        self.position
    }
    pub fn phase(&self) -> RoutePhase {
        self.phase
    }
    pub fn request(&self) -> &RouteRequest {
        &self.request
    }
    pub fn pending_callback(&self) -> Option<&RouteCallback> {
        self.nested
            .last()
            .map_or(self.pending.as_ref(), |child| child.pending_callback())
    }
    pub fn parent_route_id(&self) -> Option<u64> {
        self.parent_route_id
    }
    pub fn cancel(&mut self) {
        self.nested.clear();
        self.pending = None;
        self.callback_result = None;
        self.fail(RouteFailCode::Interrupted, None);
    }
    pub fn complete_callback(
        &mut self,
        token: u64,
        outcome: CallbackOutcome,
    ) -> Result<(), RouteError> {
        if let Some(child) = self.nested.last_mut() {
            return child.complete_callback(token, outcome);
        }
        if self.pending.as_ref().map(|p| p.token) != Some(token) {
            return Err(RouteError::CallbackMismatch);
        }
        if self.callback_result.is_some() {
            return Err(RouteError::CallbackAlreadyCompleted);
        }
        if outcome.blocker.is_some_and(|r| !r.is_valid())
            || outcome.position.is_some_and(|p| !self.contains_position(p))
        {
            return Err(RouteError::InvalidRequest);
        }
        self.callback_result = Some(outcome);
        Ok(())
    }
    pub fn complete_callback_for(
        &mut self,
        route_id: u64,
        token: u64,
        outcome: CallbackOutcome,
    ) -> Result<(), RouteError> {
        if let Some(child) = self.nested.last_mut() {
            return child.complete_callback_for(route_id, token, outcome);
        }
        if self.id() != route_id {
            return Err(RouteError::CallbackMismatch);
        }
        self.complete_callback(token, outcome)
    }
    pub fn push_nested<Q: WorldQuery + ?Sized>(
        &mut self,
        request: RouteRequest,
        query: &Q,
    ) -> Result<(), RouteError> {
        // Check every ancestor before descending; route IDs fence asynchronous replies.
        if request.id == self.id() {
            return Err(RouteError::InvalidRequest);
        }
        if let Some(child) = self.nested.last_mut() {
            return child.push_nested(request, query);
        }
        if self.pending.is_none() || self.callback_result.is_some() {
            return Err(RouteError::NoPendingCallback);
        }
        if self.depth + 1 >= MAX_ROUTE_DEPTH {
            return Err(RouteError::DepthLimit);
        }
        if request.actor != self.actor()
            || request.id == self.id()
            || request.start != self.position
        {
            return Err(RouteError::InvalidRequest);
        }
        let mut child = Self::new(request, query)?;
        child.depth = self.depth + 1;
        child.parent_route_id = Some(self.id());
        child.avoided_avatars = self.avoided_avatars.clone();
        self.nested.push(child);
        Ok(())
    }
    fn replan(&mut self, reason: ReplanReason) -> RouteStep {
        self.phase = RoutePhase::Planning;
        self.search = None;
        self.path.clear();
        RouteStep::Replan { reason }
    }
    fn fail(&mut self, code: RouteFailCode, blocker: Option<EntityRef>) {
        self.phase = RoutePhase::Failed;
        self.failure = Some(code);
        self.blocker = blocker;
        self.search = None;
        self.path.clear();
    }
    /// Source slot discovery can fail before a path search starts. Keep that
    /// failure in the same callback/timeout continuation as later route failures.
    pub fn reject(
        &mut self,
        code: RouteFailCode,
        blocker: Option<EntityRef>,
    ) -> Result<(), RouteError> {
        if code == RouteFailCode::Success || blocker.is_some_and(|r| !r.is_valid()) {
            return Err(RouteError::InvalidRequest);
        }
        self.pending = None;
        self.callback_result = None;
        self.nested.clear();
        self.fail(code, blocker);
        Ok(())
    }
    fn terminal_failure_step(&self) -> RouteStep {
        RouteStep::Failed {
            code: self.failure.unwrap_or(RouteFailCode::Unknown),
            blocker: self.blocker,
        }
    }
    fn failure_step(&mut self) -> RouteStep {
        if self.request.config.call_failure_tree
            && self.parent_route_id.is_none()
            && !self.failure_tree_called
        {
            self.failure_tree_called = true;
            return self.callback(
                self.actor(),
                RouteCallbackKind::Failure {
                    code: self.failure.unwrap_or(RouteFailCode::Unknown),
                    blocker: self.blocker,
                },
                398,
            );
        }
        self.terminal_failure_step()
    }
    fn arrived(&self) -> RouteStep {
        let goal = &self.request.goals[self.goal_index];
        RouteStep::Arrived {
            position: self.position,
            goal_index: self.goal_index as u16,
            chair: goal.chair,
            slot: goal.slot,
        }
    }
    fn callback(
        &mut self,
        target: EntityRef,
        kind: RouteCallbackKind,
        entrypoint: u16,
    ) -> RouteStep {
        self.next_callback += 1;
        self.timeout_left = self.request.config.wait_timeout;
        let call = RouteCallback {
            token: self.next_callback,
            route_id: self.id(),
            actor: self.actor(),
            target,
            kind,
            entrypoint,
        };
        self.phase = RoutePhase::AwaitingScript;
        self.pending = Some(call.clone());
        RouteStep::Script(call)
    }
    fn soft_fail(&mut self, code: RouteFailCode, blocker: Option<EntityRef>) -> RouteStep {
        if self.goal_index + 1 < self.request.goals.len() {
            self.goal_index += 1;
            self.retries_left = self.request.config.max_retries;
            self.timeout_left = self.request.config.wait_timeout;
            self.blocker = None;
            self.replan(ReplanReason::AlternativeGoal)
        } else {
            self.fail(code, blocker);
            self.failure_step()
        }
    }
    /// One call is one source route-frame dispatch and consumes exactly NextRandom(1).
    /// When a nested route runs, only that active child's dispatch consumes the draw.
    pub fn step<Q: WorldQuery + ?Sized>(
        &mut self,
        query: &Q,
        rng: &mut SimRng,
        budget: u32,
    ) -> RouteStep {
        if let Some(child) = self.nested.last_mut() {
            let result = child.step(query, rng, budget);
            let terminal = match &result {
                RouteStep::Arrived { position, .. } => Some(Ok(*position)),
                RouteStep::Failed { code, .. } => Some(Err(*code)),
                _ => None,
            };
            if let Some(result) = terminal {
                let child = self.nested.pop().expect("active child");
                let route_id = child.id();
                self.position = child.position;
                self.facing = child.facing;
                self.avoided_avatars.extend(child.avoided_avatars);
                return RouteStep::NestedFinished { route_id, result };
            }
            return result;
        }
        let _ = rng.next(1);
        self.dispatches = self.dispatches.saturating_add(1);
        if self.phase == RoutePhase::Done {
            return self.arrived();
        }
        let Some(actor) = query.object(self.actor()) else {
            self.pending = None;
            self.callback_result = None;
            self.fail(RouteFailCode::Interrupted, None);
            return self.terminal_failure_step();
        };
        if !query.lot().contains_position(actor.position) {
            self.pending = None;
            self.callback_result = None;
            self.fail(RouteFailCode::Interrupted, None);
            return self.terminal_failure_step();
        }
        let failure_callback = self
            .pending
            .as_ref()
            .is_some_and(|p| matches!(p.kind, RouteCallbackKind::Failure { .. }));
        if self.dispatches > self.request.config.max_dispatches && !failure_callback {
            self.pending = None;
            self.callback_result = None;
            self.fail(RouteFailCode::NoPath, self.blocker);
            return self.failure_step();
        }
        if self.phase == RoutePhase::Failed {
            return self.failure_step();
        }
        if let Some(outcome) = self.callback_result.take() {
            return self.apply_callback(outcome, query);
        }
        if let Some(pending) = &self.pending {
            if query.object(pending.target).is_none() {
                return self.apply_callback(CallbackOutcome::failure(Some(pending.target)), query);
            }
            self.timeout_left = self.timeout_left.saturating_sub(1);
            if self.timeout_left == 0 {
                return self.apply_callback(CallbackOutcome::failure(Some(pending.target)), query);
            }
            return RouteStep::Wait {
                remaining: self.timeout_left,
                reason: WaitReason::Script,
            };
        }
        if let Some(target) = self.target() {
            let Some(object) = query.object(target) else {
                self.fail(RouteFailCode::NoValidGoals, Some(target));
                return self.failure_step();
            };
            if !query.lot().contains_position(object.position) {
                self.fail(RouteFailCode::NoValidGoals, Some(target));
                return self.failure_step();
            }
            if Some(object.revision) != self.target_revision {
                if let Some(old) = self.target_position {
                    let dx = object.position.x - old.x;
                    let dy = object.position.y - old.y;
                    let dz = i16::from(object.position.level) - i16::from(old.level);
                    for goal in &mut self.request.goals {
                        if goal.follows_target {
                            let Some(position) = i8::try_from(dz)
                                .ok()
                                .and_then(|dz| goal.position.offset(dx, dy, dz))
                                .filter(|p| query.lot().contains_position(*p))
                            else {
                                self.fail(RouteFailCode::NoValidGoals, Some(target));
                                return self.failure_step();
                            };
                            goal.position = position;
                        }
                    }
                }
                self.target_revision = Some(object.revision);
                self.target_position = Some(object.position);
                return self.replan(ReplanReason::TargetMoved);
            }
        }
        if query.lot().revision().architecture != self.architecture_revision {
            self.architecture_revision = query.lot().revision().architecture;
            self.ignored_portals.clear();
            return self.replan(ReplanReason::ArchitectureChanged);
        }
        if actor.position != self.position && actor.revision != self.actor_revision {
            self.position = actor.position;
            self.actor_revision = actor.revision;
            return self.replan(ReplanReason::ActorMoved);
        }
        self.actor_revision = actor.revision;
        if self.wait_remaining > 0 {
            self.wait_remaining -= 1;
            self.timeout_left = self.timeout_left.saturating_sub(1);
            if self.timeout_left == 0 {
                return self.soft_fail(RouteFailCode::NoPath, self.blocker);
            }
            if self.wait_remaining == 0 {
                return self.replan(if self.wait_reason == WaitReason::Shoo {
                    ReplanReason::ShooCompleted
                } else {
                    ReplanReason::Blocked
                });
            }
            return RouteStep::Wait {
                remaining: self.wait_remaining,
                reason: self.wait_reason,
            };
        }
        let goal = self.request.goals[self.goal_index].clone();
        if let Some(chair) = self.request.seated_on {
            if goal.chair != Some(chair) {
                if query
                    .object(chair)
                    .is_some_and(|o| o.entrypoints.contains(&27))
                {
                    return self.callback(chair, RouteCallbackKind::Stand, 27);
                }
                self.fail(RouteFailCode::CantStand, Some(chair));
                return self.failure_step();
            }
        }
        if let Some(chair) = goal.chair {
            let Some(object) = query.object(chair) else {
                return self.soft_fail(RouteFailCode::NoChair, Some(chair));
            };
            if let Some(slot) = goal.slot {
                if !query.slots().has_capacity(slot, self.actor(), 0) {
                    return self.soft_fail(RouteFailCode::DestChairOccupied, Some(chair));
                }
            }
            if query.lot().room_at(self.position) == query.lot().room_at(goal.position) {
                if !object.entrypoints.contains(&26) {
                    return self.soft_fail(RouteFailCode::CantSit, Some(chair));
                }
                return self.callback(
                    chair,
                    RouteCallbackKind::Sit {
                        goal_index: self.goal_index as u16,
                    },
                    26,
                );
            }
        }
        if self.position == goal.position && goal.chair.is_none() {
            self.phase = RoutePhase::Done;
            if let Some(f) = goal.facing {
                self.facing = f;
            }
            return self.arrived();
        }
        if self.phase == RoutePhase::Planning || self.search.is_some() {
            if self.search.is_none() {
                self.search = Some(GridSearch::new(self.position, goal.position));
            }
            let mut search = self.search.take().expect("initialized search");
            let progress =
                advance_search(&mut search, self, query, budget.min(MAX_DISPATCH_BUDGET));
            match progress {
                SearchProgress::Pending(expanded) => {
                    let total = search.expanded;
                    self.search = Some(search);
                    return RouteStep::Searching { expanded, total };
                }
                SearchProgress::Found(path) => {
                    self.path = path;
                    self.phase = RoutePhase::Walking;
                    return RouteStep::Searching {
                        expanded: 0,
                        total: search.expanded,
                    };
                }
                SearchProgress::Failed => {
                    if let Some(blocker) = self.blocker {
                        if self.request.config.allow_shoo
                            && !self.shooed_avatars.contains(&blocker)
                            && query.object(blocker).is_some_and(|o| {
                                o.rules.is_avatar && o.motion != ObstacleMotion::BeingShooed
                            })
                        {
                            self.shooed_avatars.insert(blocker);
                            return self.callback(blocker, RouteCallbackKind::Shoo, 4107);
                        }
                    }
                    let code = if self.position.level != goal.position.level
                        || query.lot().room_at(self.position) != query.lot().room_at(goal.position)
                    {
                        RouteFailCode::NoRoomRoute
                    } else {
                        RouteFailCode::NoPath
                    };
                    return self.soft_fail(code, self.blocker);
                }
            }
        }
        let Some(leg) = self.path.front().cloned() else {
            return self.replan(ReplanReason::Blocked);
        };
        match leg {
            PathLeg::Portal {
                traversal,
                entry,
                exit,
                revision,
            } => {
                let Some(portal) = query.lot().portals().get(&traversal.id) else {
                    return self.replan(ReplanReason::ArchitectureChanged);
                };
                if portal.revision != revision
                    || !portal.enabled
                    || query.object(portal.entity).is_none()
                {
                    self.ignored_portals.insert(traversal);
                    return self.replan(ReplanReason::PortalFailed);
                }
                if !query
                    .object(portal.entity)
                    .is_some_and(|o| o.entrypoints.contains(&15))
                {
                    self.ignored_portals.insert(traversal);
                    return self.replan(ReplanReason::PortalFailed);
                }
                self.callback(
                    portal.entity,
                    RouteCallbackKind::Portal {
                        traversal,
                        entry,
                        exit,
                    },
                    15,
                )
            }
            PathLeg::Walk { to } => {
                if let Some(blocker) = movement_blocker(
                    actor,
                    self.position,
                    to,
                    query,
                    &BTreeSet::new(),
                    false,
                    goal.chair,
                ) {
                    return self.handle_blocker(blocker, query);
                }
                if !segment_geometry_clear(actor, self.position, to, query) {
                    self.retries_left = self.retries_left.saturating_sub(1);
                    if self.retries_left == 0 {
                        return self.soft_fail(RouteFailCode::NoPath, None);
                    }
                    return self.replan(ReplanReason::Blocked);
                }
                self.path.pop_front();
                let from = self.position;
                self.position = to;
                self.facing = movement_facing(from, to, self.facing);
                self.timeout_left = self.request.config.wait_timeout;
                self.blocker = None;
                RouteStep::Progress {
                    from,
                    to,
                    facing: self.facing,
                }
            }
        }
    }
    fn handle_blocker<Q: WorldQuery + ?Sized>(
        &mut self,
        blocker: EntityRef,
        query: &Q,
    ) -> RouteStep {
        self.blocker = Some(blocker);
        let count = self.collision_counts.entry(blocker).or_insert(0);
        *count = count.saturating_add(1);
        let object = query.object(blocker);
        if object.is_some_and(|o| o.rules.is_avatar) {
            if *count > 1 || object.is_some_and(|o| o.motion != ObstacleMotion::MovingAvatar) {
                self.avoided_avatars.insert(blocker);
                self.retries_left = self.retries_left.saturating_sub(1);
            }
            if self.retries_left == 0 {
                return self.soft_fail(RouteFailCode::DestTileOccupiedPerson, Some(blocker));
            }
            if object.is_some_and(|o| o.motion == ObstacleMotion::MovingAvatar) {
                self.wait_remaining = self.request.config.collision_wait;
                self.wait_reason = WaitReason::MovingAvatar;
                self.phase = RoutePhase::Waiting;
                return RouteStep::Wait {
                    remaining: self.wait_remaining,
                    reason: self.wait_reason,
                };
            }
            self.replan(ReplanReason::Blocked)
        } else {
            self.retries_left = self.retries_left.saturating_sub(1);
            if self.retries_left == 0 {
                return self.soft_fail(RouteFailCode::DestTileOccupied, Some(blocker));
            }
            self.replan(ReplanReason::Blocked)
        }
    }
    fn apply_callback<Q: WorldQuery + ?Sized>(
        &mut self,
        outcome: CallbackOutcome,
        query: &Q,
    ) -> RouteStep {
        let Some(call) = self.pending.take() else {
            self.fail(RouteFailCode::Unknown, None);
            return self.failure_step();
        };
        let alive = query.object(call.target).is_some();
        let success = outcome.success && alive;
        match call.kind {
            RouteCallbackKind::Portal {
                traversal,
                entry,
                exit,
            } => {
                let destination = outcome.position.unwrap_or(exit);
                let recorded_revision = match self.path.front() {
                    Some(PathLeg::Portal {
                        traversal: recorded,
                        entry: old_entry,
                        exit: old_exit,
                        revision,
                    }) if *recorded == traversal && *old_entry == entry && *old_exit == exit => {
                        Some(*revision)
                    }
                    _ => None,
                };
                let valid = success
                    && destination == exit
                    && query.lot().contains_position(destination)
                    && query.lot().portals().get(&traversal.id).is_some_and(|p| {
                        p.enabled
                            && p.entity == call.target
                            && (!traversal.reverse || p.bidirectional)
                            && p.endpoints(traversal.reverse) == (entry, exit)
                            && Some(p.revision) == recorded_revision
                    });
                if !valid {
                    self.ignored_portals.insert(traversal);
                    self.blocker = outcome.blocker.or(Some(call.target));
                    return self.replan(ReplanReason::PortalFailed);
                }
                self.path.pop_front();
                let from = self.position;
                self.position = destination;
                self.phase = RoutePhase::Walking;
                self.timeout_left = self.request.config.wait_timeout;
                RouteStep::Progress {
                    from,
                    to: destination,
                    facing: self.facing,
                }
            }
            RouteCallbackKind::Sit { goal_index } => {
                if !success || usize::from(goal_index) != self.goal_index {
                    return self.soft_fail(
                        RouteFailCode::CantSit,
                        outcome.blocker.or(Some(call.target)),
                    );
                }
                if let Some(position) = outcome.position {
                    if !query.lot().contains_position(position) {
                        return self.soft_fail(RouteFailCode::CantSit, Some(call.target));
                    }
                    let from = self.position;
                    self.position = position;
                    self.phase = RoutePhase::Done;
                    return RouteStep::Progress {
                        from,
                        to: position,
                        facing: self.facing,
                    };
                }
                self.phase = RoutePhase::Done;
                self.arrived()
            }
            RouteCallbackKind::Stand => {
                if !success {
                    return self.soft_fail(
                        RouteFailCode::CantStand,
                        outcome.blocker.or(Some(call.target)),
                    );
                }
                self.request.seated_on = None;
                if let Some(position) = outcome.position {
                    if !query.lot().contains_position(position) {
                        self.fail(RouteFailCode::CantStand, Some(call.target));
                        return self.failure_step();
                    }
                    let from = self.position;
                    self.position = position;
                    self.phase = RoutePhase::Planning;
                    self.search = None;
                    self.path.clear();
                    return RouteStep::Progress {
                        from,
                        to: position,
                        facing: self.facing,
                    };
                }
                self.replan(ReplanReason::NestedRoute)
            }
            RouteCallbackKind::Shoo => {
                if !success {
                    self.retries_left = self.retries_left.saturating_sub(1);
                    if self.retries_left == 0 {
                        return self.soft_fail(
                            RouteFailCode::DestTileOccupiedPerson,
                            outcome.blocker.or(Some(call.target)),
                        );
                    }
                }
                self.wait_remaining = self.request.config.shoo_wait;
                self.wait_reason = WaitReason::Shoo;
                self.phase = RoutePhase::Waiting;
                self.avoided_avatars.remove(&call.target);
                RouteStep::Wait {
                    remaining: self.wait_remaining,
                    reason: self.wait_reason,
                }
            }
            RouteCallbackKind::Failure { code, .. } => {
                self.fail(code, self.blocker);
                self.failure_step()
            }
        }
    }
    pub fn validate(&self) -> Result<(), RouteError> {
        self.validate_at_depth(self.depth)?;
        let mut ids = BTreeSet::new();
        let mut cursor = Some(self);
        while let Some(route) = cursor {
            if !ids.insert(route.id()) {
                return Err(RouteError::InvalidState);
            }
            cursor = route.nested.first();
        }
        Ok(())
    }
    fn contains_position(&self, position: LotPosition) -> bool {
        position.x >= 0
            && position.y >= 0
            && position.x < i32::from(self.lot_dimensions.0) * 16
            && position.y < i32::from(self.lot_dimensions.1) * 16
            && position.level > 0
            && position.level <= self.lot_dimensions.2
    }
    fn validate_at_depth(&self, depth: u8) -> Result<(), RouteError> {
        if self.request.id == 0
            || self.request.actor.object_id.0 <= 0
            || self.request.actor.generation == 0
            || self.request.goals.is_empty()
            || self.request.goals.len() > MAX_ROUTE_GOALS
            || self.goal_index >= self.request.goals.len()
            || !self.request.config.valid()
            || self.depth != depth
            || depth >= MAX_ROUTE_DEPTH
            || self.nested.len() > 1
            || self.path.len() > self.request.config.search_node_limit as usize
            || self.ignored_portals.len() > 16_384
            || self.avoided_avatars.len() > 32_767
            || self.collision_counts.len() > 32_767
            || self.retries_left > self.request.config.max_retries
            || self.wait_remaining > self.request.config.wait_timeout
            || self.timeout_left > self.request.config.wait_timeout
            || self.dispatches
                > self
                    .request
                    .config
                    .max_dispatches
                    .saturating_add(u32::from(self.request.config.wait_timeout))
                    .saturating_add(1)
            || !self.facing.valid()
            || self.lot_dimensions.0 == 0
            || self.lot_dimensions.0 > super::lot::MAX_LOT_DIMENSION
            || self.lot_dimensions.1 == 0
            || self.lot_dimensions.1 > super::lot::MAX_LOT_DIMENSION
            || self.lot_dimensions.2 == 0
            || self.lot_dimensions.2 > super::lot::MAX_LOT_LEVELS
            || self.pending.as_ref().is_some_and(|p| {
                p.route_id != self.id()
                    || p.actor != self.actor()
                    || p.token != self.next_callback
                    || p.target.object_id.0 <= 0
                    || p.target.generation == 0
            })
            || (self.callback_result.is_some() && self.pending.is_none())
        {
            return Err(RouteError::InvalidState);
        }
        let valid_ref = |r: EntityRef| r.object_id.0 > 0 && r.generation > 0;
        if self.request.target.is_some_and(|r| !valid_ref(r))
            || self.request.seated_on.is_some_and(|r| !valid_ref(r))
            || self.request.goals.iter().any(|g| {
                g.facing.is_some_and(|f| !f.valid())
                    || g.chair.is_some_and(|r| !valid_ref(r))
                    || g.slot.is_some_and(|s| !valid_ref(s.owner))
            })
            || self
                .avoided_avatars
                .iter()
                .chain(self.shooed_avatars.iter())
                .chain(self.collision_counts.keys())
                .any(|r| !valid_ref(*r))
            || self.shooed_avatars.len() > 32_767
            || self.blocker.is_some_and(|r| !valid_ref(r))
            || self.ignored_portals.iter().any(|p| p.id.0 == 0)
            || (self.pending.is_some() != (self.phase == RoutePhase::AwaitingScript))
            || self.next_callback > u64::from(self.dispatches)
            || self
                .callback_result
                .as_ref()
                .and_then(|r| r.blocker)
                .is_some_and(|r| !valid_ref(r))
        {
            return Err(RouteError::InvalidState);
        }
        if let Some(call) = &self.pending {
            let valid = match call.kind {
                RouteCallbackKind::Portal { traversal, .. } => {
                    call.entrypoint == 15 && traversal.id.0 > 0
                }
                RouteCallbackKind::Sit { goal_index } => {
                    call.entrypoint == 26
                        && usize::from(goal_index) == self.goal_index
                        && self.request.goals[self.goal_index].chair == Some(call.target)
                }
                RouteCallbackKind::Stand => {
                    call.entrypoint == 27 && self.request.seated_on == Some(call.target)
                }
                RouteCallbackKind::Shoo => call.entrypoint == 4107,
                RouteCallbackKind::Failure { code, blocker } => {
                    call.entrypoint == 398
                        && call.target == self.actor()
                        && self.failure == Some(code)
                        && code != RouteFailCode::Success
                        && blocker == self.blocker
                        && self.failure_tree_called
                        && self.parent_route_id.is_none()
                        && self.request.config.call_failure_tree
                }
            };
            if !valid || call.token == 0 {
                return Err(RouteError::InvalidState);
            }
        }
        if let Some(search) = &self.search {
            let bound = self.request.config.search_node_limit as usize;
            if self.phase != RoutePhase::Planning
                || search.start != self.position
                || search.goal != self.request.goals[self.goal_index].position
                || search.closed.len() > bound
                || search.costs.len() > bound
                || search.parents.len() > bound
                || search.open.len() > bound * 8
                || search.expanded > self.request.config.search_node_limit
                || search.sequence
                    > u64::from(search.expanded) * (super::lot::MAX_PORTALS as u64 * 2 + 10)
                || search.closed.len() != search.expanded as usize
                || search.costs.get(&search.start) != Some(&0)
                || search.parents.contains_key(&search.start)
                || search.closed.iter().any(|p| !search.costs.contains_key(p))
                || search
                    .open
                    .windows(2)
                    .any(|w| (w[0].score, w[0].sequence) > (w[1].score, w[1].sequence))
            {
                return Err(RouteError::InvalidState);
            }
            if search.parents.iter().any(|(child, (parent, leg))| {
                !search.costs.contains_key(child)
                    || !search.costs.contains_key(parent)
                    || search.costs.get(parent) >= search.costs.get(child)
                    || *child != leg_destination(leg)
                    || match leg {
                        PathLeg::Walk { to } => parent.level != to.level,
                        PathLeg::Portal { entry, .. } => *entry != *parent,
                    }
            }) || search.open.iter().any(|n| {
                n.score < n.cost
                    || n.sequence > search.sequence
                    || search
                        .costs
                        .get(&n.position)
                        .map_or(true, |best| n.cost < *best)
            }) {
                return Err(RouteError::InvalidState);
            }
        }
        for child in &self.nested {
            if child.parent_route_id != Some(self.id())
                || child.actor() != self.actor()
                || child.id() == self.id()
                || self.pending.is_none()
            {
                return Err(RouteError::InvalidState);
            }
            child.validate_at_depth(depth + 1)?;
        }
        Ok(())
    }
    pub fn validate_against<Q: WorldQuery + ?Sized>(&self, query: &Q) -> Result<(), RouteError> {
        self.validate()?;
        if self.lot_dimensions
            != (
                query.lot().width(),
                query.lot().height(),
                query.lot().levels(),
            )
        {
            return Err(RouteError::InvalidState);
        }
        let bad_path = self.path.iter().any(|leg| {
            !query.lot().contains_position(leg_destination(leg))
                || matches!(leg,PathLeg::Portal{entry,..}if !query.lot().contains_position(*entry))
        });
        let bad_search = self
            .search
            .as_ref()
            .is_some_and(|s| s.costs.keys().any(|p| !query.lot().contains_position(*p)));
        if !query.lot().contains_position(self.position)
            || !query.lot().contains_position(self.request.start)
            || self
                .target_position
                .is_some_and(|p| !query.lot().contains_position(p))
            || self
                .request
                .goals
                .iter()
                .any(|g| !query.lot().contains_position(g.position))
            || bad_path
            || bad_search
            || self
                .callback_result
                .as_ref()
                .and_then(|r| r.position)
                .is_some_and(|p| !query.lot().contains_position(p))
        {
            return Err(RouteError::InvalidState);
        }
        if query.object(self.actor()).is_none() {
            return Err(RouteError::MissingActor);
        }
        let failure_callback = self
            .pending
            .as_ref()
            .is_some_and(|p| matches!(p.kind, RouteCallbackKind::Failure { .. }));
        if self
            .pending
            .as_ref()
            .is_some_and(|p| query.object(p.actor).is_none() || query.object(p.target).is_none())
        {
            return Err(RouteError::MissingTarget);
        }
        // A failure tree still runs in the live actor, while failed route
        // targets/chairs/slots are now historical and can have been deleted.
        if self.phase != RoutePhase::Failed && !failure_callback {
            if self.target().is_some_and(|r| query.object(r).is_none())
                || self
                    .request
                    .seated_on
                    .is_some_and(|r| query.object(r).is_none())
                || self.pending.as_ref().is_some_and(|p| {
                    query.object(p.actor).is_none() || query.object(p.target).is_none()
                })
            {
                return Err(RouteError::MissingTarget);
            }
            if let Some(call) = &self.pending {
                if let RouteCallbackKind::Portal { entry, exit, .. } = call.kind {
                    if !query.lot().contains_position(entry) || !query.lot().contains_position(exit)
                    {
                        return Err(RouteError::InvalidState);
                    }
                }
            }
            if self.request.goals[self.goal_index..].iter().any(|g| {
                g.chair.is_some_and(|r| query.object(r).is_none())
                    || g.slot.is_some_and(|s| {
                        query.object(s.owner).is_none() || query.slots().get(s).is_none()
                    })
            }) {
                return Err(RouteError::MissingTarget);
            }
        }
        for child in &self.nested {
            child.validate_against(query)?;
        }
        Ok(())
    }
}

enum SearchProgress {
    Pending(u32),
    Found(VecDeque<PathLeg>),
    Failed,
}
fn leg_destination(leg: &PathLeg) -> LotPosition {
    match leg {
        PathLeg::Walk { to } => *to,
        PathLeg::Portal { exit, .. } => *exit,
    }
}

fn advance_search<Q: WorldQuery + ?Sized>(
    search: &mut GridSearch,
    route: &RouteContinuation,
    query: &Q,
    budget: u32,
) -> SearchProgress {
    let Some(actor) = query.object(route.actor()) else {
        return SearchProgress::Failed;
    };
    let goal = &route.request.goals[route.goal_index];
    let mut expanded = 0;
    while expanded < budget {
        let mut current = None;
        while !search.open.is_empty() {
            let node = search.open.remove(0);
            if !search.closed.contains(&node.position)
                && search.costs.get(&node.position) == Some(&node.cost)
            {
                current = Some(node);
                break;
            }
        }
        let Some(current) = current else {
            return SearchProgress::Failed;
        };
        if current.position == search.goal {
            return search
                .reconstruct()
                .map_or(SearchProgress::Failed, SearchProgress::Found);
        }
        if search.expanded >= route.request.config.search_node_limit {
            return SearchProgress::Failed;
        }
        search.closed.insert(current.position);
        search.expanded += 1;
        expanded += 1;
        let mut neighbors: Vec<(LotPosition, u64, PathLeg)> = Vec::new();
        for next in grid_neighbors(current.position, search.goal) {
            if !query.lot().contains_position(next)
                || !segment_geometry_clear(actor, current.position, next, query)
                || movement_blocker(
                    actor,
                    current.position,
                    next,
                    query,
                    &route.avoided_avatars,
                    true,
                    goal.chair,
                )
                .is_some()
            {
                continue;
            }
            let cost = integer_sqrt(current.position.distance_squared(next) * 1_048_576).max(1);
            neighbors.push((next, cost, PathLeg::Walk { to: next }));
        }
        for portal in query
            .lot()
            .portals()
            .values()
            .filter(|p| p.enabled && query.object(p.entity).is_some())
        {
            for reverse in [false, true] {
                if reverse && !portal.bidirectional {
                    continue;
                }
                let traversal = PortalTraversal {
                    id: portal.id,
                    reverse,
                };
                if route.ignored_portals.contains(&traversal) {
                    continue;
                }
                let (entry, exit) = portal.endpoints(reverse);
                if current.position == entry {
                    neighbors.push((
                        exit,
                        u64::from(portal.cost) * 1_024,
                        PathLeg::Portal {
                            traversal,
                            entry,
                            exit,
                            revision: portal.revision,
                        },
                    ));
                } else if current.position.level == entry.level
                    && (current.position.x - entry.x).abs() <= 8
                    && (current.position.y - entry.y).abs() <= 8
                    && segment_geometry_clear(actor, current.position, entry, query)
                    && movement_blocker(
                        actor,
                        current.position,
                        entry,
                        query,
                        &route.avoided_avatars,
                        true,
                        Some(portal.entity),
                    )
                    .is_none()
                {
                    neighbors.push((
                        entry,
                        integer_sqrt(current.position.distance_squared(entry) * 1_048_576).max(1),
                        PathLeg::Walk { to: entry },
                    ));
                }
            }
        }
        for (next, edge_cost, leg) in neighbors {
            if search.closed.contains(&next) {
                continue;
            }
            let cost = current.cost.saturating_add(edge_cost);
            if search
                .costs
                .get(&next)
                .map_or(true, |previous| cost < *previous)
            {
                if !search.costs.contains_key(&next)
                    && search.costs.len() >= route.request.config.search_node_limit as usize
                {
                    continue;
                }
                if search.open.len() >= route.request.config.search_node_limit as usize * 8 {
                    return SearchProgress::Failed;
                }
                search.costs.insert(next, cost);
                search.parents.insert(next, (current.position, leg));
                let heuristic =
                    if query.lot().portals().is_empty() && next.level == search.goal.level {
                        u64::from(
                            (next.x - search.goal.x)
                                .unsigned_abs()
                                .max((next.y - search.goal.y).unsigned_abs()),
                        ) * 1_024
                    } else {
                        0
                    };
                if !search.insert(next, cost, cost.saturating_add(heuristic)) {
                    return SearchProgress::Failed;
                }
            }
        }
    }
    SearchProgress::Pending(expanded)
}

fn grid_neighbors(position: LotPosition, goal: LotPosition) -> Vec<LotPosition> {
    let mut result = Vec::with_capacity(10);
    if position.level == goal.level
        && (position.x - goal.x).abs() <= GRID_RESOLUTION
        && (position.y - goal.y).abs() <= GRID_RESOLUTION
        && position != goal
    {
        result.push(goal);
    }
    if (position.x - 2).rem_euclid(GRID_RESOLUTION) == 0
        && (position.y - 2).rem_euclid(GRID_RESOLUTION) == 0
    {
        for (dx, dy) in [
            (0, -4),
            (4, 0),
            (0, 4),
            (-4, 0),
            (4, -4),
            (4, 4),
            (-4, 4),
            (-4, -4),
        ] {
            if let Some(next) = position.offset(dx, dy, 0) {
                result.push(next);
            }
        }
    } else {
        let (x, y) = (
            (position.x - 2).div_euclid(4) * 4 + 2,
            (position.y - 2).div_euclid(4) * 4 + 2,
        );
        for (dx, dy) in [
            (0, 0),
            (4, 0),
            (0, 4),
            (4, 4),
            (-4, 0),
            (0, -4),
            (-4, -4),
            (4, -4),
            (-4, 4),
        ] {
            let next = LotPosition::new(x + dx, y + dy, position.level);
            if next != position {
                result.push(next);
            }
        }
    }
    let mut seen = BTreeSet::new();
    result.retain(|p| seen.insert(*p));
    result
}

fn movement_facing(from: LotPosition, to: LotPosition, previous: Facing) -> Facing {
    match ((to.x - from.x).signum(), (to.y - from.y).signum()) {
        (0, -1) => Facing(0),
        (1, -1) => Facing(1),
        (1, 0) => Facing(2),
        (1, 1) => Facing(3),
        (0, 1) => Facing(4),
        (-1, 1) => Facing(5),
        (-1, 0) => Facing(6),
        (-1, -1) => Facing(7),
        _ => previous,
    }
}

/// Collision traversal samples every integer subtile along the line; walls and diagonals
/// are additionally tested against the actor's full footprint at each sample.
fn sample_line(from: LotPosition, to: LotPosition) -> Vec<LotPosition> {
    if from.level != to.level {
        return Vec::new();
    }
    let (dx, dy) = ((to.x - from.x).abs(), (to.y - from.y).abs());
    if dx > 8 || dy > 8 {
        return Vec::new();
    }
    let steps = dx.max(dy).max(1);
    let mut result = Vec::with_capacity(steps as usize);
    for i in 1..=steps {
        result.push(LotPosition::new(
            from.x + (to.x - from.x) * i / steps,
            from.y + (to.y - from.y) * i / steps,
            from.level,
        ));
    }
    result
}

pub fn segment_geometry_clear<Q: WorldQuery + ?Sized>(
    actor: &WorldObject,
    from: LotPosition,
    to: LotPosition,
    query: &Q,
) -> bool {
    if from.level != to.level
        || !query.lot().contains_position(from)
        || !query.lot().contains_position(to)
    {
        return false;
    }
    if crosses_wall_centerline(from, to, query) {
        return false;
    }
    let samples = sample_line(from, to);
    if samples.is_empty() {
        return false;
    }
    samples.into_iter().all(|position| {
        let lot = query.lot();
        let rects = if actor.rules.zero_extent {
            vec![WorldRect {
                min_x: position.x,
                min_y: position.y,
                max_x: position.x + 1,
                max_y: position.y + 1,
                level: position.level,
            }]
        } else {
            actor.footprint.at(position, actor.facing)
        };
        for rect in rects {
            if rect.min_x < 0
                || rect.min_y < 0
                || rect.max_x > i32::from(lot.width()) * 16
                || rect.max_y > i32::from(lot.height()) * 16
            {
                return false;
            }
            for tile in rect.tiles() {
                let Some(data) = lot.tile(tile) else {
                    return false;
                };
                let (x, y) = (i32::from(tile.x) * 16, i32::from(tile.y) * 16);
                for (edge, coordinate, min, max) in [
                    (Cardinal::West, x, rect.min_x, rect.max_x),
                    (Cardinal::East, x + 16, rect.min_x, rect.max_x),
                    (Cardinal::North, y, rect.min_y, rect.max_y),
                    (Cardinal::South, y + 16, rect.min_y, rect.max_y),
                ] {
                    if min < coordinate && max > coordinate && lot.edge_blocked(tile, edge) {
                        return false;
                    }
                }
                let (left, right, top, bottom) = (
                    rect.min_x.max(x),
                    rect.max_x.min(x + 16),
                    rect.min_y.max(y),
                    rect.max_y.min(y + 16),
                );
                match data.wall.diagonal {
                    Diagonal::Vertical => {
                        let low = left - bottom - x + y;
                        let high = right - top - x + y;
                        if low < 0 && high > 0 {
                            return false;
                        }
                    }
                    Diagonal::Horizontal => {
                        let low = left + top - x - y - 16;
                        let high = right + bottom - x - y - 16;
                        if low < 0 && high > 0 {
                            return false;
                        }
                    }
                    Diagonal::None => {}
                }
                for (sx, sy) in [
                    (left, top),
                    (right - 1, top),
                    (left, bottom - 1),
                    (right - 1, bottom - 1),
                ] {
                    if validate_floor(
                        &actor.rules,
                        data.floor_in_half(data.wall.diagonal.half_at(sx, sy)),
                        tile.level,
                    )
                    .is_some()
                    {
                        return false;
                    }
                }
            }
        }
        true
    })
}

/// Bounded integer wall line-of-sight for SLOT goal verification. Footprint and
/// floor checks are separate; this does not claim source floating raycast parity.
pub fn wall_line_clear<Q: WorldQuery + ?Sized>(
    from: LotPosition,
    to: LotPosition,
    query: &Q,
) -> bool {
    from.level == to.level
        && query.lot().contains_position(from)
        && query.lot().contains_position(to)
        && !crosses_wall_centerline(from, to, query)
}

fn crosses_wall_centerline<Q: WorldQuery + ?Sized>(
    from: LotPosition,
    to: LotPosition,
    query: &Q,
) -> bool {
    if from == to {
        return false;
    }
    let from_point = (i64::from(from.x), i64::from(from.y));
    let to_point = (i64::from(to.x), i64::from(to.y));
    for y in from.y.min(to.y).div_euclid(16)..=from.y.max(to.y).div_euclid(16) {
        for x in from.x.min(to.x).div_euclid(16)..=from.x.max(to.x).div_euclid(16) {
            let tile = super::tiles::TilePos::new(x as i16, y as i16, from.level);
            let Some(data) = query.lot().tile(tile) else {
                continue;
            };
            let (x, y) = (i64::from(x) * 16, i64::from(y) * 16);
            let mut lines = Vec::new();
            for (edge, a, b) in [
                (Cardinal::West, (x, y), (x, y + 16)),
                (Cardinal::North, (x, y), (x + 16, y)),
                (Cardinal::East, (x + 16, y), (x + 16, y + 16)),
                (Cardinal::South, (x, y + 16), (x + 16, y + 16)),
            ] {
                if query.lot().edge_blocked(tile, edge) {
                    lines.push((a, b));
                }
            }
            match data.wall.diagonal {
                Diagonal::Vertical => lines.push(((x, y), (x + 16, y + 16))),
                Diagonal::Horizontal => lines.push(((x, y + 16), (x + 16, y))),
                Diagonal::None => {}
            }
            if lines
                .into_iter()
                .any(|(a, b)| segments_intersect(from_point, to_point, a, b))
            {
                return true;
            }
        }
    }
    false
}
fn segments_intersect(a: (i64, i64), b: (i64, i64), c: (i64, i64), d: (i64, i64)) -> bool {
    fn orientation(a: (i64, i64), b: (i64, i64), c: (i64, i64)) -> i64 {
        (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
    }
    fn on(a: (i64, i64), b: (i64, i64), p: (i64, i64)) -> bool {
        p.0 >= a.0.min(b.0) && p.0 <= a.0.max(b.0) && p.1 >= a.1.min(b.1) && p.1 <= a.1.max(b.1)
    }
    let (o1, o2, o3, o4) = (
        orientation(a, b, c),
        orientation(a, b, d),
        orientation(c, d, a),
        orientation(c, d, b),
    );
    (o1.signum() != o2.signum() && o3.signum() != o4.signum())
        || (o1 == 0 && on(a, b, c))
        || (o2 == 0 && on(a, b, d))
        || (o3 == 0 && on(c, d, a))
        || (o4 == 0 && on(c, d, b))
}

fn movement_blocker<Q: WorldQuery + ?Sized>(
    actor: &WorldObject,
    from: LotPosition,
    to: LotPosition,
    query: &Q,
    avoided: &BTreeSet<EntityRef>,
    planning: bool,
    ignore: Option<EntityRef>,
) -> Option<EntityRef> {
    let mut others = query.objects();
    others.sort_by_key(|o| o.entity);
    let samples = sample_line(from, to);
    for other in others {
        if other.entity == actor.entity
            || Some(other.entity) == ignore
            || other.rules.zero_extent
            || actor.rules.ignored.contains(&other.entity)
            || (actor.multitile_group.is_some() && actor.multitile_group == other.multitile_group)
        {
            continue;
        }
        if other.rules.is_avatar {
            if actor.rules.allow_person_intersection && !actor.rules.disallow_person_intersection {
                continue;
            }
            if planning && !avoided.contains(&other.entity) {
                continue;
            }
        } else if actor.rules.is_avatar
            && other.rules.allow_person_intersection
            && !other.rules.disallow_person_intersection
        {
            continue;
        }
        for position in &samples {
            if actor
                .footprint
                .at(*position, actor.facing)
                .iter()
                .any(|a| other.rects().iter().any(|b| a.intersects(*b)))
            {
                return Some(other.entity);
            }
        }
    }
    None
}

#[cfg(test)]
mod restore_regressions {
    use super::*;
    use crate::{
        ids::ObjectId,
        world::{LotModel, TilePos, WorldState},
    };

    fn entity(id: i16) -> EntityRef {
        EntityRef {
            object_id: ObjectId(id),
            generation: 1,
        }
    }
    fn fixture() -> WorldState {
        let mut world = WorldState::new(LotModel::new(8, 8, 1).unwrap());
        let mut actor = WorldObject::new(entity(1), TilePos::new(1, 1, 1).center());
        actor.rules.is_avatar = true;
        world.insert_object(actor).unwrap();
        let mut seat = WorldObject::new(entity(2), TilePos::new(2, 2, 1).center());
        seat.entrypoints.insert(27);
        seat.rules.zero_extent = true;
        world.insert_object(seat).unwrap();
        world
    }
    fn request(id: u64, world: &WorldState) -> RouteRequest {
        RouteRequest::new(
            id,
            entity(1),
            world.object(entity(1)).unwrap().position,
            vec![RouteGoal::point(TilePos::new(6, 6, 1).center())],
        )
    }

    #[test]
    fn restored_search_sequence_cannot_overflow_on_next_insert() {
        let world = fixture();
        let mut route = RouteContinuation::new(request(900, &world), &world).unwrap();
        route.step(&world, &mut SimRng::new(10), 0);
        route.validate_against(&world).unwrap();
        route.search.as_mut().unwrap().sequence = u64::MAX;
        let decoded: RouteContinuation =
            bincode::deserialize(&bincode::serialize(&route).unwrap()).unwrap();
        assert_eq!(
            decoded.validate_against(&world),
            Err(RouteError::InvalidState)
        );
        let mut search = GridSearch::new(route.position(), route.request.goals[0].position);
        search.sequence = u64::MAX;
        assert!(!search.insert(route.position(), 1, 1));
    }

    #[test]
    fn nested_route_ids_are_unique_across_every_ancestor_and_restore() {
        let world = fixture();
        let mut r = request(900, &world);
        r.seated_on = Some(entity(2));
        let mut root = RouteContinuation::new(r, &world).unwrap();
        let mut rng = SimRng::new(10);
        assert!(matches!(
            root.step(&world, &mut rng, 1),
            RouteStep::Script(_)
        ));
        let mut r = request(901, &world);
        r.seated_on = Some(entity(2));
        root.push_nested(r, &world).unwrap();
        assert!(matches!(
            root.step(&world, &mut rng, 1),
            RouteStep::Script(_)
        ));
        let before = root.clone();
        assert_eq!(
            root.push_nested(request(900, &world), &world),
            Err(RouteError::InvalidRequest)
        );
        assert_eq!(root, before);
        root.push_nested(request(902, &world), &world).unwrap();
        root.validate_against(&world).unwrap();
        root.nested[0].nested[0].request.id = 900;
        let decoded: RouteContinuation =
            bincode::deserialize(&bincode::serialize(&root).unwrap()).unwrap();
        assert_eq!(
            decoded.validate_against(&world),
            Err(RouteError::InvalidState)
        );
    }
}
