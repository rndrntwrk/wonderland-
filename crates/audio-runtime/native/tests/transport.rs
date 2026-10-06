use wonderland_audio_runtime::{
    mixer::{MixerIntent, VoiceId, VolumeGroup},
    pcm::PcmBuffer,
};
use wonderland_native_audio::*;
use wonderland_render_core::AssetKey;

fn config() -> DriverConfig {
    DriverConfig {
        sample_rate: 8,
        max_voices: 4,
        max_pcm_bytes: 128,
        max_queued_pcm_bytes: 128,
        command_capacity: 8,
        event_capacity: 16,
        block_frames: 2,
        ring_frames: 8,
        max_callback_frames: 8,
    }
}
fn sample(values: &[i16]) -> Command {
    Command::InsertSample {
        key: AssetKey([7; 32]),
        pcm: PcmBuffer {
            sample_rate: 8,
            channels: 1,
            samples: values.to_vec(),
        },
    }
}
fn voice(serial: u64) -> VoiceId {
    VoiceId {
        generation: 1,
        serial,
    }
}
fn start(id: VoiceId, looped: bool) -> Command {
    Command::Apply(MixerIntent::Start {
        voice: id,
        sample: AssetKey([7; 32]),
        group: VolumeGroup::Fx,
        gain: 1.0,
        pan: 0.0,
        looped,
        seek_frame: 0,
    })
}
fn submit(control: &mut Control, command: Command) -> Ticket {
    control.try_submit(control.session(), command).unwrap()
}
fn drain(control: &mut Control) -> Vec<Event> {
    std::iter::from_fn(|| control.try_event()).collect()
}

#[test]
fn callback_copies_actual_native_mixer_output_and_silences_underrun() {
    let (mut control, mut worker, mut callback) = channel(config()).unwrap();
    submit(&mut control, sample(&[100, -200, 300]));
    submit(&mut control, start(voice(1), false));
    worker.step();
    let mut out = [999; 8];
    callback.write_i16(&mut out);
    assert_eq!(out, [100, 100, -200, -200, 0, 0, 0, 0]);
    assert_eq!(control.snapshot().underrun_frames, 2);
    assert_eq!(control.snapshot().copied_frames, 2);
}

#[test]
fn completion_waits_for_callback_consumption_and_is_delivered_once() {
    let (mut control, mut worker, mut callback) = channel(config()).unwrap();
    submit(&mut control, sample(&[17]));
    submit(&mut control, start(voice(1), false));
    worker.step();
    assert!(drain(&mut control)
        .iter()
        .all(|e| !matches!(e, Event::Finished { .. })));
    worker.step();
    assert!(drain(&mut control).is_empty());
    callback.write_i16(&mut [0; 4]);
    worker.step();
    assert!(
        matches!(control.try_event(), Some(Event::Finished { voice: id, .. }) if id == voice(1))
    );
    worker.step();
    assert!(control.try_event().is_none());
}

#[test]
fn a_full_frame_ring_does_not_advance_mixer_or_grow_memory() {
    let mut cfg = config();
    cfg.ring_frames = 2;
    let (mut control, mut worker, mut callback) = channel(cfg).unwrap();
    submit(&mut control, sample(&[1, 2, 3, 4]));
    submit(&mut control, start(voice(1), false));
    worker.step();
    for _ in 0..20 {
        worker.step();
    }
    assert_eq!(control.snapshot().queued_frames, 2);
    let mut out = [0; 4];
    callback.write_i16(&mut out);
    assert_eq!(out, [1, 1, 2, 2]);
    worker.step();
    callback.write_i16(&mut out);
    assert_eq!(out, [3, 3, 4, 4]);
}

#[test]
fn reset_discards_old_audio_old_commands_and_old_completions() {
    let (mut control, mut worker, mut callback) = channel(config()).unwrap();
    let old = control.session();
    submit(&mut control, sample(&[123]));
    submit(&mut control, start(voice(1), false));
    worker.step();
    submit(&mut control, sample(&[456]));
    let new = control.reset().unwrap();
    assert_ne!(new, old);
    let error = control.try_submit(old, start(voice(2), false)).unwrap_err();
    assert_eq!(error.kind, DriverError::StaleSession);
    submit(&mut control, sample(&[456]));
    submit(&mut control, start(voice(2), false));
    worker.step();
    let mut out = [0; 4];
    callback.write_i16(&mut out);
    assert_eq!(out, [456, 456, 0, 0]);
    worker.step();
    assert!(drain(&mut control)
        .iter()
        .all(|event| { !matches!(event, Event::Finished { voice: id, .. } if *id == voice(1)) }));
    assert_eq!(control.snapshot().discarded_frames, 2);
}

#[test]
fn suspend_and_resume_freeze_the_unconsumed_buffer() {
    let (mut control, mut worker, mut callback) = channel(config()).unwrap();
    submit(&mut control, sample(&[11, 22, 33, 44]));
    submit(&mut control, start(voice(1), false));
    worker.step();
    control.suspend();
    worker.step();
    let mut out = [999; 4];
    callback.write_i16(&mut out);
    assert_eq!(out, [0; 4]);
    assert_eq!(control.snapshot().queued_frames, 2);
    control.resume().unwrap();
    callback.write_i16(&mut out);
    assert_eq!(out, [11, 11, 22, 22]);
    worker.step();
    callback.write_i16(&mut out);
    assert_eq!(out, [33, 33, 44, 44]);
}

#[test]
fn stop_resets_and_requires_explicit_resume() {
    let (mut control, mut worker, mut callback) = channel(config()).unwrap();
    submit(&mut control, sample(&[55]));
    submit(&mut control, start(voice(1), true));
    worker.step();
    control.stop().unwrap();
    worker.step();
    let mut out = [999; 4];
    callback.write_i16(&mut out);
    assert_eq!(out, [0; 4]);
    assert_eq!(control.snapshot().state, TransportState::Suspended);
    control.resume().unwrap();
    callback.write_i16(&mut out);
    assert_eq!(out, [0; 4]);
    assert_eq!(worker.resident_bytes(), 0);
}

#[test]
fn command_and_pcm_backpressure_return_the_original_payload() {
    let mut cfg = config();
    cfg.command_capacity = 1;
    cfg.max_queued_pcm_bytes = 4;
    let (mut control, mut worker, _callback) = channel(cfg).unwrap();
    submit(&mut control, sample(&[1, 2]));
    let error = control
        .try_submit(control.session(), sample(&[3]))
        .unwrap_err();
    assert_eq!(error.kind, DriverError::QueuedPcmLimit);
    assert!(matches!(error.command, Command::InsertSample { pcm, .. } if pcm.samples == [3]));
    let error = control
        .try_submit(control.session(), start(voice(1), false))
        .unwrap_err();
    assert_eq!(error.kind, DriverError::CommandQueueFull);
    assert_eq!(control.snapshot().queued_pcm_bytes, 4);
    worker.step();
    assert_eq!(control.snapshot().queued_pcm_bytes, 0);
    submit(&mut control, start(voice(1), false));
    worker.step();
}

#[test]
fn a_device_error_is_latched_and_cannot_be_cleared_by_resume_or_reset() {
    let (mut control, mut worker, mut callback) = channel(config()).unwrap();
    submit(&mut control, sample(&[12]));
    submit(&mut control, start(voice(1), true));
    worker.step();
    let signal = control.fault_signal();
    signal.report(DeviceFault::Disconnected);
    signal.report(DeviceFault::Backend);
    let mut out = [999; 4];
    callback.write_i16(&mut out);
    assert_eq!(out, [0; 4]);
    assert_eq!(control.snapshot().fault, Some(DeviceFault::Disconnected));
    assert_eq!(control.snapshot().device_errors, 2);
    assert_eq!(
        control.resume(),
        Err(DriverError::DeviceFault(DeviceFault::Disconnected))
    );
    control.reset().unwrap();
    assert_eq!(control.snapshot().state, TransportState::Faulted);
}

#[test]
fn a_reopened_driver_rejects_a_prior_instance_session() {
    let (old, _, _) = channel(config()).unwrap();
    let old_session = old.session();
    let (mut new, _, _) = channel(config()).unwrap();
    assert_eq!(
        new.try_submit(old_session, sample(&[1])).unwrap_err().kind,
        DriverError::StaleSession
    );
}

#[test]
fn callback_shape_faults_are_silent_and_do_not_consume_pcm() {
    let (mut control, mut worker, mut callback) = channel(config()).unwrap();
    submit(&mut control, sample(&[12]));
    submit(&mut control, start(voice(1), true));
    worker.step();
    let mut odd = [999; 3];
    callback.write_i16(&mut odd);
    assert_eq!(odd, [0; 3]);
    assert_eq!(control.snapshot().fault, Some(DeviceFault::CallbackShape));
    assert_eq!(control.snapshot().queued_frames, 2);
}

#[test]
fn format_conversion_has_exact_silence_and_endpoint_scaling() {
    let (mut control, mut worker, mut callback) = channel(config()).unwrap();
    submit(&mut control, sample(&[i16::MIN, i16::MAX]));
    submit(&mut control, start(voice(1), true));
    worker.step();
    let mut floats = [99.0; 6];
    callback.write_f32(&mut floats);
    assert_eq!(
        floats,
        [-1.0, -1.0, 32767.0 / 32768.0, 32767.0 / 32768.0, 0.0, 0.0]
    );
    worker.step();
    let mut unsigned = [0; 6];
    callback.write_u16(&mut unsigned);
    assert_eq!(unsigned, [0, 0, 65535, 65535, 32768, 32768]);
}

#[test]
fn full_event_queue_retains_completions_and_orders_start_ack_first() {
    let mut cfg = config();
    cfg.event_capacity = 1;
    cfg.max_voices = 1;
    let (mut control, mut worker, mut callback) = channel(cfg).unwrap();
    let insert = submit(&mut control, sample(&[91]));
    let start = submit(&mut control, start(voice(1), false));
    worker.step();
    callback.write_i16(&mut [0; 4]);
    for _ in 0..30 {
        worker.step();
    }
    assert_eq!(worker.pending_completions(), 1);
    assert!(
        matches!(control.try_event(), Some(Event::Command { ticket, result: Ok(()), .. }) if ticket == insert)
    );
    worker.step();
    assert!(
        matches!(control.try_event(), Some(Event::Command { ticket, result: Ok(()), .. }) if ticket == start)
    );
    worker.step();
    assert!(
        matches!(control.try_event(), Some(Event::Finished { voice: id, .. }) if id == voice(1))
    );
    assert_eq!(worker.pending_completions(), 0);
    worker.step();
    assert!(control.try_event().is_none());
}

#[test]
fn finished_but_unconsumed_voice_backpressures_starts_without_reusing_identity() {
    let mut cfg = config();
    cfg.max_voices = 1;
    let (mut control, mut worker, mut callback) = channel(cfg).unwrap();
    submit(&mut control, sample(&[41]));
    submit(&mut control, start(voice(1), false));
    worker.step();
    drain(&mut control);
    let rejected = submit(&mut control, start(voice(2), false));
    worker.step();
    assert!(
        matches!(control.try_event(), Some(Event::Command { ticket, result: Err(DriverError::CompletionBackpressure), .. }) if ticket == rejected)
    );
    callback.write_i16(&mut [0; 4]);
    worker.step();
    drain(&mut control);
    let accepted = submit(&mut control, start(voice(2), false));
    worker.step();
    assert!(
        matches!(control.try_event(), Some(Event::Command { ticket, result: Ok(()), .. }) if ticket == accepted)
    );
}

#[test]
fn newer_voice_generation_invalidates_older_buffer_and_completion() {
    let (mut control, mut worker, mut callback) = channel(config()).unwrap();
    submit(&mut control, sample(&[11, 22]));
    submit(&mut control, start(voice(1), false));
    worker.step();
    submit(
        &mut control,
        start(
            VoiceId {
                generation: 2,
                serial: 1,
            },
            false,
        ),
    );
    worker.step();
    let mut out = [0; 4];
    callback.write_i16(&mut out);
    assert_eq!(out, [11, 11, 22, 22]);
    assert_eq!(control.snapshot().discarded_frames, 2);
    worker.step();
    let events = drain(&mut control);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::Finished { .. }))
            .count(),
        1
    );
    assert!(events
        .iter()
        .any(|event| matches!(event, Event::Finished { voice, .. } if voice.generation == 2)));
}

#[test]
fn pcm_allocation_capacity_is_bounded_not_only_visible_length() {
    let (mut control, _worker, _callback) = channel(config()).unwrap();
    let mut oversized = Vec::with_capacity(1000);
    oversized.push(1);
    let command = Command::InsertSample {
        key: AssetKey([7; 32]),
        pcm: PcmBuffer {
            sample_rate: 8,
            channels: 1,
            samples: oversized,
        },
    };
    assert!(matches!(
        control
            .try_submit(control.session(), command)
            .unwrap_err()
            .kind,
        DriverError::Audio(wonderland_audio_runtime::AudioError::Limit(
            "PCM allocation"
        ))
    ));
    assert_eq!(control.snapshot().queued_pcm_bytes, 0);
}

#[test]
fn dropping_worker_releases_queued_pcm_and_closes_the_callback() {
    let (mut control, worker, mut callback) = channel(config()).unwrap();
    submit(&mut control, sample(&[1, 2]));
    assert_eq!(control.snapshot().queued_pcm_bytes, 4);
    drop(worker);
    assert_eq!(control.snapshot().queued_pcm_bytes, 0);
    assert_eq!(control.snapshot().state, TransportState::Closed);
    let mut out = [999; 4];
    callback.write_i16(&mut out);
    assert_eq!(out, [0; 4]);
    assert_eq!(
        control
            .try_submit(control.session(), start(voice(1), false))
            .unwrap_err()
            .kind,
        DriverError::Closed
    );
}

#[test]
fn oversized_callback_fills_silence_and_faults_without_consuming_the_ring() {
    let (mut control, mut worker, mut callback) = channel(config()).unwrap();
    submit(&mut control, sample(&[12]));
    submit(&mut control, start(voice(1), true));
    worker.step();
    let mut out = [999; 18];
    callback.write_i16(&mut out);
    assert_eq!(out, [0; 18]);
    assert_eq!(
        control.snapshot().fault,
        Some(DeviceFault::CallbackTooLarge)
    );
    assert_eq!(control.snapshot().queued_frames, 2);
}

#[test]
fn native_intent_pause_resume_stop_and_release_reach_the_mixer() {
    let (mut control, mut worker, mut callback) = channel(config()).unwrap();
    submit(&mut control, sample(&[11, 22, 33, 44]));
    submit(&mut control, start(voice(1), false));
    worker.step();
    callback.write_i16(&mut [0; 4]);
    submit(
        &mut control,
        Command::Apply(MixerIntent::Pause { voice: voice(1) }),
    );
    worker.step();
    let mut out = [999; 4];
    callback.write_i16(&mut out);
    assert_eq!(out, [0; 4]);
    submit(
        &mut control,
        Command::Apply(MixerIntent::Resume { voice: voice(1) }),
    );
    submit(
        &mut control,
        Command::Apply(MixerIntent::SetGainPan {
            voice: voice(1),
            gain: 1.0,
            pan: -1.0,
        }),
    );
    worker.step();
    callback.write_i16(&mut out);
    assert_eq!(out, [33, 0, 44, 0]);
    submit(
        &mut control,
        Command::Apply(MixerIntent::Stop { voice: voice(1) }),
    );
    submit(
        &mut control,
        Command::Apply(MixerIntent::Release { voice: voice(1) }),
    );
    worker.step();
    assert_eq!(worker.active_voices(), 0);
    submit(
        &mut control,
        Command::EvictSample {
            key: AssetKey([7; 32]),
        },
    );
    worker.step();
    assert_eq!(worker.resident_bytes(), 0);
}
