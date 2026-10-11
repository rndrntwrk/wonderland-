//! FSOv latest version 38 wire DTO. This is refresh-only source projection,
//! never an A interpreter restore or claim of identical RNG consumption.
use crate::{DecodeLimits, Result};
use serde::Serialize;
#[derive(Clone, Debug, Serialize)]
pub enum RestoreSemantics {
    RefreshOnly,
}
#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub version: i32,
    pub compressed: bool,
    pub semantics: RestoreSemantics,
    pub context: Context,
    pub entities: Vec<Entity>,
    pub threads: Vec<Thread>,
    pub multitile_groups: Vec<MultitileGroup>,
    pub global_state: Vec<i16>,
    pub platform: LotState,
    pub next_object_id: i16,
    pub tuning: Option<Tuning>,
    pub consumed: usize,
    #[serde(skip)]
    pub source_body: Vec<u8>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Clock {
    #[serde(serialize_with = "crate::decimal_i64")]
    pub ticks: i64,
    pub minute_fractions: i32,
    pub ticks_per_minute: i32,
    pub minutes: i32,
    pub hours: i32,
    pub day: i32,
    pub month: i32,
    pub year: i32,
    pub fire_percent: i32,
    #[serde(serialize_with = "crate::decimal_i64")]
    pub utc_start: i64,
}
#[derive(Clone, Debug, Serialize)]
pub struct Context {
    pub clock: Clock,
    pub architecture: Architecture,
    #[serde(serialize_with = "crate::decimal_u64")]
    pub ambience_bits: u64,
    #[serde(serialize_with = "crate::decimal_u64")]
    pub random_seed: u64,
}
#[derive(Clone, Debug, Serialize)]
pub struct Architecture {
    pub width: u16,
    pub height: u16,
    pub stories: u8,
    pub terrain_light: u8,
    pub terrain_dark: u8,
    pub heights: Vec<i16>,
    pub grass: Vec<u8>,
    pub walls: Vec<Vec<Wall>>,
    pub floors: Vec<Vec<u16>>,
    pub walls_dirty: bool,
    pub floors_dirty: bool,
    pub roof_style: u32,
    pub roof_pitch: f32,
    pub id_map: Option<ResourceMap>,
    pub fine_buildable: Option<Vec<bool>>,
    pub build_buy_enabled: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct Wall {
    pub segments: u8,
    pub patterns: [u16; 4],
    pub styles: [u16; 2],
}
#[derive(Clone, Debug, Serialize)]
pub struct ResourceMap {
    pub walls: Vec<(u16, String)>,
    pub floors: Vec<(u16, String)>,
    pub roof: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Position {
    pub x: i16,
    pub y: i16,
    pub level: i8,
}
#[derive(Clone, Debug, Serialize)]
pub enum EntityPlatform {
    Object {
        budget: u32,
        owner_id: u32,
        wear: u16,
        repair_quarters: u8,
        flags: u8,
        upgrade_level: u8,
    },
    Avatar {
        budget: u32,
        permissions: u8,
        ignored: Vec<u32>,
        jobs: Vec<(i16, [i16; 4])>,
        flags: u32,
        chat_rgb: [u8; 3],
        chat_pitch: i8,
        chat_channel: u8,
    },
}
#[derive(Clone, Debug, Serialize)]
pub struct Entity {
    pub object_id: i16,
    pub persist_id: u32,
    pub platform: EntityPlatform,
    pub object_data: Vec<i16>,
    pub my_list: Vec<i16>,
    pub headline: Option<Headline>,
    pub guid: u32,
    pub master_guid: u32,
    pub main_param: i16,
    pub main_stack_object: i16,
    pub contained: Vec<i16>,
    pub container: i16,
    pub container_slot: i16,
    pub attributes: Vec<i16>,
    pub object_relationships: Vec<(u16, Vec<i16>)>,
    pub persistent_relationships: Vec<(u32, Vec<i16>)>,
    #[serde(serialize_with = "crate::decimal_u64")]
    pub dynamic_flags: u64,
    #[serde(serialize_with = "crate::decimal_u64")]
    pub dynamic_flags2: u64,
    pub position: Position,
    pub timestamp_lockout: u32,
    pub light_color: u32,
    pub appearance: Appearance,
}
#[derive(Clone, Debug, Serialize)]
pub struct Headline {
    pub operand: [u8; 8],
    pub target: i16,
    pub icon_target: i16,
    pub index: i8,
    pub duration: i32,
    pub animation: i32,
}
#[derive(Clone, Debug, Serialize)]
pub enum Appearance {
    Object { direction: u8, disabled: u8 },
    Avatar(Box<Avatar>),
}
#[derive(Clone, Debug, Serialize)]
pub struct Outfit {
    #[serde(serialize_with = "crate::decimal_u64")]
    pub id: u64,
    pub name: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Avatar {
    pub animations: Vec<Animation>,
    pub carry_animation: Option<Animation>,
    pub message: String,
    pub message_timeout: i32,
    pub motive_changes: Vec<MotiveChange>,
    pub decay_last_minute: i32,
    pub decay_fractions: [i16; 7],
    pub person_data: Vec<i16>,
    pub motives: Vec<i16>,
    pub old_hand_object: i16,
    pub yaw: f32,
    pub kill_timeout: i32,
    pub default_suits: [Outfit; 3],
    #[serde(serialize_with = "u64s")]
    pub dynamic_suits: [u64; 4],
    #[serde(serialize_with = "u64s")]
    pub decoration: [u64; 4],
    pub bound_appearances: Vec<String>,
    pub body: Outfit,
    pub head: Outfit,
    pub skin_tone: u8,
}
fn u64s<S: serde::Serializer>(n: &[u64; 4], s: S) -> std::result::Result<S::Ok, S::Error> {
    use serde::ser::SerializeSeq;
    let mut seq = s.serialize_seq(Some(4))?;
    for n in n {
        seq.serialize_element(&n.to_string())?;
    }
    seq.end()
}
#[derive(Clone, Debug, Serialize)]
pub struct Animation {
    pub name: String,
    pub frame: f32,
    pub event_queue: Vec<i16>,
    pub events_run: u8,
    pub end_reached: bool,
    pub backwards: bool,
    pub speed: f32,
    pub weight: f32,
    pub looped: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct MotiveChange {
    pub per_hour: i16,
    pub maximum: i16,
    pub motive: u8,
    pub fractional: f64,
}
#[derive(Clone, Debug, Serialize)]
pub struct MultitileGroup {
    pub multi_tile: bool,
    pub name: String,
    pub price: i32,
    pub sale_price: i32,
    pub objects: Vec<i16>,
    pub offsets: Vec<Position>,
}
#[derive(Clone, Debug, Serialize)]
pub struct LotState {
    pub name: String,
    pub lot_id: u32,
    pub surrounding_blends: Vec<[u8; 4]>,
    pub surrounding_roads: [u8; 9],
    pub surrounding_heights: [u8; 16],
    pub category: u8,
    pub size: i32,
    pub owner_id: u32,
    pub roommates: Vec<u32>,
    pub build_roommates: Vec<u32>,
    pub job_ui: Option<JobUi>,
    pub skill_mode: u8,
    pub chat_channels: Vec<ChatChannel>,
    pub neighborhood_id: u32,
}
#[derive(Clone, Debug, Serialize)]
pub struct JobUi {
    pub messages: Vec<String>,
    pub minutes: u8,
    pub seconds: u8,
    pub mode: u8,
}
#[derive(Clone, Debug, Serialize)]
pub struct ChatChannel {
    pub id: u8,
    pub name: String,
    pub description: String,
    pub minimum_view_permission: u8,
    pub minimum_send_permission: u8,
    pub flags: u8,
    pub text_color: u32,
}
#[derive(Clone, Debug, Serialize)]
pub struct Tuning {
    pub version: i32,
    pub types: Vec<(String, TuningTables)>,
}
pub type TuningEntries = Vec<(i32, f32)>;
pub type TuningTables = Vec<(i32, TuningEntries)>;
#[derive(Clone, Debug, Serialize)]
pub struct Thread {
    pub stack: Vec<StackFrame>,
    pub queue: Vec<QueuedAction>,
    pub active_queue_block: i8,
    pub temp_registers: [i16; 20],
    pub temp_xl: [i32; 2],
    pub last_exit_code: u8,
    pub blocking_state: Option<AsyncState>,
    pub eod_connection: Option<PluginThreadState>,
    pub interrupt: bool,
    pub action_uid: u16,
    pub dialog_cooldown: i32,
    pub schedule_idle_start: u32,
}
#[derive(Clone, Debug, Serialize)]
pub struct StackFrame {
    pub frame_type: u8,
    pub routine_id: u16,
    pub instruction_pointer: u16,
    pub caller: i16,
    pub callee: i16,
    pub stack_object: i16,
    pub code_owner_guid: u32,
    pub locals: Option<Vec<i16>>,
    pub args: Option<Vec<i16>>,
    pub special_result: u8,
    pub action_tree: bool,
    pub extension: Vec<u8>,
}
#[derive(Clone, Debug, Serialize)]
pub struct QueuedAction {
    pub routine_id: u16,
    pub check_routine_id: u16,
    pub callee: i16,
    pub stack_object: i16,
    pub icon_owner: i16,
    pub code_owner_guid: u32,
    pub name: Option<String>,
    pub args: Option<Vec<i16>>,
    pub interaction_number: i32,
    pub notify_idle: bool,
    pub priority: i16,
    pub mode: u8,
    pub flags: u32,
    pub flags2: u32,
    pub uid: u16,
    pub callback: Option<ActionCallback>,
    pub interaction_result: i8,
    pub result_check_counter: u16,
}
#[derive(Clone, Debug, Serialize)]
pub struct ActionCallback {
    pub kind: i32,
    pub target: i16,
    pub interaction: i16,
    pub set_param: bool,
    pub stack_object: i16,
    pub caller: i16,
    pub is_tree: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct AsyncState {
    pub kind: u8,
    pub responded: bool,
    pub wait_time: i32,
    pub source_fields: Vec<u8>,
}
#[derive(Clone, Debug, Serialize)]
pub struct PluginThreadState {
    pub responded: bool,
    pub wait_time: i32,
    pub avatar_id: i16,
    pub object_id: i16,
    pub joinable: bool,
    pub ended: bool,
    pub events: Vec<EodObjectEvent>,
}
#[derive(Clone, Debug, Serialize)]
pub struct EodObjectEvent {
    pub code: i16,
    pub data: Vec<i16>,
}
use crate::{reader::Reader, ErrorKind};
use std::io::Read;
pub fn decode_snapshot(bytes: &[u8], limits: &DecodeLimits) -> Result<Snapshot> {
    let mut r = Reader::new(bytes, limits)?;
    let s = read_snapshot(&mut r)?;
    r.finish()?;
    Ok(s)
}
pub(crate) fn read_snapshot(r: &mut Reader<'_, '_>) -> Result<Snapshot> {
    let start = r.at;
    if r.take(4)? != b"FSOv" {
        return Err(r.error(ErrorKind::Invalid, "FSOv magic"));
    }
    let version = r.i32()?;
    if version != 38 {
        return Err(r.error(
            ErrorKind::UnsupportedVersion(version),
            "FSOv source version",
        ));
    }
    let compressed = r.boolean()?;
    let mut snapshot = if compressed {
        let n = r.byte_length()?;
        let encoded = r.take(n)?;
        let mut gzip = flate2::bufread::GzDecoder::new(encoded);
        let mut decoded = Vec::new();
        gzip.by_ref()
            .take((r.decompress_remaining() as u64).saturating_add(1))
            .read_to_end(&mut decoded)
            .map_err(|_| r.error(ErrorKind::Compression, "FSOv GZIP member"))?;
        if decoded.len() > r.decompress_remaining() {
            return Err(r.error(ErrorKind::Limit, "FSOv GZIP expansion"));
        }
        r.decompress_charge(decoded.len())?;
        if !gzip.into_inner().is_empty() {
            return Err(r.error(ErrorKind::Trailing, "FSOv GZIP trailing member/bytes"));
        }
        let mut inner = r.fork(&decoded);
        let mut s = read_body(&mut inner)?;
        inner.finish()?;
        s.source_body = decoded;
        s
    } else {
        let body_start = r.at;
        let mut s = read_body(r)?;
        s.source_body = r.bytes[body_start..r.at].to_vec();
        s
    };
    snapshot.version = version;
    snapshot.compressed = compressed;
    snapshot.consumed = r.at - start;
    Ok(snapshot)
}
fn read_body(r: &mut Reader<'_, '_>) -> Result<Snapshot> {
    if r.boolean()? {
        return Err(r.error(
            ErrorKind::UnsupportedVersion(38),
            "TS1 platform snapshot unsupported",
        ));
    }
    let clock = Clock {
        ticks: r.i64()?,
        minute_fractions: r.i32()?,
        ticks_per_minute: r.i32()?,
        minutes: r.i32()?,
        hours: r.i32()?,
        day: r.i32()?,
        month: r.i32()?,
        year: r.i32()?,
        fire_percent: r.i32()?,
        utc_start: r.i64()?,
    };
    let architecture = architecture(r)?;
    let context = Context {
        clock,
        architecture,
        ambience_bits: r.u64()?,
        random_seed: r.u64()?,
    };
    let n = r.count_max(r.limits.max_entities, 92)?; // type + smallest v38 game object
    let mut entities = Vec::with_capacity(n);
    let mut ids = std::collections::BTreeSet::new();
    for _ in 0..n {
        let type_ = r.u8()?;
        if type_ > 1 {
            return Err(r.error(ErrorKind::Invalid, "VMEntity type"));
        }
        let entity = entity(r, type_ == 1)?;
        if entity.object_id <= 0 || !ids.insert(entity.object_id) {
            return Err(r.error(ErrorKind::Invalid, "VMEntity object identity"));
        }
        entities.push(entity);
    }
    let n = r.count_max(r.limits.max_entities, 71)?; // empty v38 thread
    if n != entities.len() {
        return Err(r.error(ErrorKind::Invalid, "VM thread/entity correspondence"));
    }
    let mut threads = Vec::with_capacity(n);
    for _ in 0..n {
        threads.push(thread(r)?);
    }
    let n = r.count(14)?; // group flags/name/prices/object count
    let mut multitile_groups = Vec::with_capacity(n);
    for _ in 0..n {
        let multi_tile = r.boolean()?;
        let name = r.text()?;
        let price = r.i32()?;
        let sale_price = r.i32()?;
        let objects = r.shorts()?;
        r.require_items(objects.len(), 5)?;
        let mut offsets = Vec::with_capacity(objects.len());
        for _ in 0..objects.len() {
            offsets.push(position(r)?);
        }
        multitile_groups.push(MultitileGroup {
            multi_tile,
            name,
            price,
            sale_price,
            objects,
            offsets,
        });
    }
    let global_state = r.shorts()?;
    let platform = lot_state(r)?;
    let next_object_id = r.i16()?;
    let tuning = if r.boolean()? { Some(tuning(r)?) } else { None };
    Ok(Snapshot {
        version: 38,
        compressed: false,
        semantics: RestoreSemantics::RefreshOnly,
        context,
        entities,
        threads,
        multitile_groups,
        global_state,
        platform,
        next_object_id,
        tuning,
        consumed: 0,
        source_body: vec![],
    })
}
fn architecture(r: &mut Reader<'_, '_>) -> Result<Architecture> {
    let width = r.i32()?;
    let height = r.i32()?;
    let stories = r.i32()?;
    if !(1..=65535).contains(&width)
        || !(1..=65535).contains(&height)
        || !(1..=255).contains(&stories)
    {
        return Err(r.error(ErrorKind::Invalid, "VM architecture dimensions"));
    }
    let cells = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| r.error(ErrorKind::Limit, "architecture cells"))?;
    let tiles = cells
        .checked_mul(stories as usize)
        .filter(|n| *n <= r.limits.max_tiles)
        .ok_or_else(|| r.error(ErrorKind::Limit, "architecture tiles"))?;
    r.reserve(tiles)?;
    let terrain_light = r.u8()?;
    let terrain_dark = r.u8()?;
    let expected_heights = cells
        .checked_mul(2)
        .ok_or_else(|| r.error(ErrorKind::Limit, "terrain height bytes"))?;
    let n = r.byte_length()?;
    if n != expected_heights {
        return Err(r.error(ErrorKind::Invalid, "source terrain height shape"));
    }
    let heights = r.short_values(cells)?;
    let n = r.byte_length()?;
    if n != cells {
        return Err(r.error(ErrorKind::Invalid, "source grass shape"));
    }
    let grass = r.take(n)?.to_vec();
    r.require_items(tiles, 15)?; // 13-byte wall and 2-byte floor per tile
    let mut walls = Vec::with_capacity(stories as usize);
    for _ in 0..stories {
        let mut level = Vec::with_capacity(cells);
        for _ in 0..cells {
            level.push(Wall {
                segments: r.u8()?,
                patterns: [r.u16()?, r.u16()?, r.u16()?, r.u16()?],
                styles: [r.u16()?, r.u16()?],
            });
        }
        walls.push(level);
    }
    let mut floors = Vec::with_capacity(stories as usize);
    for _ in 0..stories {
        let mut level = Vec::with_capacity(cells);
        for _ in 0..cells {
            level.push(r.u16()?);
        }
        floors.push(level);
    }
    let walls_dirty = r.boolean()?;
    let floors_dirty = r.boolean()?;
    let roof_style = r.u32()?;
    let roof_pitch = r.f32()?;
    let id_map = if r.boolean()? {
        let walln = r.count(3)?; // UInt16 ID and at least one string-length byte
        let mut wallnames = Vec::with_capacity(walln);
        for _ in 0..walln {
            wallnames.push((r.u16()?, r.text()?));
        }
        let floorn = r.count(3)?;
        let mut floornames = Vec::with_capacity(floorn);
        for _ in 0..floorn {
            floornames.push((r.u16()?, r.text()?));
        }
        Some(ResourceMap {
            walls: wallnames,
            floors: floornames,
            roof: r.text()?,
        })
    } else {
        None
    };
    let fine_buildable = if r.boolean()? {
        Some(r.take(cells)?.iter().map(|n| *n > 0).collect())
    } else {
        None
    };
    let build_buy_enabled = r.boolean()?;
    Ok(Architecture {
        width: width as u16,
        height: height as u16,
        stories: stories as u8,
        terrain_light,
        terrain_dark,
        heights,
        grass,
        walls,
        floors,
        walls_dirty,
        floors_dirty,
        roof_style,
        roof_pitch,
        id_map,
        fine_buildable,
        build_buy_enabled,
    })
}
pub(crate) fn position(r: &mut Reader<'_, '_>) -> Result<Position> {
    Ok(Position {
        x: r.i16()?,
        y: r.i16()?,
        level: r.i8()?,
    })
}
fn entity(r: &mut Reader<'_, '_>, avatar_: bool) -> Result<Entity> {
    let object_id = r.i16()?;
    let persist_id = r.u32()?;
    let budget = r.u32()?;
    let platform = if avatar_ {
        let permissions = r.u8()?;
        let n = r.count(4)?;
        let mut ignored = Vec::with_capacity(n);
        for _ in 0..n {
            ignored.push(r.u32()?);
        }
        let jobs = jobs(r)?;
        EntityPlatform::Avatar {
            budget,
            permissions,
            ignored,
            jobs,
            flags: r.u32()?,
            chat_rgb: r.take(3)?.try_into().unwrap(),
            chat_pitch: r.i8()?,
            chat_channel: r.u8()?,
        }
    } else {
        EntityPlatform::Object {
            budget,
            owner_id: r.u32()?,
            wear: r.u16()?,
            repair_quarters: r.u8()?,
            flags: r.u8()?,
            upgrade_level: r.u8()?,
        }
    };
    let object_data = r.shorts()?;
    let my_list = r.shorts()?;
    let headline = if r.boolean()? {
        Some(Headline {
            operand: r.take(8)?.try_into().unwrap(),
            target: r.i16()?,
            icon_target: r.i16()?,
            index: r.i8()?,
            duration: r.i32()?,
            animation: r.i32()?,
        })
    } else {
        None
    };
    let guid = r.u32()?;
    let master_guid = r.u32()?;
    let main_param = r.i16()?;
    let main_stack_object = r.i16()?;
    let contained = r.shorts()?;
    let container = r.i16()?;
    let container_slot = r.i16()?;
    let attributes = r.shorts()?;
    let n = r.count(6)?; // UInt16 relationship target and value count
    let mut object_relationships = Vec::with_capacity(n);
    for _ in 0..n {
        object_relationships.push((r.u16()?, r.shorts()?));
    }
    let persistent_relationships = persistent_relationships(r)?;
    let dynamic_flags = r.u64()?;
    let dynamic_flags2 = r.u64()?;
    let position = position(r)?;
    let timestamp_lockout = r.u32()?;
    let light_color = r.u32()?;
    let appearance = if avatar_ {
        Appearance::Avatar(Box::new(avatar(r)?))
    } else {
        Appearance::Object {
            direction: r.u8()?,
            disabled: r.u8()?,
        }
    };
    Ok(Entity {
        object_id,
        persist_id,
        platform,
        object_data,
        my_list,
        headline,
        guid,
        master_guid,
        main_param,
        main_stack_object,
        contained,
        container,
        container_slot,
        attributes,
        object_relationships,
        persistent_relationships,
        dynamic_flags,
        dynamic_flags2,
        position,
        timestamp_lockout,
        light_color,
        appearance,
    })
}
pub(crate) fn jobs(r: &mut Reader<'_, '_>) -> Result<Vec<(i16, [i16; 4])>> {
    let n = r.count(10)?;
    let mut jobs = Vec::with_capacity(n);
    for _ in 0..n {
        jobs.push((r.i16()?, [r.i16()?, r.i16()?, r.i16()?, r.i16()?]));
    }
    Ok(jobs)
}
pub(crate) fn persistent_relationships(r: &mut Reader<'_, '_>) -> Result<Vec<(u32, Vec<i16>)>> {
    let n = r.count(8)?;
    let mut relationships = Vec::with_capacity(n);
    for _ in 0..n {
        relationships.push((r.u32()?, r.shorts()?));
    }
    Ok(relationships)
}
pub(crate) fn outfit(r: &mut Reader<'_, '_>) -> Result<Outfit> {
    let id = r.u64()?;
    let name = if id == u32::MAX as u64 {
        Some(r.text()?)
    } else {
        None
    };
    Ok(Outfit { id, name })
}
fn animation(r: &mut Reader<'_, '_>) -> Result<Animation> {
    let name = r.text()?;
    let frame = r.f32()?;
    let n = r.byte_count(2)?;
    let event_queue = r.short_values(n)?;
    Ok(Animation {
        name,
        frame,
        event_queue,
        events_run: r.u8()?,
        end_reached: r.boolean()?,
        backwards: r.boolean()?,
        speed: r.f32()?,
        weight: r.f32()?,
        looped: r.boolean()?,
    })
}
fn avatar(r: &mut Reader<'_, '_>) -> Result<Avatar> {
    let n = r.count(18)?; // animation with empty name/event queue
    let mut animations = Vec::with_capacity(n);
    for _ in 0..n {
        animations.push(animation(r)?);
    }
    let carry_animation = if r.boolean()? {
        Some(animation(r)?)
    } else {
        None
    };
    let message = r.text()?;
    let message_timeout = r.i32()?;
    let n = r.count(13)?; // two Int16s, motive byte, Double fraction
    let mut motive_changes = Vec::with_capacity(n);
    for _ in 0..n {
        motive_changes.push(MotiveChange {
            per_hour: r.i16()?,
            maximum: r.i16()?,
            motive: r.u8()?,
            fractional: r.f64()?,
        });
    }
    let decay_last_minute = r.i32()?;
    let mut decay_fractions = [0; 7];
    for n in &mut decay_fractions {
        *n = r.i16()?;
    }
    let person_data = r.shorts()?;
    let motives = r.shorts()?;
    let old_hand_object = r.i16()?;
    let yaw = r.f32()?;
    let kill_timeout = r.i32()?;
    let default_suits = [outfit(r)?, outfit(r)?, outfit(r)?];
    let dynamic_suits = [r.u64()?, r.u64()?, r.u64()?, r.u64()?];
    let decoration = [r.u64()?, r.u64()?, r.u64()?, r.u64()?];
    let n = r.count(1)?;
    let mut bound_appearances = Vec::with_capacity(n);
    for _ in 0..n {
        bound_appearances.push(r.text()?);
    }
    let body = outfit(r)?;
    let head = outfit(r)?;
    let skin_tone = r.u8()?;
    Ok(Avatar {
        animations,
        carry_animation,
        message,
        message_timeout,
        motive_changes,
        decay_last_minute,
        decay_fractions,
        person_data,
        motives,
        old_hand_object,
        yaw,
        kill_timeout,
        default_suits,
        dynamic_suits,
        decoration,
        bound_appearances,
        body,
        head,
        skin_tone,
    })
}
pub(crate) fn chat_channel(r: &mut Reader<'_, '_>) -> Result<ChatChannel> {
    Ok(ChatChannel {
        id: r.u8()?,
        name: r.text()?,
        description: r.text()?,
        minimum_view_permission: r.u8()?,
        minimum_send_permission: r.u8()?,
        flags: r.u8()?,
        text_color: r.u32()?,
    })
}
fn lot_state(r: &mut Reader<'_, '_>) -> Result<LotState> {
    let name = r.text()?;
    let lot_id = r.u32()?;
    let mut surrounding_blends = Vec::with_capacity(9);
    for _ in 0..9 {
        surrounding_blends.push(r.take(4)?.try_into().unwrap());
    }
    let surrounding_roads = r.take(9)?.try_into().unwrap();
    let surrounding_heights = r.take(16)?.try_into().unwrap();
    let category = r.u8()?;
    let size = r.i32()?;
    let owner_id = r.u32()?;
    let n = r.short_count(4)?;
    let mut roommates = Vec::with_capacity(n);
    for _ in 0..n {
        roommates.push(r.u32()?);
    }
    let n = r.short_count(4)?;
    let mut build_roommates = Vec::with_capacity(n);
    for _ in 0..n {
        build_roommates.push(r.u32()?);
    }
    let job_ui = if r.boolean()? {
        let n = r.count(1)?;
        let mut messages = Vec::with_capacity(n);
        for _ in 0..n {
            messages.push(r.text()?);
        }
        Some(JobUi {
            messages,
            minutes: r.u8()?,
            seconds: r.u8()?,
            mode: r.u8()?,
        })
    } else {
        None
    };
    let skill_mode = r.u8()?;
    let n = r.byte_count(10)?; // channel with empty name/description
    let mut chat_channels = Vec::with_capacity(n);
    for _ in 0..n {
        chat_channels.push(chat_channel(r)?);
    }
    let neighborhood_id = r.u32()?;
    Ok(LotState {
        name,
        lot_id,
        surrounding_blends,
        surrounding_roads,
        surrounding_heights,
        category,
        size,
        owner_id,
        roommates,
        build_roommates,
        job_ui,
        skill_mode,
        chat_channels,
        neighborhood_id,
    })
}
pub(crate) fn tuning(r: &mut Reader<'_, '_>) -> Result<Tuning> {
    let version = r.i32()?;
    if version != 0 {
        return Err(r.error(ErrorKind::UnsupportedVersion(version), "DynamicTuning"));
    }
    let n = r.count(5)?; // type name and table count
    let mut types = Vec::with_capacity(n);
    for _ in 0..n {
        let name = r.text()?;
        let n = r.count(8)?; // table ID and value count
        let mut tables = Vec::with_capacity(n);
        for _ in 0..n {
            let table = r.i32()?;
            let n = r.count(8)?; // value ID and Single
            let mut values = Vec::with_capacity(n);
            for _ in 0..n {
                values.push((r.i32()?, r.f32()?));
            }
            tables.push((table, values));
        }
        types.push((name, tables));
    }
    Ok(Tuning { version, types })
}
fn thread(r: &mut Reader<'_, '_>) -> Result<Thread> {
    let n = r.count(25)?; // normal frame with null locals/args
    let mut stack = Vec::with_capacity(n);
    for _ in 0..n {
        stack.push(stack_frame(r)?);
    }
    let n = r.count(41)?; // action with null name/args/callback
    let mut queue = Vec::with_capacity(n);
    for _ in 0..n {
        queue.push(queued_action(r)?);
    }
    let active_queue_block = r.i8()?;
    let mut temp_registers = [0; 20];
    for n in &mut temp_registers {
        *n = r.i16()?;
    }
    let temp_xl = [r.i32()?, r.i32()?];
    let last_exit_code = r.u8()?;
    let blocking_state = if r.boolean()? {
        Some(async_state(r)?)
    } else {
        None
    };
    let eod_connection = if r.boolean()? {
        Some(plugin_thread(r)?)
    } else {
        None
    };
    Ok(Thread {
        stack,
        queue,
        active_queue_block,
        temp_registers,
        temp_xl,
        last_exit_code,
        blocking_state,
        eod_connection,
        interrupt: r.boolean()?,
        action_uid: r.u16()?,
        dialog_cooldown: r.i32()?,
        schedule_idle_start: r.u32()?,
    })
}
fn stack_frame(r: &mut Reader<'_, '_>) -> Result<StackFrame> {
    let frame_type = r.u8()?;
    if frame_type > 2 {
        return Err(r.error(ErrorKind::Invalid, "VM stack frame type"));
    }
    let routine_id = r.u16()?;
    let instruction_pointer = r.u16()?;
    let caller = r.i16()?;
    let callee = r.i16()?;
    let stack_object = r.i16()?;
    let code_owner_guid = r.u32()?;
    let locals = r.optional_shorts()?;
    let args = r.optional_shorts()?;
    let special_result = r.u8()?;
    let action_tree = r.boolean()?;
    let start = r.at;
    match frame_type {
        0 => {}
        1 => routing_frame(r)?,
        2 => {
            for _ in 0..4 {
                r.i32()?;
            }
            r.i16()?;
            r.i16()?;
            for _ in 0..3 {
                r.f32()?;
            }
            r.boolean()?;
            for _ in 0..5 {
                r.i32()?;
            }
        }
        _ => unreachable!(),
    }
    Ok(StackFrame {
        frame_type,
        routine_id,
        instruction_pointer,
        caller,
        callee,
        stack_object,
        code_owner_guid,
        locals,
        args,
        special_result,
        action_tree,
        extension: r.bytes[start..r.at].to_vec(),
    })
}
fn room_portal(r: &mut Reader<'_, '_>) -> Result<()> {
    r.i16()?;
    r.u16()?;
    Ok(())
}
fn path_segment(r: &mut Reader<'_, '_>) -> Result<()> {
    let kind = r.u8()?;
    let points = match kind {
        0 => 2,
        1 => 4,
        2 => 0,
        _ => return Err(r.error(ErrorKind::Invalid, "routing path segment type")),
    };
    for _ in 0..points {
        r.i32()?;
        r.i32()?;
    }
    Ok(())
}
fn find_location(r: &mut Reader<'_, '_>) -> Result<()> {
    r.f32()?;
    position(r)?;
    r.f64()?;
    r.boolean()?;
    r.i16()?;
    r.i32()?;
    Ok(())
}
fn routing_frame(r: &mut Reader<'_, '_>) -> Result<()> {
    let n = r.count(4)?;
    for _ in 0..n {
        room_portal(r)?;
    }
    if r.boolean()? {
        room_portal(r)?;
    }
    if let Some(n) = r.nullable_count(1)? {
        // null path segment has only its tag
        for _ in 0..n {
            path_segment(r)?;
        }
    }
    r.f64()?;
    r.f64()?;
    r.boolean()?;
    r.u8()?;
    for _ in 0..4 {
        r.i32()?;
    }
    r.boolean()?;
    r.f32()?;
    r.i32()?;
    for _ in 0..3 {
        r.i32()?;
    }
    r.boolean()?;
    let n = r.count(4)?;
    for _ in 0..n {
        room_portal(r)?;
    }
    r.shorts()?;
    position(r)?;
    path_segment(r)?;
    r.boolean()?;
    if r.boolean()? {
        r.u16()?;
        for _ in 0..3 {
            r.f32()?;
        }
        for _ in 0..8 {
            r.i32()?;
        }
        r.f32()?;
        for _ in 0..3 {
            r.i32()?;
        }
    }
    r.i16()?;
    if let Some(n) = r.nullable_count(24)? {
        for _ in 0..n {
            find_location(r)?;
        }
    }
    if r.boolean()? {
        find_location(r)?;
    }
    r.i16()?;
    Ok(())
}
fn queued_action(r: &mut Reader<'_, '_>) -> Result<QueuedAction> {
    let routine_id = r.u16()?;
    let check_routine_id = r.u16()?;
    let callee = r.i16()?;
    let stack_object = r.i16()?;
    let icon_owner = r.i16()?;
    let code_owner_guid = r.u32()?;
    let name = if r.boolean()? { Some(r.text()?) } else { None };
    let args = r.optional_shorts()?;
    let interaction_number = r.i32()?;
    let notify_idle = r.boolean()?;
    let priority = r.i16()?;
    let mode = r.u8()?;
    let flags = r.u32()?;
    let flags2 = r.u32()?;
    let uid = r.u16()?;
    // Source SerializeInto writes Interaction as Int16, but its old Deserialize
    // reads Byte. Parse the actual emitted wire; retain refresh-only semantics.
    let callback = if r.boolean()? {
        Some(ActionCallback {
            kind: r.i32()?,
            target: r.i16()?,
            interaction: r.i16()?,
            set_param: r.boolean()?,
            stack_object: r.i16()?,
            caller: r.i16()?,
            is_tree: r.boolean()?,
        })
    } else {
        None
    };
    Ok(QueuedAction {
        routine_id,
        check_routine_id,
        callee,
        stack_object,
        icon_owner,
        code_owner_guid,
        name,
        args,
        interaction_number,
        notify_idle,
        priority,
        mode,
        flags,
        flags2,
        uid,
        callback,
        interaction_result: r.i8()?,
        result_check_counter: r.u16()?,
    })
}
pub(crate) fn eod_object_event(r: &mut Reader<'_, '_>) -> Result<EodObjectEvent> {
    let code = r.i16()?;
    let n = r.byte_count(2)?;
    if n > 4 {
        return Err(r.error(ErrorKind::Invalid, "source EOD event data maximum four"));
    }
    Ok(EodObjectEvent {
        code,
        data: r.short_values(n)?,
    })
}
fn plugin_fields(
    r: &mut Reader<'_, '_>,
    responded: bool,
    wait_time: i32,
) -> Result<PluginThreadState> {
    let avatar_id = r.i16()?;
    let object_id = r.i16()?;
    let joinable = r.boolean()?;
    let ended = r.boolean()?;
    let n = r.byte_count(3)?; // event code and empty-data count
    let mut events = Vec::with_capacity(n);
    for _ in 0..n {
        events.push(eod_object_event(r)?);
    }
    Ok(PluginThreadState {
        responded,
        wait_time,
        avatar_id,
        object_id,
        joinable,
        ended,
        events,
    })
}
fn plugin_thread(r: &mut Reader<'_, '_>) -> Result<PluginThreadState> {
    let responded = r.boolean()?;
    let wait_time = r.i32()?;
    plugin_fields(r, responded, wait_time)
}
pub(crate) fn async_state(r: &mut Reader<'_, '_>) -> Result<AsyncState> {
    let kind = r.u8()?;
    if kind > 3 {
        return Err(r.error(ErrorKind::Invalid, "VM async state type"));
    }
    let responded = r.boolean()?;
    let wait_time = r.i32()?;
    let start = r.at;
    match kind {
        0 => {
            r.boolean()?;
            r.i32()?;
            for _ in 0..4 {
                r.u32()?;
            }
        }
        1 => {
            r.i32()?;
            r.u8()?;
            r.text()?;
            r.u8()?;
        }
        2 => {
            plugin_fields(r, responded, wait_time)?;
        }
        3 => {
            r.boolean()?;
            r.boolean()?;
            r.i32()?;
            r.u32()?;
            r.u8()?;
            r.i16()?;
            r.shorts()?;
        }
        _ => unreachable!(),
    }
    Ok(AsyncState {
        kind,
        responded,
        wait_time,
        source_fields: r.bytes[start..r.at].to_vec(),
    })
}
