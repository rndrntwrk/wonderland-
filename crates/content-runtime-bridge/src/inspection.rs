//! Bounded read-only JSON views of the real stored runtime. Positions describe
//! stored frames and continuations; this is not an executed-instruction trace.
use serde::{ser::SerializeSeq, Serialize, Serializer};
use sim_core::{
    ids::EntityRef,
    runtime::SimRuntime,
    state::{ContentDescriptor, ContinuationKind, SimState},
    vm::{ActionString, VmDiagnostic, VmFrame, VmStop},
    world::{
        routing::RoutePhase,
        slots::{SlotEntry, SlotState},
        LotPosition, SlotDefinition, SlotKey,
    },
};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Write},
};

pub const MAX_INSPECTION_JSON_BYTES: usize = 1024 * 1024;

#[derive(Serialize)]
struct EntityView<'a> {
    schema: &'static str,
    lot_id: u64,
    authority_epoch: u64,
    completed_tick: u64,
    content: ContentDescriptor,
    entity: EntityRef,
    guid: u32,
    revision: u64,
    is_avatar: bool,
    position: sim_core::vm::VmPosition,
    attributes: &'a [i16],
    object_data: &'a [i16],
    slots: &'a [Option<EntityRef>],
    container: Option<(EntityRef, u16)>,
    active_queue_users: &'a BTreeSet<EntityRef>,
    active_advertisements: Option<Advertisements<'a>>,
    thread: ThreadView<'a>,
    routes: Routes<'a>,
    slot_state: Slots<'a>,
}

#[derive(Serialize)]
struct ThreadView<'a> {
    is_check: bool,
    interrupt: bool,
    stop: &'a VmStop,
    frames: &'a [VmFrame],
    temps: &'a [i16; 20],
    temp_xl: &'a [i32; 2],
    advertisements: Advertisements<'a>,
    action_strings: Option<&'a [ActionString]>,
    diagnostics: &'a [VmDiagnostic],
}

struct Advertisements<'a>(&'a BTreeMap<(u8, u16), i16>);
impl Serialize for Advertisements<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Entry {
            kind: u8,
            index: u16,
            value: i16,
        }
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for (&(kind, index), &value) in self.0 {
            seq.serialize_element(&Entry { kind, index, value })?;
        }
        seq.end()
    }
}

struct Routes<'a> {
    state: &'a SimState,
    entity: EntityRef,
}
impl Serialize for Routes<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct RouteView {
            continuation_id: u64,
            route_id: u64,
            resumes_vm: bool,
            target: Option<EntityRef>,
            position: LotPosition,
            phase: RoutePhase,
            callback_token: Option<u64>,
        }
        let routes = || {
            self.state.continuations.values().filter_map(|entry| {
                if entry.entity != self.entity {
                    return None;
                }
                let ContinuationKind::Route(route) = &entry.kind else {
                    return None;
                };
                Some(RouteView {
                    continuation_id: entry.id,
                    route_id: route.id(),
                    resumes_vm: entry.resumes_vm,
                    target: route.target(),
                    position: route.position(),
                    phase: route.phase(),
                    callback_token: route.pending_callback().map(|value| value.token),
                })
            })
        };
        let mut seq = serializer.serialize_seq(Some(routes().count()))?;
        for route in routes() {
            seq.serialize_element(&route)?;
        }
        seq.end()
    }
}

struct Slots<'a> {
    state: &'a SlotState,
    entity: EntityRef,
}
struct Reservations<'a> {
    entry: &'a SlotEntry,
    entity: EntityRef,
    owner: bool,
}
struct Occupants<'a> {
    entry: &'a SlotEntry,
    entity: EntityRef,
    owner: bool,
}
impl Serialize for Reservations<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let rows = || {
            self.entry
                .reservations
                .values()
                .filter(|row| self.owner || row.token.actor == self.entity)
        };
        let mut seq = serializer.serialize_seq(Some(rows().count()))?;
        for row in rows() {
            seq.serialize_element(row)?;
        }
        seq.end()
    }
}
impl Serialize for Occupants<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let rows = || {
            self.entry
                .occupants
                .values()
                .filter(|token| self.owner || token.actor == self.entity)
        };
        let mut seq = serializer.serialize_seq(Some(rows().count()))?;
        for row in rows() {
            seq.serialize_element(row)?;
        }
        seq.end()
    }
}
impl Serialize for Slots<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct SlotView<'a> {
            key: SlotKey,
            definition: Option<&'a SlotDefinition>,
            reservations: Reservations<'a>,
            occupants: Occupants<'a>,
        }
        let rows = || {
            self.state.entries().iter().filter_map(|(key, entry)| {
                let owner = key.owner == self.entity;
                if !owner
                    && !entry.occupants.contains_key(&self.entity)
                    && !entry
                        .reservations
                        .values()
                        .any(|row| row.token.actor == self.entity)
                {
                    return None;
                }
                Some(SlotView {
                    key: *key,
                    definition: entry.definition.as_ref(),
                    reservations: Reservations {
                        entry,
                        entity: self.entity,
                        owner,
                    },
                    occupants: Occupants {
                        entry,
                        entity: self.entity,
                        owner,
                    },
                })
            })
        };
        let mut seq = serializer.serialize_seq(Some(rows().count()))?;
        for row in rows() {
            seq.serialize_element(&row)?;
        }
        seq.end()
    }
}

struct Output<'a> {
    bytes: Option<&'a mut Vec<u8>>,
    used: usize,
    limit: usize,
}
impl Write for Output<'_> {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        self.used = self
            .used
            .checked_add(input.len())
            .filter(|size| *size <= self.limit)
            .ok_or_else(|| io::Error::other("entity inspection JSON byte limit"))?;
        if let Some(output) = &mut self.bytes {
            output.extend_from_slice(input);
        }
        Ok(input.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Serialize borrowed state only after a nonallocating counting pass admits the
/// entire report, including JSON escapes in faults, labels and action strings.
/// A one-byte-short limit fails instead of truncating or partially publishing.
/// Callers provide the authenticated debugging/preview capability outside this
/// pure library. This API does not expose any private EOD host state.
pub fn inspect_entity_json(
    runtime: &SimRuntime,
    entity: EntityRef,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    if max_bytes == 0 || max_bytes > MAX_INSPECTION_JSON_BYTES {
        return Err("entity inspection JSON limit must be between 1 and 1048576".into());
    }
    let state = runtime.state();
    if !state.ids.is_live(entity) {
        return Err("inspection entity is not a live generation".into());
    }
    let item = state
        .entities
        .get(&entity.object_id)
        .filter(|item| item.info.reference == entity)
        .ok_or("inspection entity state missing")?;
    let thread = state
        .threads
        .get(&entity.object_id)
        .filter(|thread| thread.owner == entity)
        .ok_or("inspection thread state missing")?;
    let view = EntityView {
        schema: "wonderland.runtime-entity-inspection.v1",
        lot_id: state.lot_id,
        authority_epoch: state.authority_epoch,
        completed_tick: state.completed_tick,
        content: state.content,
        entity,
        guid: item.info.guid,
        revision: item.revision,
        is_avatar: item.info.is_avatar,
        position: item.info.position,
        attributes: &item.attributes,
        object_data: &item.object_data,
        slots: &item.slots,
        container: item.container,
        active_queue_users: &item.queued_users,
        active_advertisements: item.active_advertisements.as_ref().map(Advertisements),
        thread: ThreadView {
            is_check: thread.is_check,
            interrupt: thread.interrupt,
            stop: &thread.stop,
            frames: &thread.frames,
            temps: &thread.temps,
            temp_xl: &thread.temp_xl,
            advertisements: Advertisements(&thread.tree_advertisements),
            action_strings: thread.action_strings.as_deref(),
            diagnostics: &thread.diagnostics,
        },
        routes: Routes { state, entity },
        slot_state: Slots {
            state: &state.world.slots,
            entity,
        },
    };
    let mut counter = Output {
        bytes: None,
        used: 0,
        limit: max_bytes,
    };
    serde_json::to_writer(&mut counter, &crate::json_u64::ExactU64(&view))
        .map_err(|error| error.to_string())?;
    let size = counter.used;
    let mut bytes = Vec::with_capacity(size);
    let mut output = Output {
        bytes: Some(&mut bytes),
        used: 0,
        limit: size,
    };
    serde_json::to_writer(&mut output, &crate::json_u64::ExactU64(&view))
        .map_err(|error| error.to_string())?;
    Ok(bytes)
}
