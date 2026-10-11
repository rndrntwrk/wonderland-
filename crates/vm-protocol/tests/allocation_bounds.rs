//! Observe real allocator requests while malformed source records are decoded.
//! The allocator delegates unchanged to System; no decoder behavior is mocked.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
use wonderland_vm_protocol::*;

thread_local! {
    static LARGEST_REQUEST: Cell<Option<usize>> = const { Cell::new(None) };
}

struct ObservedAllocator;

fn observe(size: usize) {
    let _ = LARGEST_REQUEST.try_with(|largest| {
        if let Some(previous) = largest.get() {
            largest.set(Some(previous.max(size)));
        }
    });
}

// SAFETY: Every operation delegates the original pointer/layout unchanged to
// System. Observation only updates a const-initialized, allocation-free TLS Cell.
unsafe impl GlobalAlloc for ObservedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        observe(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        observe(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        observe(size);
        unsafe { System.realloc(ptr, layout, size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: ObservedAllocator = ObservedAllocator;

fn measured<T>(decode: impl FnOnce() -> T) -> (T, usize) {
    LARGEST_REQUEST.with(|largest| largest.set(Some(0)));
    let result = decode();
    let largest = LARGEST_REQUEST.with(|largest| largest.replace(None).unwrap());
    (result, largest)
}

fn rejects_without_large_reservation<T: std::fmt::Debug>(
    decode: impl FnOnce() -> Result<T>,
    maximum_request: usize,
) {
    let (result, largest) = measured(decode);
    assert_eq!(result.unwrap_err().kind, ErrorKind::Truncated);
    assert!(
        largest <= maximum_request,
        "malformed collection requested {largest} bytes before rejecting it"
    );
}

#[test]
fn missing_ticks_reject_before_reserving_the_tick_vector() {
    let mut bytes = vec![0];
    bytes.extend(4096i32.to_le_bytes());
    rejects_without_large_reservation(|| decode_tick_list(&bytes, &Default::default()), 64 * 1024);
}

#[test]
fn missing_commands_reject_before_reserving_the_command_vector() {
    let mut bytes = vec![0];
    bytes.extend(1i32.to_le_bytes());
    bytes.extend(0u32.to_le_bytes());
    bytes.extend(0u64.to_le_bytes());
    bytes.extend(65_536i32.to_le_bytes());
    let limits = DecodeLimits {
        max_input_bytes: bytes.len(),
        ..Default::default()
    };
    rejects_without_large_reservation(|| decode_tick_list(&bytes, &limits), 64 * 1024);
}

// Minimal uncompressed v38 TSO context, from VMContextMarshal and
// VMArchitectureMarshal.SerializeInto. Subsequent test fields are deliberately
// incomplete; these fixtures are not claimed to be original saves.
fn context_prefix(width: i32, height: i32) -> Vec<u8> {
    let mut bytes = b"FSOv".to_vec();
    bytes.extend(38i32.to_le_bytes());
    bytes.extend([0, 0]); // uncompressed, TSO
    bytes.extend([0; 48]); // source clock
    bytes.extend(width.to_le_bytes());
    bytes.extend(height.to_le_bytes());
    bytes.extend(1i32.to_le_bytes()); // one story
    bytes.extend([0, 0]); // terrain types
    let cells = (width * height) as usize;
    bytes.extend(((cells * 2) as i32).to_le_bytes());
    bytes.resize(bytes.len() + cells * 2, 0); // signed height samples
    bytes.extend((cells as i32).to_le_bytes());
    bytes.resize(bytes.len() + cells, 0); // grass
    bytes
}

fn complete_small_context() -> Vec<u8> {
    let mut bytes = context_prefix(1, 1);
    bytes.extend([0; 13]); // wall tile
    bytes.extend(0u16.to_le_bytes()); // floor tile
    bytes.extend([0, 0]); // dirty flags
    bytes.extend(0u32.to_le_bytes()); // roof style
    bytes.extend(0f32.to_le_bytes()); // roof pitch
    bytes.extend([0, 0, 1]); // no remap, no fine-build mask, build enabled
    bytes.extend([0; 16]); // ambience and RNG
    bytes
}

#[test]
fn missing_entities_reject_before_reserving_the_entity_vector() {
    let mut bytes = complete_small_context();
    bytes.extend(32_767i32.to_le_bytes());
    rejects_without_large_reservation(|| decode_snapshot(&bytes, &Default::default()), 64 * 1024);
}

fn one_entity_before_thread() -> Vec<u8> {
    let mut bytes = complete_small_context();
    bytes.extend(1i32.to_le_bytes());
    bytes.push(0); // game object
    bytes.extend(1i16.to_le_bytes()); // ObjectID
    bytes.extend(0u32.to_le_bytes()); // PersistID
    bytes.extend([0; 13]); // object platform state
    bytes.extend([0; 8]); // empty ObjectData and MyList
    bytes.push(0); // no headline
    bytes.extend([0; 8]); // GUID and master GUID
    bytes.extend([0; 4]); // main parameters
    bytes.extend([0; 4]); // empty contained IDs
    bytes.extend([0; 4]); // container and slot
    bytes.extend([0; 12]); // empty attributes and relationship collections
    bytes.extend([0; 16]); // dynamic masks
    bytes.extend([0; 5]); // position
    bytes.extend([0; 8]); // lockout and light color
    bytes.extend([0; 2]); // direction and disabled flags
    bytes.extend(1i32.to_le_bytes()); // corresponding thread
    bytes
}

#[test]
fn missing_stack_frames_reject_before_reserving_the_frame_vector() {
    let mut bytes = one_entity_before_thread();
    bytes.extend(65_536i32.to_le_bytes());
    bytes.extend([0; 80]); // enough for a minimal thread, not its declared stack
    rejects_without_large_reservation(|| decode_snapshot(&bytes, &Default::default()), 64 * 1024);
}

#[test]
fn missing_queued_actions_reject_before_reserving_the_queue_vector() {
    let mut bytes = one_entity_before_thread();
    bytes.extend(0i32.to_le_bytes()); // empty stack
    bytes.extend(65_536i32.to_le_bytes());
    bytes.extend([0; 64]); // enough for a minimal thread, not its declared queue
    rejects_without_large_reservation(|| decode_snapshot(&bytes, &Default::default()), 64 * 1024);
}

fn state_sync_before_traces() -> Vec<u8> {
    let mut bytes = vec![12];
    bytes.extend(include_bytes!("fixtures/source-v38-object-compressed.fsov"));
    bytes.push(1); // traces present
    bytes
}

#[test]
fn missing_trace_ticks_reject_before_reserving_the_trace_vector() {
    let mut bytes = state_sync_before_traces();
    bytes.extend(65_536i32.to_le_bytes());
    rejects_without_large_reservation(
        || decode_direct_command(&bytes, &Default::default()),
        64 * 1024,
    );
}

#[test]
fn missing_trace_strings_reject_before_reserving_the_string_vector() {
    let mut bytes = state_sync_before_traces();
    bytes.extend(1i32.to_le_bytes());
    bytes.extend(0u32.to_le_bytes());
    bytes.extend(65_536i32.to_le_bytes());
    rejects_without_large_reservation(
        || decode_direct_command(&bytes, &Default::default()),
        64 * 1024,
    );
}

#[test]
fn missing_wall_floor_records_reject_before_reserving_a_large_level() {
    let bytes = context_prefix(512, 512);
    // The supplied terrain legitimately needs 512 KiB for decoded heights, but
    // the absent wall/floor level must not cause a multi-megabyte reservation.
    rejects_without_large_reservation(|| decode_snapshot(&bytes, &Default::default()), 1024 * 1024);
}

#[test]
fn byte_counted_event_data_obeys_the_configured_collection_limit() {
    let bytes = [17, 7, 0, 0, 0, 8, 0, 5, 0, 1, 9, 0];
    let mut limits = DecodeLimits {
        max_count: 0,
        ..Default::default()
    };
    assert_eq!(
        decode_direct_command(&bytes, &limits).unwrap_err().kind,
        ErrorKind::Limit
    );
    limits.max_count = 1;
    assert_eq!(
        decode_direct_command(&bytes, &limits).unwrap().consumed,
        bytes.len()
    );
}
