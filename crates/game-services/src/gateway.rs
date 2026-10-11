//! Browser/native gateway wire DTOs. These are gateway paths, not invented legacy routes.
use crate::{DecimalU64, DirectoryResult, RosterEntry, ServiceError, Shard};
use serde::{Deserialize, Serialize};

pub const GATEWAY_PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Authenticated,
    CityConnecting,
    CityReady,
    LotConnecting,
    LotReady,
    Disconnected,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityStatus {
    pub capability: String,
    pub available: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionProjection {
    pub epoch: u64,
    pub state: SessionState,
    pub avatar_id: Option<u32>,
    pub shard_name: Option<String>,
    /// Packed city location used by original FindLotRequest (not a lot database ID).
    pub lot_location: Option<u32>,
    pub lot_incarnation: Option<u64>,
    pub capabilities: Vec<CapabilityStatus>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayHealth {
    pub protocol_version: u32,
    pub configured: bool,
    pub capabilities: Vec<CapabilityStatus>,
}

// Credentials intentionally have no Debug implementation.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize, Deserialize)]
pub struct SessionCreated {
    pub session_token: String,
    pub session: SessionProjection,
    pub roster: Vec<RosterEntry>,
    pub shards: Vec<Shard>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum BrowserMessage {
    Authenticate {
        session_token: String,
    },
    Request {
        epoch: u64,
        operation_id: String,
        operation: GatewayOperation,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum GatewayOperation {
    RefreshRoster,
    /// avatar_id=0 is the original anonymous CAS city connection.
    ConnectCity {
        shard_name: String,
        avatar_id: u32,
    },
    DisconnectCity,
    JoinLot {
        lot_location: u32,
        #[serde(default)]
        open_if_closed: bool,
    },
    LeaveLot,
    CreateAvatar {
        name: String,
        description: String,
        gender: Gender,
        skin: SkinTone,
        head_key: DecimalU64,
        body_key: DecimalU64,
    },
    RetireAvatar,
    FindAvatar {
        avatar_id: u32,
    },
    PrivateMessage {
        target_avatar_id: u32,
        message: String,
        #[serde(default)]
        color: u32,
    },
    MailPoll {
        since_ticks: DecimalU64,
    },
    MailSend {
        target_avatar_id: u32,
        subject: String,
        body: String,
    },
    MailDelete {
        message_id: i32,
    },
    PurchaseLot {
        x: u16,
        y: u16,
        name: String,
        start_fresh: bool,
        #[serde(default)]
        mayor_mode: bool,
    },
    Roommate {
        action: RoommateAction,
        avatar_id: u32,
        lot_location: u32,
    },
    Neighborhood {
        action: NeighborhoodAction,
        target_avatar_id: u32,
        neighborhood_id: u32,
        #[serde(default)]
        message: String,
        #[serde(default)]
        value: u32,
    },
    Bulletin {
        action: BulletinAction,
        neighborhood_id: u32,
        #[serde(default)]
        title: String,
        #[serde(default)]
        message: String,
        #[serde(default)]
        lot_id: u32,
        #[serde(default)]
        value: u32,
    },
    LotChat {
        message: String,
    },
    /// A native VM/EOD provider must establish the active actor/plugin before this can run.
    Eod {
        incarnation: u64,
        plugin_id: u32,
        event_name: String,
        text: Option<String>,
        binary: Option<Vec<u8>>,
    },
    /// Complete original VMNetCommand, admitted by the native source-command validator.
    LotCommand {
        lot_incarnation: u64,
        data: Vec<u8>,
    },
    RequestWorldSnapshot {
        lot_incarnation: u64,
    },
    CancelInteraction {
        lot_incarnation: u64,
        action_uid: u16,
    },
    /// Interaction and Param0 must come from the actual source GotoObject menu.
    WalkTo {
        lot_incarnation: u64,
        interaction: u16,
        param0: i16,
        x: i16,
        y: i16,
        level: i8,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Gender {
    Male,
    Female,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkinTone {
    Light,
    Medium,
    Dark,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoommateAction {
    Invite,
    Kick,
    Accept,
    Decline,
    Poll,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NeighborhoodAction {
    Vote,
    CanVote,
    Nominate,
    CanNominate,
    Rate,
    CanRate,
    NominationRun,
    CanRun,
    CanFreeVote,
    FreeVote,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BulletinAction {
    GetMessages,
    PostMessage,
    PromoteMessage,
    CanPostMessage,
    DeleteMessage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeStatus {
    Accepted,
    Rejected,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GatewayEnvelope {
    pub epoch: u64,
    pub operation_id: Option<String>,
    pub event: GatewayEvent,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GatewayEvent {
    Session {
        session: SessionProjection,
    },
    Pending {
        family: String,
    },
    Outcome {
        status: OutcomeStatus,
        source_code: Option<u16>,
        data: serde_json::Value,
        error: Option<ServiceError>,
    },
    Roster {
        roster: Vec<RosterEntry>,
    },
    Directory {
        result: DirectoryResult,
    },
    /// Incoming source events without a pending operation (messages, mail, invitations).
    SourceEvent {
        family: String,
        source_code: Option<u16>,
        data: serde_json::Value,
    },
    /// Bounded original VM payload for an explicit decoder/runtime adapter.
    VmFrame {
        lot_incarnation: u64,
        direct: bool,
        data: Vec<u8>,
    },
    Error {
        error: ServiceError,
    },
}
