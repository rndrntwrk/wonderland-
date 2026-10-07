//! Browser player boundary. Admission/authentication remains server-owned.
//! Bootstrap is native executable content, never an FSOv presentation snapshot.
use super::{CheckpointRequest, CompareWriter, NativeWire, Received, WireLimits, codec, guard};
use crate::live_session::{CancelSelection, InteractionSelection, LiveReplica, StreamIdentity};
use crate::sim_core::{
    effects::EffectLimits,
    interactions::InteractionKey,
    state::{ContentSet, SimulationLimits},
};
use crate::world_view::WorldDocument;
use crate::{
    ActionId, CancelIntent, EntityRef, GameRuntime, InteractionIntent, LotModel, OfferBatch,
    PrincipalKey, QueryOptions, RuntimeConfig, RuntimeProjection, RuntimeRole, VmMode,
};
use bincode::Options;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub const MAX_BOOTSTRAP_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_ACTION_BYTES: usize = 16 * 1024;
const BOOT: &[u8; 8] = b"WLB1\r\n\x1a\n";
const ACTION: &[u8; 8] = b"WLC1\r\n\x1a\n";
const RECEIPT: &[u8; 8] = b"WLA1\r\n\x1a\n";
pub type Result<T> = std::result::Result<T, &'static str>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerBinding {
    pub source_epoch: u64,
    pub lot_incarnation: u64,
    pub lot_location: u32,
    pub avatar_id: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bootstrap {
    pub binding: PlayerBinding,
    pub principal: PrincipalKey,
    pub actor: EntityRef,
    pub content: ContentSet,
    pub appearance: WorldDocument,
    pub lot: LotModel,
    pub mode: VmMode,
    pub lot_id: u64,
    pub authority_epoch: u64,
    pub effect_namespace: u64,
    pub limits: SimulationLimits,
    pub effect_limits: EffectLimits,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PlayerAction {
    Invoke(InteractionIntent),
    Cancel(CancelIntent),
}
impl PlayerAction {
    pub fn sequence(&self) -> u64 {
        match self {
            Self::Invoke(v) => v.command_sequence,
            Self::Cancel(v) => v.command_sequence,
        }
    }
}
/// Sent only after server admission/rejection. An accepted result must follow its
/// accepted tick on the same ordered stream. Echo binds the result to exact bytes,
/// not merely a sequence another client may have used for the same actor.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActionReceipt {
    pub request: Vec<u8>,
    pub accepted_tick: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionStatus {
    Idle,
    Pending,
    Accepted,
    Rejected,
    Unknown,
}

fn encode<T: Serialize>(magic: &[u8; 8], value: &T, maximum: usize) -> Result<Vec<u8>> {
    let size = usize::try_from(
        codec(maximum)
            .serialized_size(value)
            .map_err(|_| "Native packet is too large")?,
    )
    .map_err(|_| "Native packet is too large")?;
    if size > maximum.saturating_sub(16) {
        return Err("Native packet is too large");
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size + 16)
        .map_err(|_| "Native packet allocation failed")?;
    bytes.extend_from_slice(magic);
    bytes.extend_from_slice(&(size as u64).to_le_bytes());
    codec(size)
        .serialize_into(&mut bytes, value)
        .map_err(|_| "Native packet could not be encoded")?;
    Ok(bytes)
}
fn decode<T: DeserializeOwned + Serialize>(
    magic: &[u8; 8],
    bytes: &[u8],
    maximum: usize,
) -> Result<T> {
    if bytes.len() < 16 || bytes.len() > maximum || &bytes[..8] != magic {
        return Err("Invalid native packet");
    }
    let length = u64::from_le_bytes(
        bytes[8..16]
            .try_into()
            .map_err(|_| "Invalid native length")?,
    );
    if length != (bytes.len() - 16) as u64 {
        return Err("Invalid native length");
    }
    let raw = &bytes[16..];
    let budget = guard::Budget::new(1_048_576, 128 * 1024 * 1024, 128);
    let value: T = codec(raw.len())
        .deserialize_seed(guard::ValueSeed::new(&budget), raw)
        .map_err(|_| "Invalid native payload")?;
    let mut comparison = CompareWriter { remaining: raw };
    codec(raw.len())
        .serialize_into(&mut comparison, &value)
        .map_err(|_| "Noncanonical native payload")?;
    if !comparison.remaining.is_empty() {
        return Err("Trailing native payload");
    }
    Ok(value)
}
pub fn encode_bootstrap(value: &Bootstrap) -> Result<Vec<u8>> {
    encode(BOOT, value, MAX_BOOTSTRAP_BYTES)
}
pub fn encode_action(value: &PlayerAction) -> Result<Vec<u8>> {
    encode(ACTION, value, MAX_ACTION_BYTES)
}
pub fn decode_action(bytes: &[u8]) -> Result<PlayerAction> {
    let action: PlayerAction = decode(ACTION, bytes, MAX_ACTION_BYTES)?;
    if action.sequence() == 0 {
        return Err("Invalid action sequence");
    }
    Ok(action)
}
pub fn encode_receipt(value: &ActionReceipt) -> Result<Vec<u8>> {
    decode_action(&value.request)?;
    encode(RECEIPT, value, MAX_ACTION_BYTES + 64)
}

/// A validated input result. Live tick outcomes are owned and ordered; a recovery
/// checkpoint never carries historical effects. Receipt acceptance is not action completion.
#[derive(Debug, PartialEq)]
pub enum PlayerUpdate {
    Checkpoint(crate::live_session::ReplicaCursor),
    Ticks(Vec<crate::TickOutcome>),
    Receipt(ActionStatus),
}

pub struct NativePlayer {
    wire: NativeWire,
    appearance: WorldDocument,
    binding: PlayerBinding,
    actor: EntityRef,
    principal: PrincipalKey,
    sequence: u64,
    pending_after: u64,
    pending: Option<Vec<u8>>,
    status: ActionStatus,
    activity: activity::ActivityLog,
}
impl NativePlayer {
    pub fn open(bytes: &[u8], expected: PlayerBinding, browser_epoch: u64) -> Result<Self> {
        let value: Bootstrap = decode(BOOT, bytes, MAX_BOOTSTRAP_BYTES)?;
        if value.binding != expected
            || expected.source_epoch == 0
            || expected.lot_incarnation == 0
            || expected.avatar_id == 0
            || expected.lot_location == 0
        {
            return Err("Native admission does not match the selected Sim and lot");
        }
        value
            .content
            .validate()
            .map_err(|_| "Invalid executable content")?;
        value
            .appearance
            .validate()
            .map_err(|_| "Invalid native appearance")?;
        if value.appearance.lot.width != value.lot.width()
            || value.appearance.lot.height != value.lot.height()
        {
            return Err("Native appearance has different lot dimensions");
        }
        let mut config = RuntimeConfig::new(value.mode, value.lot_id, value.authority_epoch, 0);
        config.effect_namespace = value.effect_namespace;
        config.limits = value.limits;
        config.effect_limits = value.effect_limits;
        let runtime = GameRuntime::new(value.content, value.lot, config, RuntimeRole::Replica)
            .map_err(|_| "Native runtime could not be admitted")?;
        let limits = WireLimits::default();
        let replica = LiveReplica::new(
            runtime,
            StreamIdentity {
                browser_epoch,
                source_epoch: expected.source_epoch,
                lot_incarnation: expected.lot_incarnation,
            },
            limits.replay,
        )
        .map_err(|_| "Invalid native session")?;
        Ok(Self {
            wire: NativeWire::new(replica, limits).map_err(|_| "Invalid native stream")?,
            appearance: value.appearance,
            binding: expected,
            actor: value.actor,
            principal: value.principal,
            sequence: 0,
            pending_after: 0,
            pending: None,
            status: ActionStatus::Idle,
            activity: activity::ActivityLog::new(value.actor),
        })
    }
    pub fn checkpoint_request(&self) -> Option<CheckpointRequest> {
        self.wire.checkpoint_request()
    }
    pub fn status(&self) -> ActionStatus {
        self.status
    }
    /// Recent terminal outcomes for the admitted actor only. Kept across matching
    /// reconnects, never rebuilt from historical recovery ticks or receipts.
    pub fn activity(&self) -> impl DoubleEndedIterator<Item = &activity::ActionActivity> {
        self.activity.entries()
    }
    pub fn actor(&self) -> EntityRef {
        self.actor
    }
    fn validate_actor(&self) -> Result<()> {
        if self.wire.replica().status() != crate::live_session::SessionStatus::Live {
            return Err("Native world is not live");
        }
        let state = self
            .wire
            .replica()
            .runtime()
            .ok_or("Native session is closed")?
            .sim()
            .state();
        let entity = state
            .entities
            .get(&self.actor.object_id)
            .ok_or("Admitted Sim is not in this world")?;
        if entity.info.reference != self.actor
            || entity.info.persistent_id != self.binding.avatar_id
            || !entity.info.is_avatar
            || entity.info.dead
        {
            return Err("Admitted Sim identity has changed");
        }
        Ok(())
    }
    pub fn projection(&self) -> Result<RuntimeProjection> {
        self.validate_actor()?;
        self.wire
            .replica()
            .projection()
            .map_err(|_| "Native world is not live")
    }
    pub fn world(&self) -> Result<WorldDocument> {
        self.validate_actor()?;
        self.wire
            .replica()
            .runtime()
            .ok_or("Native session is closed")?
            .world_document(&self.appearance)
            .map_err(|_| "Native scene is invalid")
    }
    /// Read-only appearance/animation fields from the same accepted tick as `world`.
    /// Fails closed while disconnected or recovering, like other live projections.
    pub fn avatar_visual_frame(&self) -> Result<crate::AvatarVisualFrame> {
        self.validate_actor()?;
        Ok(self
            .wire
            .replica()
            .runtime()
            .ok_or("Native session is closed")?
            .avatar_visual_frame())
    }
    pub fn offers(&self, target: EntityRef) -> Result<OfferBatch> {
        self.validate_actor()?;
        self.wire
            .replica()
            .offers(
                self.wire.connection(),
                self.principal,
                self.actor,
                target,
                QueryOptions::default(),
            )
            .map_err(|_| "Source actions are unavailable")
    }
    /// Compatibility state-only entry point. Presentation consumers must use
    /// `receive_update` so accepted events are not lost between transport and UI.
    pub fn receive(&mut self, bytes: &[u8]) -> Result<()> {
        self.receive_update(bytes).map(|_| ())
    }
    /// Return outcomes only after the complete message and admitted actor validate.
    /// Consume each returned vector once; there is no last-value event mailbox.
    pub fn receive_update(&mut self, bytes: &[u8]) -> Result<PlayerUpdate> {
        if bytes.starts_with(RECEIPT) {
            self.validate_actor()?;
            let receipt: ActionReceipt = decode(RECEIPT, bytes, MAX_ACTION_BYTES + 64)?;
            if self.pending.as_deref() != Some(receipt.request.as_slice()) {
                return Err("Uncorrelated action result");
            }
            if receipt.accepted_tick.is_some_and(|tick| {
                tick <= self.pending_after
                    || self
                        .wire
                        .replica()
                        .cursor()
                        .is_none_or(|cursor| cursor.completed_tick < tick)
            }) {
                return Err("Action result precedes its accepted state");
            }
            self.pending = None;
            self.status = if receipt.accepted_tick.is_some() {
                ActionStatus::Accepted
            } else {
                ActionStatus::Rejected
            };
            return Ok(PlayerUpdate::Receipt(self.status));
        }
        let result = self.wire.receive(self.wire.connection(), bytes);
        match result {
            Ok(update) => {
                if self.validate_actor().is_err() {
                    self.close();
                    return Err("Native actor no longer matches admission");
                }
                Ok(match update {
                    Received::Checkpoint(cursor) => PlayerUpdate::Checkpoint(cursor),
                    Received::Ticks(outcomes) => {
                        self.activity.observe(&outcomes);
                        PlayerUpdate::Ticks(outcomes)
                    }
                })
            }
            Err(_) => Err("Native stream requires recovery"),
        }
    }
    fn next_sequence(&self) -> Result<u64> {
        if self.pending.is_some() {
            return Err("Wait for the previous action result");
        }
        let accepted = self
            .wire
            .replica()
            .runtime()
            .ok_or("Native session is closed")?
            .sim()
            .state()
            .interaction_queues
            .get(&self.actor.object_id)
            .and_then(|q| q.last_command_sequence())
            .unwrap_or(0);
        self.sequence
            .max(accepted)
            .checked_add(1)
            .ok_or("Action sequence exhausted")
    }
    pub fn prepare(
        &mut self,
        target: EntityRef,
        key: InteractionKey,
        param0: i16,
    ) -> Result<Vec<u8>> {
        self.validate_actor()?;
        let sequence = self.next_sequence()?;
        let intent = self
            .wire
            .replica()
            .prepare_interaction(
                self.wire.connection(),
                InteractionSelection {
                    principal: self.principal,
                    actor: self.actor,
                    target,
                    interaction: key,
                    param0,
                    command_sequence: sequence,
                },
            )
            .map_err(|_| "This source action is no longer available")?;
        let bytes = encode_action(&PlayerAction::Invoke(intent))?;
        self.pending_after = self
            .wire
            .replica()
            .cursor()
            .ok_or("Native cursor is unavailable")?
            .completed_tick;
        self.sequence = sequence;
        self.pending = Some(bytes.clone());
        self.status = ActionStatus::Pending;
        Ok(bytes)
    }
    pub fn prepare_cancel(&mut self, action: u64) -> Result<Vec<u8>> {
        self.validate_actor()?;
        let sequence = self.next_sequence()?;
        let intent = self
            .wire
            .replica()
            .prepare_cancel(
                self.wire.connection(),
                CancelSelection {
                    principal: self.principal,
                    actor: self.actor,
                    action: ActionId(action),
                    command_sequence: sequence,
                },
            )
            .map_err(|_| "This action cannot be cancelled")?;
        let bytes = encode_action(&PlayerAction::Cancel(intent))?;
        self.pending_after = self
            .wire
            .replica()
            .cursor()
            .ok_or("Native cursor is unavailable")?
            .completed_tick;
        self.sequence = sequence;
        self.pending = Some(bytes.clone());
        self.status = ActionStatus::Pending;
        Ok(bytes)
    }
    pub fn disconnect(&mut self) {
        if self.pending.is_some() {
            self.status = ActionStatus::Unknown;
        }
        let _ = self.wire.disconnect(self.wire.connection());
    }
    pub fn reconnect(&mut self) -> Result<()> {
        self.wire
            .reconnect()
            .map_err(|_| "Native reconnect failed")?;
        Ok(())
    }
    /// Explicitly abandon a pending result after recovery, never replay it. The
    /// sequence high-water stays intact; only a separately chosen action may follow.
    pub fn dismiss_unknown(&mut self) -> Result<()> {
        self.validate_actor()?;
        if self.status != ActionStatus::Unknown {
            return Err("No unknown result");
        }
        self.pending = None;
        self.status = ActionStatus::Idle;
        Ok(())
    }
    pub fn close(&mut self) {
        self.wire.close();
        self.activity.clear();
        self.pending = None;
        self.status = ActionStatus::Unknown;
    }
}

pub mod activity;
pub mod admission;
