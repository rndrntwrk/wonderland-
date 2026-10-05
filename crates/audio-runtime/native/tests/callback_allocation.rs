//! Test-only allocator instrumentation. Production transport forbids unsafe.
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use wonderland_audio_runtime::{
    mixer::{MixerIntent, VoiceId, VolumeGroup},
    pcm::PcmBuffer,
};
use wonderland_native_audio::{channel, Command, DeviceFault, DriverConfig};
use wonderland_render_core::AssetKey;

thread_local! {
    static TRACK: Cell<bool> = const { Cell::new(false) };
    static OPERATIONS: Cell<usize> = const { Cell::new(0) };
}
struct CountedSystem;
fn count() {
    let tracking = TRACK.try_with(Cell::get).unwrap_or(false);
    if tracking {
        let _ = OPERATIONS.try_with(|count| count.set(count.get() + 1));
    }
}
// SAFETY: This test allocator forwards each request, unchanged, to System.
// Thread-local counters do not allocate or change allocation ownership.
unsafe impl GlobalAlloc for CountedSystem {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        System.alloc(layout)
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count();
        System.alloc_zeroed(layout)
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        count();
        System.dealloc(pointer, layout);
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count();
        System.realloc(pointer, layout, size)
    }
}
#[global_allocator]
static ALLOCATOR: CountedSystem = CountedSystem;

fn without_heap(work: impl FnOnce()) {
    OPERATIONS.with(|counter| counter.set(0));
    TRACK.with(|tracking| tracking.set(true));
    work();
    TRACK.with(|tracking| tracking.set(false));
    assert_eq!(
        OPERATIONS.with(Cell::get),
        0,
        "callback allocated or freed heap memory"
    );
}

#[test]
fn application_data_callback_has_no_allocations_or_deallocations_in_all_paths() {
    let cfg = DriverConfig {
        sample_rate: 8,
        block_frames: 2,
        ring_frames: 4,
        max_callback_frames: 4,
        ..DriverConfig::default()
    };
    let (mut control, mut worker, mut callback) = channel(cfg).unwrap();
    let session = control.session();
    control
        .try_submit(
            session,
            Command::InsertSample {
                key: AssetKey([1; 32]),
                pcm: PcmBuffer {
                    sample_rate: 8,
                    channels: 1,
                    samples: vec![10, -10],
                },
            },
        )
        .unwrap();
    control
        .try_submit(
            session,
            Command::Apply(MixerIntent::Start {
                voice: VoiceId {
                    generation: 1,
                    serial: 1,
                },
                sample: AssetKey([1; 32]),
                group: VolumeGroup::Fx,
                gain: 1.0,
                pan: 0.0,
                looped: true,
                seek_frame: 0,
            }),
        )
        .unwrap();
    worker.step();
    let mut i16_out = [0; 8];
    let mut f32_out = [0.0; 8];
    let mut u16_out = [0; 8];
    without_heap(|| callback.write_i16(&mut i16_out));
    worker.step();
    without_heap(|| callback.write_f32(&mut f32_out));
    worker.step();
    without_heap(|| callback.write_u16(&mut u16_out));
    without_heap(|| callback.write_i16(&mut i16_out)); // underrun
    control.suspend();
    without_heap(|| callback.write_i16(&mut i16_out));
    control.resume().unwrap();
    worker.step();
    control.reset().unwrap();
    without_heap(|| callback.write_i16(&mut i16_out)); // stale frames
    let fault = control.fault_signal();
    without_heap(|| fault.report(DeviceFault::Disconnected));
    without_heap(|| callback.write_i16(&mut i16_out));
    without_heap(|| callback.write_i16(&mut [0; 9])); // shape rejection
    without_heap(|| callback.write_i16(&mut [0; 10])); // size rejection
}
