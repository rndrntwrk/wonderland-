# Continuous native audio callback

This package connects the existing C-local `NativeMixer` and `MixerIntent` API
to a continuously serviced native output callback. The mixer worker owns PCM,
voices, resampling, allocations and completion delivery. The application data
callback reads a bounded atomic frame ring and writes stereo PCM16, F32 or U16.
It performs no decoding, mixing, allocation, deallocation, logging, channel
operation, mutex acquisition or wait.

`native/` is an independent Rust 1.75 workspace with no new registry dependency.
Its generated `Cargo.lock` is checked in. `native/cpal/` is a separate device
workspace pinned to CPAL **0.15.3**; this keeps unavailable platform libraries
from preventing transport tests. Actual device compilation, four configuration
tests and the OS ALSA-null stream passed at
`bc529fd4c08be5473b2da549f0aa80cebe17a45f` in
[native-audio job 111899190176](https://github.com/rndrntwrk/wonderland-/actions/runs/37350305093/job/111899190176).
The job retained the complete device dependency lock and logs; use that lock for
subsequent `--locked` runs. The separate local transport tests still require no
CPAL/ALSA development packages. Neither their success nor the null-device run
proves audible output or physical-device performance.

## Integration

Use `wonderland-native-audio-cpal::NativeOutput::open(OutputOptions::default())`
on the application thread. `output.info()` reports the selected rate, format,
device-buffer bound and transport buffers. `output.control()` exposes the
bounded command and event endpoints:

```rust,ignore
let session = output.control().session();
output.control().try_submit(
    session,
    Command::InsertSample { key: normalized_asset_key, pcm: decoded_pcm },
)?;
for intent in presentation_intents {
    output.control().try_submit(session, Command::Apply(intent))?;
}

while let Some(event) = output.control().try_event() {
    match event {
        Event::Command { ticket, result, .. } => record_presentation_result(ticket, result),
        Event::Finished { voice, .. } => { audio_system.complete_voice(voice); }
    }
}
```

An accepted queue submission is not yet a successful mixer operation. Consume
the corresponding `Event::Command` to observe validation, PCM residency or
voice-budget rejection. Queue saturation returns the original owned command,
including its PCM, for retry without decoding it again. Preserve the original
session token across an asynchronous asset load; never replace a stale token
with the current session to make an old request pass.

`AudioSystem::tick` continues at C's fixed presentation cadence. Neither worker
iterations, device timestamps, callbacks nor completion events advance or
acknowledge Swarm A. A completion can feed `AudioSystem::complete_voice` only.
Provider metadata and decoding remain outside this callback adapter; production
content must still come from the declared normalized asset provider.

For another native backend, `channel(config)` returns one `Control`, one `Worker`
and one `Callback`. Move the unique callback into the backend's data callback,
move `control.fault_signal()` into its error callback, and call `worker.spawn()`
or service `worker.refill()` on an existing audio worker. None of these endpoints
is clonable except the atomic fault signal.

## Ownership and bounds

| Resource | Default | Enforcement |
| --- | ---: | --- |
| Resident decoded PCM | 64 MiB | Existing `NativeMixer` residency checks |
| Queued and in-flight decoded PCM | 16 MiB | Reservation lasts until insertion or rejection finishes |
| Active plus pending natural completions | 128 voices | New starts backpressure before another identity is consumed |
| Command queue | 64 entries | Nonblocking `try_submit`; owned payload returned on saturation |
| Event queue | 256 entries | Worker retains one command reply and bounded pending completions |
| PCM ring | 2,048 stereo frames | All atomic slots allocated once at construction |
| Mixing block | 256 stereo frames | One temporary mixer output allocation per block, on the worker |
| Device callback ceiling | Negotiated device buffer | CPAL tightens the configured ceiling to the selected buffer |

PCM vectors are validated by both initialized length and allocation capacity,
then normalized to exact-capacity ownership before admission. This prevents a
one-frame vector from hiding a much larger retained allocation. The temporary
render block and the mixer's bounded finished-identity vector are additional
worker-owned storage; neither is transferred to the callback. Metadata storage
is bounded by the entry and voice limits.

The ring has one producer and one consumer. A producer publishes packed stereo
samples and epoch tags before releasing the write cursor. The consumer acquires
that cursor, examines at most its initial snapshot of available slots, then
releases the read cursor after deciding whether the block is valid. The producer
cannot reuse a slot until that release. This follows Rust's documented
[release/acquire ordering](https://doc.rust-lang.org/std/sync/atomic/enum.Ordering.html).
Production code forbids unsafe Rust and requires native 64-bit atomics. The only
unsafe code in this package is the test-only allocator wrapper that forwards
requests unchanged to `System` to detect callback allocations and frees.

Each worker burst produces at most `floor(ring_frames / block_frames)` blocks,
even if a virtual device consumes continuously. It rechecks control state at
every block. Rings shorter than **2 ms** are rejected. The worker's park request
is one quarter of ring duration, capped at **1 ms**, preventing a fixed
1,000-block-per-second ceiling for small blocks or high sample rates. This is a
scheduling policy, not a hard real-time guarantee; contention and device behavior
can still cause measured underruns.

The callback first silences the supplied output and then copies valid frames.
An empty ring leaves silence. `underrun_frames` counts missing queued frames
while the mixer expects active voices; ordinary idle silence is not counted.
An odd sample count or callback larger than the configured limit produces silence
and a latched fault. Filling an oversized backend-owned output remains linear in
that supplied slice; the adapter does not allocate a replacement or attempt an
unbounded refill on the callback thread. Statistics are concurrent observations,
not a transactional snapshot or a device-performance verdict.

## Lifecycle semantics

`suspend()` takes effect at callback entry. A callback already in progress is
allowed to return its whole block, and subsequent callbacks remain silent without
consuming queued frames. `resume()` reuses those frames and voice phases. This
portable software gate keeps the native stream running, without relying on every
backend supporting a hardware pause operation.

`reset()` advances a private controller generation and output epoch. It cancels
earlier tickets, queued commands, buffered audio and completion events. The next
worker step clears voices and sample residency while preserving NativeMixer's
retired-voice watermark. `stop()` additionally suspends output until an explicit
resume. A callback that observes a reset during its copy discards that entire
copy. Audio already submitted to a device cannot be recalled.

Per-voice `Pause`, `Resume`, `SetGainPan`, `Stop` and `Release` are sent through
the actual mixer. They affect future mixing blocks; prebuffered samples retain
the bounded transport/device latency. Use global stop/reset when the entire
session must be invalidated. A newly accepted higher voice generation also
invalidates the previous generation's queued PCM and pending completions.

Natural completion waits until the consumer cursor reaches the end of the
mixing block containing the voice's last sample. Its start acknowledgement is
delivered first. This means the PCM has been consumed by the application device
callback, not that a DAC or speaker has physically played the last sample.
Completions are retained under event backpressure and never drive A.

Device errors latch their first small fault code and increment an atomic count.
The application data callback becomes silent and natural completion delivery
stops. Resume or reset cannot clear a hardware fault. `NativeOutput::reopen`
closes the old stream and worker, then selects a fresh device and returns a new
controller instance. Rebind current C presentation assets/cues with its new
session; old instance tokens are rejected. The error callback performs only
atomic classification; CPAL owns and disposes its backend error object. The
zero-heap data-callback assertion does not make a claim about CPAL's internals.

## Device negotiation

The CPAL binding selects an advertised stereo F32, PCM16 or U16 configuration,
preferring the requested rate and then F32. NativeMixer resamples to the actual
selected rate on the worker. It requests `BufferSize::Fixed` within the device's
advertised range and both transport bounds, targeting four mixing blocks. The
default is 1,024 frames when supported. An unknown buffer range or an incompatible
minimum fails before stream construction; no unbounded default-buffer fallback
is attempted. A caller receives `UnsupportedBufferBound` and can choose another
device or a compatible explicit transport budget.

In pinned CPAL's ALSA backend, a fixed value specifies the whole hardware buffer,
and the callback uses the currently available frames. The callback still checks
its negotiated maximum because a backend or plugin can violate expectations.
The original default ALSA buffer can exceed this package's ring at higher sample
rates, which is why the binding negotiates an explicit bound. Sources:
[CPAL 0.15.3 ALSA implementation](https://github.com/RustAudio/cpal/blob/v0.15.3/src/host/alsa/mod.rs),
[ALSA buffer-size API](https://www.alsa-project.org/alsa-doc/alsa-lib/pcm_2pcm_8c.html),
[CPAL configuration ranges](https://docs.rs/cpal/0.15.3/cpal/struct.SupportedStreamConfigRange.html),
[CPAL stream API](https://docs.rs/cpal/0.15.3/src/cpal/traits.rs.html).

## Verification

From the repository root:

```sh
cargo test --locked --manifest-path crates/audio-runtime/native/Cargo.toml
cargo test --locked --release --manifest-path crates/audio-runtime/native/Cargo.toml
cargo fmt --manifest-path crates/audio-runtime/native/Cargo.toml -- --check
```

The transport suite covers concurrent variable-size callbacks, exact frame order,
stereo atomicity, ring saturation, fixed-capacity payload ownership, completion
and command backpressure, high-rate refill, minimum buffering, exact sample
conversion, per-voice intents, private session reuse, and deterministic
reset/suspend/fault races at callback commit. Its allocator instrumentation checks
both allocation and deallocation across normal, underrun, stale, suspended,
faulted and malformed-buffer paths.

The [CPAL workspace instructions](cpal/README.md) describe the passing real
ALSA-null stream smoke and its required development packages. It exercised actual
OS callbacks, one-shot and loop consumption, silent software suspension, resume,
live stop/reset and stale-session rejection without a latched device error.
Its physical-output flag remains false. Physical output, hot unplug on physical hardware,
latency/underrun targets under production load, and platform configurations with
unknown buffer ranges remain separate validation or implementation work.
