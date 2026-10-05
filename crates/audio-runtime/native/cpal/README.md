# CPAL device workspace

This isolated package pins `cpal = "=0.15.3"` and connects its actual native output
stream to the [bounded native transport](../README.md). It owns device selection,
format/buffer negotiation, stream play, error classification, stream closure and
explicit reopen. It does not generate PCM inside the data callback.

## Linux build and virtual-device gate

Required runner packages: `pkg-config` and `libasound2-dev`, plus the normal Rust
linker/build tools. The local offline cache does not contain CPAL, so the full
device dependency graph and `Cargo.lock` must first be generated in the allowed
CI environment. The transport's separate lock already supports local Rust 1.75
tests. Use the engine runner's Rust 1.95 toolchain for the first device build;
this does not claim that every newly resolved platform dependency supports 1.75.

```sh
# First enabled CI run: retain the resulting Cargo.lock as an artifact and
# commit that exact graph before subsequent locked verification.
cargo test --manifest-path crates/audio-runtime/native/cpal/Cargo.toml

# Once Cargo.lock exists:
cargo test --locked --manifest-path crates/audio-runtime/native/cpal/Cargo.toml
bash crates/audio-runtime/native/cpal/run-alsa-null.sh
cargo fmt --manifest-path crates/audio-runtime/native/cpal/Cargo.toml -- --check
```

`run-alsa-null.sh` sets an isolated `ALSA_CONFIG_PATH` to this package's
`alsa-null.conf`. It routes the process's default ALSA output to the null plugin.
It does not change a user's global audio configuration or need a physical sound
card. The `native_smoke` example uses real CPAL data callbacks to consume a
bounded fixture decoded by C's PCM codec and mixed by the actual NativeMixer.
Its deadline-bounded checks require:

- Two one-shot voices acknowledged and completed once after callback consumption.
- A live loop that produces repeated callback copies.
- Continued native callback activity while software-suspended copying stabilizes.
- The same loop copying again after resume.
- Stop/reset during that live loop, rejection of its old session and no replay
  or late natural completion when output resumes.
- No latched device error.

The final stdout line is JSON; device details go to stderr. Every success report
sets `physical_output_verified` to false. The ALSA null plugin can consume much
faster than wall-clock playback, so its callback count and underrun count are
functional execution evidence, not physical latency or real-time performance.

To exercise the operating system's default output instead, run the example
directly with no test `ALSA_CONFIG_PATH`. Its short synthetic fixture is quiet but
potentially audible. Record actual device, rate, buffer bounds, OS and output
observation separately; a successful process exit alone cannot prove speakers
were connected or audible.

## Supported policy and recovery

Stereo F32, PCM16 and U16 formats are supported. Device rate selection is explicit
and the worker performs all resampling. Buffer negotiation requires an advertised
range and requests a fixed whole-device buffer no larger than either the ring
or callback budget. Unknown ranges and incompatible minima return an error
before stream creation. This is intentionally an explicit compatibility limit;
Windows, macOS and physical-device behavior have not been established by a Linux
null-device run.

`NativeOutput::suspend` uses the transport's software gate and leaves callbacks
active. `reopen(self)` drops the old stream before its worker, selects a fresh
device with the original requested limits, and issues a new instance/session.
It does not silently retry device failures or resurrect old commands. The caller
must rebind current presentation samples and cues after a successful reopen.

Primary API references used by this adapter:

- [Pinned CPAL usage and callback model](https://docs.rs/cpal/0.15.3/cpal/).
- [Pinned device/stream trait source](https://docs.rs/cpal/0.15.3/src/cpal/traits.rs.html).
- [Configuration range and buffer limits](https://docs.rs/cpal/0.15.3/cpal/struct.SupportedStreamConfigRange.html).
- [Pinned ALSA backend](https://github.com/RustAudio/cpal/blob/v0.15.3/src/host/alsa/mod.rs).
- [Device-disconnect and backend error variants](https://docs.rs/cpal/0.15.3/cpal/enum.StreamError.html).
