# Original/native/WASM interpreter differential implementation plan

Goal: execute the same declared scenarios using the complete original SimAntics
assembly and the shipping sim-core interpreter, natively and as WebAssembly.

Spec: PR #44's docs/swarm-f/ORIGINAL_VM_BOOTSTRAP.md plus the approved continuation:
align tick boundaries; compare original C#, native Rust and WASM; extend branches,
calls and yields without replacing production simulation code.

Base: 2fe667ab037c9a76a129a1fe7c07f1de5cf2450b (#44).
Branch: feat/swarm-f-interpreter-differential. No parent rewrite or merge.

## Constraints and rulings
- Change only F-owned test/fixture/docs/workflow paths. No client/avatar/terrain/
  audio edits, root dependencies/lockfiles, original asset changes or production
  interpreter edits in this checkpoint.
- Ruling: compare explicitly enumerated observable fields, not an invented
  universal snapshot. Native state hashes are separate from cross-language state.
- Ruling: retain unchanged original BHAV 4110 separately from an authored
  two-routine branch/call/sleep fixture. Never relabel the latter original content.
- Same mode, GUID, initial attributes, UTC seed, 8x8 lot, one out-of-world object.
  Input writes occur before the scheduled tick. No production effect providers.
- Python execution is available for validator tests. Container command execution
  failed, and local Rust/.NET tools are absent. Original/Rust/WASM executions
  will be verified on the isolated hosted runner, with no local execution claim.

## Tasks
1. Write strict trace-parser/differential tests, observe failures, implement parser.
   Test schema, row count/order, bounds, duplicate/missing data, localization,
   malformed framing, and rejection of identical but vacuous traces.
2. Encode a declared IFF fixture; implement separate original-assembly and
   shipping Rust runtime witnesses. Four cohorts: original/authored x TS1/TSO.
   32 post-tick observations per cohort. Original init/main mapping is explicit.
3. Compile native and wasm32-unknown-unknown witnesses against shipping libraries.
   Execute WASM, not just compile. Compare all pairs and repeats. Native fault
   variants omit tick 7 or corrupt the preserved attribute via real input.
4. Retain per-step logs, artifact/input/executable hashes, actual CLR/WebAssembly
   provenance and source-before/after checks. Inspect failures without weakening
   the comparator or changing original/Rust expected data to match.
5. Review the scoped diff, update evidence ledger, publish the PR and handoff.

## Review focus
Fail closed on empty/partial/equal-invalid output; locate first mismatch exactly;
prove actual execution rather than output mutation; preserve baseline tests;
do not imply device/rendering/provider/content-installation or full-game parity.

## Ledger
Pre-flight: the original and Rust production interfaces expose different private
state; the comparison uses one versioned, explicit numeric projection. Both
witnesses will emit it directly from post-tick runtime fields.
Task 1: local RED observed: 9 tests, 34 failures against the missing validator.
Local GREEN: all 9 tests passed after implementation. Node syntax and empty-WASM
rejection also passed. No local Rust/.NET runtime execution is claimed.
Tasks 2-4: witnesses written; hosted qualification pending.

## Authored fixture contract
`fixtures/reference/interpreter-authored.iff.hex` is a 372-byte authored IFF
represented as hexadecimal plus one LF, not extracted or modified game content.
Decoded SHA-256: `4d52ba85e96cc6f87d6f14e570837ca7d32a3809123894f93f8f071cff725579`.
Both production decoders consume the same bytes. Main 4096 branches on attribute 0,
calls 4097 on the positive branch, or writes -11 on the negative branch, then
sleeps and increments attribute 3. Subroutine 4097 writes +11, stores local 0=5,
adds it to attribute 2, and sleeps inside the nested call before returning.
This exercises branches, a persistent nested frame/local, sleep and continuation.
BHAV 4110 remains decoded unchanged from the separately hash-pinned original IFF.

The 16-field common projection contains case/mode/logical tick, actual VM clock,
RNG state, entity count/ID, all four attributes, frame count and top-frame routine,
program counter, argument 0 and local 0. Empty-frame values are -1. It is NOT a
full C# VM snapshot. Native/WASM canonical state hashes are compared separately.
C# console CRLF is converted to LF only at the text transport boundary; raw
stdout is retained. No values are rewritten, sorted, dropped or tolerated.

Run the original-VM workflow on this branch. It builds the original DLL graph,
retains #44's ordinary/negative gates, then runs `tools/replay/run_interpreter.py`
against the same temporary exact-source copy. All compilation products stay
outside tracked source. Evidence is uploaded even on failure. No game assets,
font files, DLLs or restored packages are included in the evidence artifact.
