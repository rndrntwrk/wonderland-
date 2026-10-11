# PR50 actual-playing audio observation — 9 October 2026

Base: `48b3c056905c9c5ba1c01e63222d1776a1e7ac0a`, tree
`3ceef9b35a834e7ac0bc80422ceee786db524015`.

The base's Native browser run 37891792216 passes source/worker export, actions,
avatars, terrain, both retained-pose journeys, receipt deadlines and standalone
audio, then fails the combined journey after reconnect: `No real playing voice`.
The failure is retained, not replaced by the older green component results.

The sound witness waited for `snapshot().activeVoices === 1`. The shipping
BrowserAudio snapshot counts retained entries, including queued/loading samples.
`_start` attaches the real AudioBufferSourceNode, gain and pan nodes later. A
regression using that unchanged shipping adapter with a deferred sample provider
proves the old count can pass while the entry is still `loading` with no nodes.
The initial extracted old predicate fails four of five readiness tests, including
that real-adapter counterexample. This establishes an observation defect, not
proof that every previously missing voice was healthy.

The test now waits, within the existing 12-second observation budget, for exactly
one playing voice, running context, complete source/gain/pan nodes, no pending
starts/decodes and no device errors. Duplicate voices and device errors fail
immediately. A permanently absent or non-playing voice still fails. No product
clock, sound, source cue, device state, action, receipt or network packet changes.

Measurement binds to the exact observed decimal voice ID, asserts the same voice
and graph remain alive through the real 100 ms analyser interval, and disconnects
the analyser even on failure. RMS, mute/volume, existing-voice identity, actual
foreground delay, StopSound and no replay/retry requirements are unchanged.
Readiness poll count and elapsed time accompany actual measurement records.
A low or missing waveform is never retried until it looks correct.

Ten regression tests cover pending/paused states, missing graphs, wrong device
state, duplicates/errors, the real BrowserAudio loading counterexample, delayed
readiness, deadline failure, immediate readiness and actual workflow/call-site
wiring. Virtual time is confined to Node unit tests; browser observation uses
ordinary monotonic time and timers. Native browser CI now runs every
`tools/native-browser/*.test.mjs` and records the two new helper source hashes.

Reproduction:

```sh
node --test --test-reporter=tap tools/native-browser/*.test.mjs \
  apps/web-shell/tests/gpu/*.test.mjs apps/web-shell/scripts/native-*.test.mjs \
  crates/audio-runtime/browser/*.test.mjs
```

The initial local aggregate passes 175 tests, zero failed or skipped. Hosted
results must be attached to their actual new commit; the prior combined failure
is not a pass for this revision. No fresh local Rust or packaged-browser run is
claimed by these Node checks. The existing full read-only Native browser workflow
rebuilds both Rust/WASM targets and exercises source exports and all native
journeys on the same release. Independent review of the latest combined branch
is still required. This is a bounded PR50 acceptance correction, not full C/E,
original-content, physical-device or production multiplayer qualification.
