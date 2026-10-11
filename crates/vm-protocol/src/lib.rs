//! Exact, bounded FreeSO 4c6b3e8 VM wire decoding; refresh-only snapshots.
#![forbid(unsafe_code)]
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Truncated,
    Invalid,
    Limit,
    UnsupportedCommand(u8),
    UnsupportedVersion(i32),
    Trailing,
    Compression,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub offset: usize,
    pub kind: ErrorKind,
    pub context: &'static str,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} at {}: {}", self.kind, self.offset, self.context)
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Debug)]
pub struct DecodeLimits {
    pub max_input_bytes: usize,
    pub max_decompressed_bytes: usize,
    pub max_total_entries: usize,
    pub max_count: usize,
    pub max_string_bytes: usize,
    pub max_total_string_bytes: usize,
    pub max_ticks: usize,
    pub max_commands: usize,
    pub max_entities: usize,
    pub max_tiles: usize,
}
impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 64 * 1024 * 1024,
            max_decompressed_bytes: 64 * 1024 * 1024,
            max_total_entries: 1_000_000,
            max_count: 65536,
            max_string_bytes: 64 * 1024,
            max_total_string_bytes: 4 * 1024 * 1024,
            max_ticks: 4096,
            max_commands: 65536,
            max_entities: 32767,
            max_tiles: 512 * 512 * 5,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EodPayload {
    Text(String),
    Binary(Vec<u8>),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EodMessage {
    pub actor_uid: u32,
    pub plugin_id: u32,
    pub event_name: String,
    pub payload: EodPayload,
}
#[derive(Clone, Debug, Serialize)]
pub struct TickList {
    pub immediate_mode: bool,
    pub ticks: Vec<Tick>,
    pub consumed: usize,
}
#[derive(Clone, Debug, Serialize)]
pub struct Tick {
    pub tick_id: u32,
    #[serde(serialize_with = "decimal_u64")]
    pub random_seed: u64,
    pub commands: Vec<Command>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Command {
    pub kind: u8,
    pub actor_uid: Option<u32>,
    pub offset: usize,
    pub consumed: usize,
    pub body: CommandBody,
}
#[derive(Clone, Debug, Serialize)]
pub enum CommandBody {
    /// VMNetChatCmd; the high channel bit means a private direct delivery.
    Chat {
        message: String,
        channel_id: u8,
    },
    AvatarJoin(AvatarJoin),
    ChangePermissions {
        target_uid: u32,
        replace_uid: u32,
        level: u8,
        mode: u8,
    },
    SetIgnore {
        target_uid: u32,
        ignore: bool,
    },
    ChatParameters {
        pitch: i8,
        color: u32,
    },
    ChatEditChannel(snapshot::ChatChannel),
    EodMessage(EodMessage),
    StateSync {
        snapshot: Box<Snapshot>,
        traces: Option<Vec<SyncTraceTick>>,
    },
    SourceFields {
        bytes: Vec<u8>,
    },
}
/// Fields needed to identify a source Sim join without executing its VM effects.
#[derive(Clone, Debug, Serialize)]
pub struct AvatarJoin {
    pub name: String,
    pub persist_id: u32,
    pub permissions: u8,
    pub ignored: Vec<u32>,
}
#[derive(Clone, Debug, Serialize)]
pub struct SyncTraceTick {
    pub tick_id: u32,
    pub trace: Vec<String>,
}
fn decimal_u64<S: serde::Serializer>(n: &u64, s: S) -> std::result::Result<S::Ok, S::Error> {
    s.serialize_str(&n.to_string())
}
pub mod chat;
mod command;
mod reader;
pub fn decode_tick_list(bytes: &[u8], limits: &DecodeLimits) -> Result<TickList> {
    let mut reader = reader::Reader::new(bytes, limits)?;
    let immediate_mode = reader.boolean()?;
    let n = reader.count_max(limits.max_ticks, 16)?; // ID, seed, command count
    let mut ticks = Vec::with_capacity(n);
    let mut command_total = 0usize;
    for _ in 0..n {
        let tick_id = reader.u32()?;
        let random_seed = reader.u64()?;
        let count = reader.count_max(limits.max_commands, 5)?; // enum + smallest body
        command_total = command_total
            .checked_add(count)
            .filter(|n| *n <= limits.max_commands)
            .ok_or_else(|| reader.error(ErrorKind::Limit, "total tick commands"))?;
        let mut commands = Vec::with_capacity(count);
        for _ in 0..count {
            commands.push(command::decode(&mut reader)?);
        }
        ticks.push(Tick {
            tick_id,
            random_seed,
            commands,
        });
    }
    reader.finish()?;
    Ok(TickList {
        immediate_mode,
        ticks,
        consumed: reader.at,
    })
}
pub fn decode_direct_command(bytes: &[u8], limits: &DecodeLimits) -> Result<Command> {
    let mut reader = reader::Reader::new(bytes, limits)?;
    let command = command::decode(&mut reader)?;
    reader.finish()?;
    Ok(command)
}
fn decimal_i64<S: serde::Serializer>(n: &i64, s: S) -> std::result::Result<S::Ok, S::Error> {
    s.serialize_str(&n.to_string())
}
pub mod snapshot;
pub use snapshot::{decode_snapshot, Snapshot};
