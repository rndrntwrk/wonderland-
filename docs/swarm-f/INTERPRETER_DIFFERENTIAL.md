# Original/native/WASM interpreter differential

## Goal and boundaries

Execute the same declared scenarios through the complete original SimAntics
assembly and the shipping sim-core interpreter, both natively and as executed
WebAssembly. This implements the approved continuation of #44, whose execution
contract is `docs/swarm-f/ORIGINAL_VM_BOOTSTRAP.md`.

Base: `2fe667ab037c9a76a129a1fe7c07f1de5cf2450b` (#44).
Branch: `feat/swarm-f-interpreter-differential`; pull request #45.

Only F-owned tests, fixtures, documentation and the original-VM workflow change.
Original assets/source, shipping simulation, client/avatar/terrain/audio files,
shared contracts, root manifests/lockfile/toolchain, and parent branches do not.
No account/database/global-link or EOD production provider is supplied. No merge
or deployment is part of this checkpoint. Local Codex retains application work.

## Execution contract

There are four cohorts, in order: source4110/TS1, source4110/TSO, authored/TS1,
authored/TSO. Each emits 32 post-tick records. The normal C# process runs twice;
the native Rust executable runs twice; the WASM module runs in two fresh V8
workers. All six 128-row traces must match byte-for-byte after only CRLF-to-LF
transport normalization. Original raw stdout is also retained.

All cohorts use GUID `0xf00d0044`, one out-of-world non-avatar entity, an 8x8 lot,
initial attributes `[10,20,30,40]`, UTC start `630822816000000000` and RNG seed
`0x123456789abcdef0`. C# creates the entity immediately before its first tick;
Rust creates it as the first admitted command before tick-one scheduling. Both
execute the actual initialization/main path. Tick one has no additional writes.
Subsequent authored ticks write only attribute 0 (positive when tick%8<4,
negative otherwise). Subsequent source ticks write attributes 0/1/3 to
`[-tick,100+tick,200+tick]`. No emitted state is corrected to match the reference.

The original Content singleton/global cache is a labelled in-memory fixture;
all original VM, VMContext, entity, translator, thread and scheduler methods run
unchanged. C# reuses #44's FixtureDriver and is compiled separately against the
complete original assemblies. Rust links the existing `sim-core`,
`wonderland-content-runtime-bridge` and `wonderland-legacy-formats` rlibs built
with `cargo --locked --lib --no-default-features`. No alternate simulation
algorithm is embedded in either driver.

## Original and authored content stay distinct

The original source routine is BHAV 4110 in
`TSOClient/FSO.Content.TSO/Content/Objects/Casino_2-Tile_Bar_CC.iff`, SHA-256
`20b67e06940bfe0a89b689312fe001721ff3760cce10fca104373bf1122c4865`.
It is bound as Init/Main under declared metadata. It clears attributes 0/1/3
and preserves attribute 2. It is not a complete installed casino-bar object.

`fixtures/reference/interpreter-authored.iff.hex` represents 372 authored IFF
bytes as hexadecimal plus one LF. Decoded SHA-256:
`4d52ba85e96cc6f87d6f14e570837ca7d32a3809123894f93f8f071cff725579`.
Both production decoders consume these same bytes. Main 4096 branches on
attribute 0, calls 4097 on its positive branch or writes -11 on its negative
branch, then sleeps and increments attribute 3. Subroutine 4097 writes +11,
stores local 0=5, adds it to attribute 2 and sleeps before returning. This tests
an observable nested frame/local across a yield, branch results and continuation.
It is authored test content, not extracted or modified game content.

## Comparison and failure controls

The common 16-field projection contains case, mode, logical tick, actual VM
clock ticks, RNG, entity count/ID, all four attributes, stack depth and top-frame
routine, program counter, argument 0 and local 0. Absent frames use -1 sentinels.
It is an explicitly scoped observation, NOT a complete canonical C# VM snapshot.
No row filtering, sorting, floating tolerance or state-value rewriting is used.

Native/WASM shipping `SimRuntime::state_hash()` values are emitted separately.
Their hash files must each contain exactly 128 correctly keyed lowercase SHA-256
records before equality is evaluated. Empty-equal files, truncated data,
reordered/duplicate keys, noncanonical integers and malformed hashes fail.
These hashes establish native/WASM state equivalence for this cohort only; they
are not compared with a C# snapshot hash.

Two compiled Rust test-driver faults execute on both targets. At source/TS1/tick7,
a wrong admitted attribute-2 write must first differ at `attribute_2` (30 vs31).
A skipped actual tick must first differ at `clock_ticks` (7 vs6). These four
controls alter execution/input, not returned output. The skip control naturally
has one repeated native-state hash because that state did not advance. An empty
but valid WASM binary must fail before any trace output. #44's independent
original skip/deschedule/no-opt-in controls remain required.

Parser coverage additionally rejects malformed headers, ranges, framing, missing
or extra rows and identical but vacuous traces. Reference coverage requires both
branch outcomes, nested yielding frames and actual continuation mutations.

## Reproduction and evidence

Run `.github/workflows/swarm-f-vm-bootstrap.yml` on the PR. The Windows job
prepares a raw-Git-verified temporary source copy, restores only declared original
packages plus the reference-only net45 targeting pack, builds the original ten
projects and executes all #44 gates. It then invokes:

```text
python -S tools/replay/run_interpreter.py --root <temporary-source> --output <new-evidence-directory>
```

The runner needs the original DLLs and targeting references laid out by those
preceding workflow steps; it is not a standalone installed-game launcher.
Rust uses the repository's pinned 1.99.0 toolchain. Native Windows and
wasm32-unknown-unknown builds share the shipping libraries. The WASM host supplies
no imports, checks exports/buffer bounds and uses a 20-second worker deadline.
The WASM link caps linear memory at 256 MiB; JS heap limits are not represented
as WASM memory limits. This is V8 execution, not a rendered-browser/device test.

All commands retain exits and stdout/stderr hashes. Runtime sidecars identify the
actual CLR/loaded DLL and executable, rather than the PowerShell launcher. WASM
metadata records Node/V8, module and output hashes. The workflow rechecks every
recorded source input after execution. Its artifact contains only text logs,
identities, JSON and numeric traces, not fonts, game assets, DLLs or packages.

Local validator command (Python 3.11+):

```text
python -S tests/parity/test_interpreter_compare.py
python -S tools/replay/compare_interpreter.py <original.tsv> <candidate.tsv>
```

Comparator exits: 0 exact match; 1 well-formed mismatch with first field; 2 invalid
or incomplete evidence. Native/WASM state hashes are independently validated by
`parse_state_hashes` before the runner's byte equality check.

## Implementation and verification ledger

1. Parser RED/GREEN completed locally: initially 9 tests/34 failures for the
   unimplemented validator; all 9 passed after implementation.
2. Real witnesses, authored fixture and runner published at initial head
   `7ab7301e059fcea4d912b2c65cb266430f24daf4`.
3. Hosted run `37675492895`, job `112977798181`, passed the complete original,
   native and WASM path. All six normal traces matched; all four differential
   execution faults and empty-WASM rejection worked. The workflow rechecked
   6,287 recorded input files with no changes.
4. That run's 103-file artifact `11507252527` was downloaded and independently
   hashed locally: `9f755afae6f1ae5eefcaed26d1d35ff2173da942bd097b55bfde586d748f031a`.
   All 80 differential evidence-file hashes and 24 command exit records were
   checked. Normal traces were re-compared locally; each normal native-state
   file has 128 correctly keyed, distinct hashes. The engines were not rerun
   locally: this environment has Python/Node, but no Rust/.NET toolchains.
5. Review added explicit native-state file validation before equality. Its new
   tests were observed failing, then all 11 comparator tests passed locally.
   Updated parser also accepts all eight real normal/fault hash streams from
   the first hosted run. This validation-only update requires a fresh hosted
   run; use the PR's final verification record rather than extrapolating this
   initial result to an untested head.

Initial common trace SHA-256:
`a0ee3f067322a379c7cb932705a7027c7e7d60037af3931e236b9497d20d1469`.
Initial normal native/WASM canonical-hash-stream SHA-256:
`0f0209e3787e535e444b3e3a7500536866fac2249dc3beab8440c371fe3dfd3d`.

## Remaining acceptance

No complete original content installation, real placed-object cohort, routing,
avatar, authenticated multiplayer, production external result/durable effect,
device/load or release qualification is implied. This is two declared scenarios,
not every primitive/behavior. Further work should broaden the original content
closure and command/effect cohorts, while preserving separate reference identity,
full native/WASM state checks and explicit cross-language observation scope.
