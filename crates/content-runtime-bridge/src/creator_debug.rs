//! Creator snapshot watches and whole-tick stepping with the real runtime.
use crate::isolated::{IsolatedRuntime, StateWatch, WatchField, WatchValue as ObservedValue};
use sim_core::{
    ids::{EntityRef, ObjectId},
    snapshot::SnapshotExpectation,
    state::ContentSet,
    vm::VmStop,
};
use std::fmt::{self, Write};
use wonderland_creator::debug::{
    DebugSnapshot, IsolatedDebugProvider, TraceEvent, Watch, WatchValue,
};

/// Maximum retained report bytes: result-vector storage, field strings and
/// formatted values. Snapshot restoration and parsed requests have separate
/// runtime/count bounds. Reports exceeding this limit are rejected, not truncated.
pub const MAX_INSPECTION_OUTPUT_BYTES: usize = 1024 * 1024;
const OUTPUT_LIMIT_ERROR: &str = "creator inspection output byte limit";

enum Observation<'a> {
    Scalar(i64),
    Stop(&'a VmStop),
}

impl fmt::Display for Observation<'_> {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Scalar(value) => fmt::Display::fmt(value, output),
            Self::Stop(stop) => fmt::Debug::fmt(stop, output),
        }
    }
}

fn observe(isolated: &IsolatedRuntime, watch: StateWatch) -> Result<Observation<'_>, String> {
    if watch.field == WatchField::Stop {
        let state = isolated.runtime().state();
        if !state.ids.is_live(watch.entity) {
            return Err("watch entity is not a live generation".into());
        }
        let thread = state
            .threads
            .get(&watch.entity.object_id)
            .ok_or("watch entity has no stored thread")?;
        // Fault variants may contain large strings. Borrow the actual stop so
        // neither the counting pass nor report admission clones that payload.
        Ok(Observation::Stop(&thread.stop))
    } else {
        match isolated.watch(watch)? {
            ObservedValue::Scalar(value) => Ok(Observation::Scalar(value)),
            ObservedValue::Stop(_) => Err("unexpected non-scalar watch result".into()),
        }
    }
}

struct ByteCounter {
    bytes: usize,
    limit: usize,
}

impl Write for ByteCounter {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.bytes = self
            .bytes
            .checked_add(text.len())
            .filter(|bytes| *bytes <= self.limit)
            .ok_or(fmt::Error)?;
        Ok(())
    }
}

struct FixedText {
    text: String,
    limit: usize,
}

impl Write for FixedText {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.text
            .len()
            .checked_add(text.len())
            .filter(|bytes| *bytes <= self.limit)
            .ok_or(fmt::Error)?;
        self.text.push_str(text);
        Ok(())
    }
}

/// Watch fields are at most 17 ASCII bytes. Indices use canonical unsigned
/// decimal u16 spelling: `0` or one to five digits without a leading zero.
/// Every watch is validated before restoring the snapshot or cloning results.
pub struct SimDebugProvider {
    content: ContentSet,
    expectation: SnapshotExpectation,
}

/// The existing creator interface has one u64 identity. Its explicit bridge
/// encoding retains both the positive local i16 ID and the u32 generation.
pub fn creator_entity(entity: EntityRef) -> u64 {
    (u64::from(entity.generation) << 16) | u64::from(entity.object_id.0 as u16)
}

fn parse_watch(watch: &Watch) -> Result<StateWatch, String> {
    // object-data/65535 is the longest accepted field spelling.
    if watch.field.len() > 17 {
        return Err("creator watch field length limit: maximum 17 ASCII bytes".into());
    }
    let id = watch.entity as u16;
    let generation =
        u32::try_from(watch.entity >> 16).map_err(|_| "creator entity generation overflow")?;
    if id == 0 || id > i16::MAX as u16 || generation == 0 {
        return Err("invalid creator entity identity".into());
    }
    let entity = EntityRef {
        object_id: ObjectId(id as i16),
        generation,
    };
    let field = if watch.field == "stop" {
        WatchField::Stop
    } else {
        let (kind, rest) = watch.field.split_once('/').ok_or("unknown watch field")?;
        if kind == "local" || kind == "arg" {
            let (depth, index) = rest
                .split_once('/')
                .ok_or("frame watch requires depth/index")?;
            let depth = parse_index(depth)?;
            let index = parse_index(index)?;
            return Ok(StateWatch {
                entity,
                field: if kind == "local" {
                    WatchField::Local { depth, index }
                } else {
                    WatchField::Argument { depth, index }
                },
            });
        }
        let index = parse_index(rest)?;
        match kind {
            "attribute" => WatchField::Attribute(index),
            "object-data" => WatchField::ObjectData(index),
            "temp" => WatchField::Temp(index),
            "temp-xl" => WatchField::TempXl(index),
            "person-data" => WatchField::PersonData(index),
            "motive" => WatchField::Motive(index),
            "global" => WatchField::Global(index),
            _ => return Err("unknown watch field".into()),
        }
    };
    Ok(StateWatch { entity, field })
}

fn parse_index(index: &str) -> Result<u16, String> {
    if index.is_empty()
        || index.len() > 5
        || !index.bytes().all(|byte| byte.is_ascii_digit())
        || (index.len() > 1 && index.starts_with('0'))
    {
        return Err("invalid watch index; use canonical unsigned decimal u16 spelling".into());
    }
    index
        .parse::<u16>()
        .map_err(|_| "invalid watch index: exceeds u16".into())
}

impl SimDebugProvider {
    pub fn new(content: ContentSet, expectation: SnapshotExpectation) -> Result<Self, String> {
        content.validate()?;
        expectation
            .validate()
            .map_err(|error| format!("snapshot expectation: {error:?}"))?;
        Ok(Self {
            content,
            expectation,
        })
    }
    pub fn restore(&self, snapshot: &DebugSnapshot) -> Result<IsolatedRuntime, String> {
        let isolated = IsolatedRuntime::from_snapshot(
            &snapshot.bytes,
            self.content.clone(),
            self.expectation,
        )?;
        if snapshot.tick != isolated.runtime().state().completed_tick {
            return Err("creator snapshot tick does not match validated payload".into());
        }
        Ok(isolated)
    }
}

impl IsolatedDebugProvider for SimDebugProvider {
    fn inspect(
        &self,
        snapshot: &DebugSnapshot,
        watches: &[Watch],
    ) -> Result<Vec<WatchValue>, String> {
        if watches.len() > 4096 {
            return Err("creator watch count limit".into());
        }
        // This typed collection is bounded by the watch count and contains no
        // cloned request strings. A malformed later watch cannot trigger restore.
        let parsed = watches
            .iter()
            .map(parse_watch)
            .collect::<Result<Vec<_>, _>>()?;
        let isolated = self.restore(snapshot)?;
        let mut retained = watches
            .len()
            .checked_mul(std::mem::size_of::<WatchValue>())
            .filter(|bytes| *bytes <= MAX_INSPECTION_OUTPUT_BYTES)
            .ok_or(OUTPUT_LIMIT_ERROR)?;
        let mut measured = Vec::with_capacity(watches.len());
        // Admit every formatted value before allocating any report strings.
        // Debug formatting can expand control characters, so raw fault-string
        // lengths alone are insufficient. The counting writer never allocates.
        for (watch, parsed) in watches.iter().zip(parsed) {
            retained = retained
                .checked_add(watch.field.len())
                .filter(|bytes| *bytes <= MAX_INSPECTION_OUTPUT_BYTES)
                .ok_or(OUTPUT_LIMIT_ERROR)?;
            let value = observe(&isolated, parsed)?;
            let mut counter = ByteCounter {
                bytes: 0,
                limit: MAX_INSPECTION_OUTPUT_BYTES - retained,
            };
            write!(&mut counter, "{value}").map_err(|_| OUTPUT_LIMIT_ERROR)?;
            retained += counter.bytes;
            measured.push((value, counter.bytes));
        }
        let mut output = Vec::with_capacity(watches.len());
        for (watch, (value, bytes)) in watches.iter().zip(measured) {
            let mut formatted = FixedText {
                text: String::with_capacity(bytes),
                limit: bytes,
            };
            write!(&mut formatted, "{value}").map_err(|_| OUTPUT_LIMIT_ERROR)?;
            if formatted.text.len() != bytes {
                return Err("creator watch formatting changed after admission".into());
            }
            let mut field = String::with_capacity(watch.field.len());
            field.push_str(&watch.field);
            output.push(WatchValue {
                watch: Watch {
                    entity: watch.entity,
                    field,
                },
                value: formatted.text,
            });
        }
        Ok(output)
    }
    fn trace(&self, _: &DebugSnapshot) -> Result<Vec<TraceEvent>, String> {
        Err(
            "executed-instruction trace unsupported; frame_positions observes stored frames only"
                .into(),
        )
    }
    fn step_isolated(&self, snapshot: &DebugSnapshot) -> Result<DebugSnapshot, String> {
        let mut isolated = self.restore(snapshot)?;
        let accepted = isolated
            .runtime()
            .next_tick(vec![])
            .map_err(|e| e.to_string())?;
        isolated.advance(&accepted)?;
        Ok(DebugSnapshot {
            tick: isolated.runtime().state().completed_tick,
            bytes: isolated.runtime().snapshot().map_err(|e| e.to_string())?,
        })
    }
}
