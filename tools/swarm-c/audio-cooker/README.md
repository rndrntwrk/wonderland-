# Bounded audio cooker and source comparison

This directory supplies an offline PCM16-WAVE cooker and a synthetic differential probe against the original FreeSO XA/UTK C# sources. The portable Rust decoder is built by `crates/audio-runtime`. It implements integer PCM, XA speech/music, and mono UTK; MP3 is an explicit external-codec capability.

No game audio payload is included. Every command must receive a source inside an authorized root and an explicit provenance value: `synthetic`, `authorized-import`, `licensed-replacement`, or `redistributable-pack`.

## Build

From the repository root:

```sh
cargo build --offline --manifest-path crates/audio-runtime/Cargo.toml --bin audio-decode
```

The workspace is independent. With no `CARGO_TARGET_DIR` override, the decoder is at `crates/audio-runtime/target/debug/audio-decode`. Supply its absolute path to commands below. This session used:

```sh
CARGO_HOME=/workspace/scratch/cb8e2814717b/toolchain/cargo CARGO_TARGET_DIR=/workspace/scratch/cb8e2814717b/toolchain/swarm-c-target-audio cargo build --offline --manifest-path crates/audio-runtime/Cargo.toml --bin audio-decode
```

## Cook PCM WAVE

```sh
python3 tools/swarm-c/audio-cooker/cooker.py /authorized/input.wav /output/cooked.wav \
  --root /authorized \
  --provenance authorized-import \
  --decoder /absolute/path/to/audio-decode
```

The standard-library WAVE reader provides integer PCM metadata (8/16/24/32-bit, mono/stereo); the Rust decoder converts to PCM16. A successful run exclusively creates `cooked.wav` and `cooked.wav.json`. Existing output or sidecar files are refused. Publication failure removes any files created by that attempt; this is not a filesystem-atomic rename of a two-file transaction.

The JSON sidecar records source-relative name, source/output SHA-256, measured bytes, sample rate, channels, frames, PCM byte length, duration, normalized metadata, selected backend, external version if used, and declared provenance. Failed or unsupported decoding never creates a success sidecar or placeholder samples.

## Cook XA or UTK from normalized metadata

B owns audio headers and metadata parsing. The cooker accepts B-normalized JSON and does not copy B's XA/UTK parser. A mono synthetic XA metadata example is:

```json
{
  "encoding": "XaSpeech",
  "format": {
    "sample_rate": 22050,
    "channels": 1,
    "bits_per_sample": 16
  },
  "sample_frames": 28,
  "payload_offset": 24,
  "payload_bytes": 15
}
```

Pass it explicitly:

```sh
python3 tools/swarm-c/audio-cooker/cooker.py /authorized/sample.xa /output/sample.wav \
  --root /authorized \
  --provenance authorized-import \
  --metadata /authorized/sample-metadata.json \
  --decoder /absolute/path/to/audio-decode
```

Supported compressed metadata encodings are `XaSpeech`, `XaMusic`, and `Utk`. The declared encoding must match the source signature. The payload offset/length is checked against the bounded input, and the decoded frame count must match. XA validates complete block capacity and supports mono/stereo. UTK requires mono PCM16 and follows the original bitstream tables, arithmetic order, reflection/excitation/pitch/synthesis history, half excitation and ties-to-even rounding.

The lower-level decoder accepts only already normalized dimensions and spans:

```text
audio-decode xa-speech|xa-music|utk|pcm RATE CHANNELS WIDTH FRAMES OFFSET LENGTH INPUT OUTPUT
```

`WIDTH` is bits per input sample. `pcm` consumes the raw PCM payload span, not a duplicate RIFF parser. The library type is `codec::SampleMetadata` (`frames` field); the cooker translates the provider's `sample_frames` JSON field into that CLI argument.

## Explicit MP3 capability

The portable Rust decoder returns `Unsupported` for MP3. To cook a bounded MP3 file with an already installed external executable:

```sh
python3 tools/swarm-c/audio-cooker/cooker.py /authorized/music.mp3 /output/music.wav \
  --root /authorized \
  --provenance authorized-import \
  --decoder /absolute/path/to/audio-decode \
  --ffmpeg /usr/bin/ffmpeg
```

This does not install or bundle FFmpeg. The input must have MP3 bytes and FFmpeg is invoked with a forced MP3 demuxer, preventing an arbitrary playlist/container from referencing other files. The backend and actual version string are recorded. Browser hosts may independently use their platform's `decodeAudioData` MP3 capability.

The source repository's bundled MP3Sharp license file and source-header notices conflict (LGPL-3 versus GPL-2-or-later). No MP3Sharp implementation was translated or bundled. External FFmpeg/FFplay distribution and build obligations remain those of the host. XA/UTK translation carries the original SimsLib MPL-2.0 attribution.

## Bounds and process ownership

Defaults/hard ceilings are 32 MiB encoded input and 64 MiB output. Runtime decode also limits output frames to 16 million and UTK unary scans to 4,096 bits. Normalized metadata is limited to 64 KiB. Input is read once into a bounded snapshot after resolving it within the authorized root; symlink escapes are rejected.

External children have a default 15-second deadline, configurable up to 60 seconds with `--timeout`. The adapter uses no shell or interactive stdin, limits diagnostics to 64 KiB, owns a process group, kills/reaps timed-out children, and applies file, CPU, and address-space limits on POSIX. The caller does not receive a success report after timeout, truncation, unsupported format, or output overflow.

For playback of already rendered WAVE files, see [native_player.py](../../../crates/audio-runtime/adapters/native_player.py). That adapter provides bounded buffered-file playback through external FFplay, POSIX pause/resume, stop/deadline/disposal, and explicit capability reporting. It does not implement a live device callback.

## Original decoder differential probe

The primary source files are unchanged `TSOClient/tso.files/XA/XAFile.cs` and `TSOClient/tso.files/UTK/UTKFile2.cs`, from source baseline **4c6b3e8f5835b228723caea3c9f683c62f244f73**. The probe compiles those files with an already installed Mono C# compiler, writes only synthetic input, executes the C# and Rust decoders, and compares complete WAVE output bytes.

```sh
python3 tools/swarm-c/audio-cooker/reference_probe.py \
  --source-root /absolute/path/to/read-only/source-checkout \
  --decoder /absolute/path/to/audio-decode \
  --report /output/audio-reference-report.json
```

Exact command used for the retained result:

```sh
python3 tools/swarm-c/audio-cooker/reference_probe.py \
  --source-root /workspace/scratch/cb8e2814717b/wonderland-swarm-a \
  --decoder /workspace/scratch/cb8e2814717b/toolchain/swarm-c-target-audio/debug/audio-decode \
  --report /workspace/scratch/cb8e2814717b/toolchain/audio-reference-report.json
```

Observed result:

```json
{"passed": 32, "synthetic": true, "byte_equal": true}
```

[reference-evidence.json](reference-evidence.json) retains source hashes and each input/output hash. The 12 XA cases cover mono/stereo, predictor selection, signed nibbles, and multiple-block channel history. The 20 UTK cases cover voiced/unvoiced magnitude paths, custom codes, half excitation, zero-fill and sinc reconstruction, synthesis history, and partial final frames. This is a real source execution comparison; it does not establish exhaustive real-content compatibility.

The source checkout, Mono, FFmpeg, and FFplay are external prerequisites. This tool performs no dependency installation. Missing prerequisites are an explicit gate.

## Tests and observed environment

```sh
AUDIO_DECODER=/absolute/path/to/audio-decode python3 -m unittest discover -s tools/swarm-c/audio-cooker/tests -v
```

The final local run passed **11 tests with no skips**: seven cooker tests, one decoder CLI test, and three native-player tests. MP3 conversion used real synthetic audio with FFmpeg. Native playback used real FFplay with `SDL_AUDIODRIVER=dummy`, deliberately silent.

Verification environment: Rust/Cargo 1.75.0; Python 3.12.14; Mono compiler/runtime 6.8.0.105; FFmpeg/FFplay 6.1.1-3ubuntu5. The adapter reports `physical_output_verified: false`. Qualification with an authorized game-content corpus and physical browser/native devices remains separate.

The runtime and adapter contracts are documented in [the audio crate README](../../../crates/audio-runtime/README.md) and the full source tables in [audio-source-notes.md](../../../docs/swarm-c/audio-source-notes.md).
