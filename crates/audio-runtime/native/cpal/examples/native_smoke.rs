//! Real CPAL callbacks using a bounded, decoded PCM fixture and actual mixer
//! intents. A null-device run proves this device path, not audible output.
use std::time::{Duration, Instant};
use wonderland_audio_runtime::{
    codec::{decode_sample, encode_wave, DecodeLimits, Encoding, SampleMetadata},
    mixer::{MixerIntent, VoiceId, VolumeGroup},
    pcm::PcmBuffer,
};
use wonderland_native_audio::{Command, DriverError, Event, Snapshot, TransportState};
use wonderland_native_audio_cpal::{NativeOutput, OutputOptions};
use wonderland_render_core::AssetKey;

fn poll_live(output: &mut NativeOutput) -> Result<Snapshot, Box<dyn std::error::Error>> {
    while let Some(event) = output.control().try_event() {
        match event {
            Event::Command { result, .. } => result?,
            Event::Finished { .. } => return Err("live loop unexpectedly completed".into()),
        }
    }
    let snapshot = output.control().snapshot();
    if snapshot.state == TransportState::Faulted {
        return Err(format!("native device fault: {snapshot:?}").into());
    }
    Ok(snapshot)
}

fn wait_live(
    output: &mut NativeOutput,
    condition: impl Fn(Snapshot) -> bool,
) -> Result<Snapshot, Box<dyn std::error::Error>> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let snapshot = poll_live(output)?;
        if condition(snapshot) {
            return Ok(snapshot);
        }
        if Instant::now() >= deadline {
            return Err("native live-loop progress timed out".into());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut output = NativeOutput::open(OutputOptions::default())?;
    eprintln!("native device: {:?}", output.info());
    let rate = output.info().sample_rate;
    let frames = (rate / 8).max(1);
    // Integer saw fixture encoded/decoded through the C PCM codec. This is not
    // an asset-provider or original-game sample claim.
    let source = PcmBuffer {
        sample_rate: rate,
        channels: 1,
        samples: (0..frames).map(|i| ((i % 64) as i16 - 32) * 64).collect(),
    };
    let wave = encode_wave(&source)?;
    let pcm = decode_sample(
        &wave,
        &SampleMetadata {
            encoding: Encoding::PcmWave,
            sample_rate: rate,
            channels: 1,
            frames: u64::from(frames),
            bits_per_sample: 16,
            payload_offset: 44,
            payload_bytes: u64::from(frames) * 2,
        },
        &DecodeLimits::default(),
    )?;
    let session = output.control().session();
    let key = AssetKey([0x43; 32]);
    output
        .control()
        .try_submit(session, Command::InsertSample { key, pcm })?;
    let voices = [
        VoiceId {
            generation: 1,
            serial: 1,
        },
        VoiceId {
            generation: 1,
            serial: 2,
        },
    ];
    for (index, voice) in voices.iter().copied().enumerate() {
        output.control().try_submit(
            session,
            Command::Apply(MixerIntent::Start {
                voice,
                sample: key,
                group: VolumeGroup::Fx,
                gain: 0.125,
                pan: if index == 0 { -1.0 } else { 1.0 },
                looped: false,
                seek_frame: 0,
            }),
        )?;
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut completed = [false; 2];
    while completed != [true; 2] && Instant::now() < deadline {
        while let Some(event) = output.control().try_event() {
            match event {
                Event::Command { result, .. } => result?,
                Event::Finished { voice, .. } => {
                    let index = voices
                        .iter()
                        .position(|v| *v == voice)
                        .ok_or("unknown completion")?;
                    if completed[index] {
                        return Err("duplicate native completion".into());
                    }
                    completed[index] = true;
                }
            }
        }
        if output.control().snapshot().state == TransportState::Faulted {
            return Err(format!("native device fault: {:?}", output.control().snapshot()).into());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    if completed != [true; 2] {
        return Err("native callback completion timed out".into());
    }
    let one_shot = output.control().snapshot();
    if one_shot.callbacks == 0
        || one_shot.copied_frames < u64::from(frames)
        || one_shot.device_errors != 0
    {
        return Err(format!("invalid callback evidence: {one_shot:?}").into());
    }
    let loop_voice = VoiceId {
        generation: 1,
        serial: 3,
    };
    output.control().try_submit(
        session,
        Command::Apply(MixerIntent::Start {
            voice: loop_voice,
            sample: key,
            group: VolumeGroup::Ambience,
            gain: 0.0625,
            pan: 0.0,
            looped: true,
            seek_frame: 0,
        }),
    )?;
    let live = wait_live(&mut output, |snapshot| {
        snapshot.copied_frames >= one_shot.copied_frames + u64::from(frames)
            && snapshot.callbacks >= one_shot.callbacks + 2
    })?;
    output.suspend();
    let mut stable = live;
    let mut stable_since = Instant::now();
    let deadline = Instant::now() + Duration::from_secs(2);
    let suspended = loop {
        let current = poll_live(&mut output)?;
        if current.copied_frames != stable.copied_frames {
            stable = current;
            stable_since = Instant::now();
        }
        if current.callbacks >= live.callbacks + 3
            && stable_since.elapsed() >= Duration::from_millis(20)
        {
            break current;
        }
        if Instant::now() >= deadline {
            return Err("suspended callbacks did not stabilize".into());
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    output.resume()?;
    let resumed = wait_live(&mut output, |snapshot| {
        snapshot.copied_frames >= suspended.copied_frames + u64::from(frames)
    })?;
    // Stop while the loop is still live; it cancels this session and keeps
    // callbacks alive but silent, including after stale buffers are discarded.
    output.control().stop()?;
    let stale = output.control().try_submit(
        session,
        Command::Apply(MixerIntent::Stop { voice: loop_voice }),
    );
    if !matches!(stale, Err(error) if error.kind == DriverError::StaleSession) {
        return Err("old session accepted after stop/reset".into());
    }
    let stopped = wait_live(&mut output, |snapshot| {
        snapshot.callbacks >= resumed.callbacks + 3
    })?;
    std::thread::sleep(Duration::from_millis(20));
    let snapshot = poll_live(&mut output)?;
    if snapshot.copied_frames != stopped.copied_frames
        || snapshot.state != TransportState::Suspended
    {
        return Err("stopped loop continued copying PCM".into());
    }
    output.resume()?;
    let after_reset = wait_live(&mut output, |current| {
        current.callbacks >= snapshot.callbacks + 3
    })?;
    if after_reset.copied_frames != snapshot.copied_frames {
        return Err("reset loop replayed after resume".into());
    }
    println!("{{\"backend\":\"cpal-0.15.3\",\"sample_rate\":{rate},\"callbacks\":{},\"copied_frames\":{},\"underrun_frames\":{},\"completed_voices\":2,\"device_errors\":0,\"live_loop_suspend_resume\":true,\"live_stop_reset\":true,\"stale_session_rejected\":true,\"physical_output_verified\":false}}",
        snapshot.callbacks, snapshot.copied_frames, snapshot.underrun_frames);
    Ok(())
}
