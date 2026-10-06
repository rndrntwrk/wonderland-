use std::time::{Duration, Instant};
use wonderland_audio_runtime::{
    mixer::{MixerIntent, VoiceId, VolumeGroup},
    pcm::PcmBuffer,
};
use wonderland_native_audio::{channel, Command, DriverConfig, Event};
use wonderland_render_core::AssetKey;

#[test]
fn worker_policy_rejects_sub_two_millisecond_rings_at_high_rates() {
    let too_small = DriverConfig {
        sample_rate: 48_000,
        block_frames: 16,
        ring_frames: 16,
        ..DriverConfig::default()
    };
    assert!(too_small.validate().is_err());
    let minimum = DriverConfig {
        ring_frames: 96,
        ..too_small
    };
    assert!(minimum.validate().is_ok());
    let fast = DriverConfig {
        sample_rate: 384_000,
        ring_frames: 767,
        ..minimum.clone()
    };
    assert!(fast.validate().is_err());
    let minimum_fast = DriverConfig {
        ring_frames: 768,
        ..fast
    };
    assert!(minimum_fast.validate().is_ok());
}

#[test]
fn high_rate_small_blocks_refill_the_entire_bounded_ring_before_parking() {
    let cfg = DriverConfig {
        sample_rate: 384_000,
        block_frames: 16,
        ring_frames: 1024,
        ..DriverConfig::default()
    };
    let (mut control, mut worker, mut callback) = channel(cfg).unwrap();
    let session = control.session();
    control
        .try_submit(
            session,
            Command::InsertSample {
                key: AssetKey([5; 32]),
                pcm: PcmBuffer {
                    sample_rate: 384_000,
                    channels: 1,
                    samples: vec![1234],
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
                sample: AssetKey([5; 32]),
                group: VolumeGroup::Fx,
                gain: 1.0,
                pan: 0.0,
                looped: true,
                seek_frame: 0,
            }),
        )
        .unwrap();
    assert_eq!(worker.refill(), 1024);
    assert_eq!(control.snapshot().queued_frames, 1024);
    assert_eq!(worker.refill(), 0);
    let mut out = [0; 2048];
    callback.write_i16(&mut out);
    assert!(out.iter().all(|sample| *sample == 1234));
    assert_eq!(worker.refill(), 1024);
}

#[test]
fn concurrent_worker_and_variable_callback_sizes_never_tear_repeat_or_reorder_frames() {
    let cfg = DriverConfig {
        sample_rate: 48_000,
        max_voices: 4,
        max_pcm_bytes: 64 * 1024,
        max_queued_pcm_bytes: 64 * 1024,
        command_capacity: 8,
        event_capacity: 8,
        block_frames: 64,
        ring_frames: 128,
        max_callback_frames: 64,
    };
    let (mut control, worker, mut callback) = channel(cfg).unwrap();
    let session = control.session();
    let expected: Vec<[i16; 2]> = (1..=4096).map(|i| [i, -i]).collect();
    control
        .try_submit(
            session,
            Command::InsertSample {
                key: AssetKey([5; 32]),
                pcm: PcmBuffer {
                    sample_rate: 48_000,
                    channels: 2,
                    samples: expected
                        .iter()
                        .flat_map(|frame| frame.iter())
                        .copied()
                        .collect(),
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
                sample: AssetKey([5; 32]),
                group: VolumeGroup::Fx,
                gain: 1.0,
                pan: 0.0,
                looped: false,
                seek_frame: 0,
            }),
        )
        .unwrap();
    let worker = worker.spawn().unwrap();
    let mut observed = Vec::with_capacity(expected.len());
    let mut buffer = [0; 128];
    let sizes = [1, 13, 64, 7, 31, 2];
    let mut iteration = 0;
    let mut finished = 0;
    let deadline = Instant::now() + Duration::from_secs(5);
    while finished == 0 && Instant::now() < deadline {
        let samples = sizes[iteration % sizes.len()] * 2;
        callback.write_i16(&mut buffer[..samples]);
        for frame in buffer[..samples].chunks_exact(2) {
            if frame != [0, 0] {
                observed.push([frame[0], frame[1]]);
            }
        }
        while let Some(event) = control.try_event() {
            match event {
                Event::Command { result, .. } => result.unwrap(),
                Event::Finished { .. } => finished += 1,
            }
        }
        iteration += 1;
        std::thread::yield_now();
    }
    assert_eq!(finished, 1);
    assert_eq!(observed, expected);
    assert_eq!(control.snapshot().device_errors, 0);
    assert!(control.snapshot().queued_frames <= 128);
    worker.shutdown().unwrap();
}
