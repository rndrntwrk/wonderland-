# Swarm B buildout verification and handoff

The cooperative-game, sprite-workflow, OBJf and cooked-runtime buildout passed
the complete fresh verification run on **2026-10-05: 47 gates, 439 Rust tests
and 48 Python tests**. No gate failed. Every independently reviewed actionable
finding was corrected before this run. The verification inputs remained
unchanged from the beginning to the end of the run.

The exact runner output is [buildout-verification.json](buildout-verification.json).
All command logs, original-handler traces and source-oracle evidence are retained
in [buildout-evidence](buildout-evidence/). Log and details paths in the machine
record are relative to that evidence directory; command arrays retain the
original run's absolute paths. This document supersedes the historical
[44-gate continuation](CONTINUATION-VERIFICATION.md); the initial
[35-gate record](VERIFICATION.md) also remains available.

## Delivered increments and publication

The four feature PRs form a sequential draft stack above the previously
published runtime-and-authoring continuation, PR #11. Each was fetched back
after publication; its Git tree exactly matches the reviewed local tree.

| PR | Increment | Published commit | Target branch |
| --- | --- | --- | --- |
| [#13](https://github.com/rndrntwrk/wonderland-/pull/13) | Bounded OBJf semantics and exact lifecycle inventory | `bb4258be8b45aaa22b325a36824f774e39063f59` | `swarm-b/runtime-and-authoring` |
| [#14](https://github.com/rndrntwrk/wonderland-/pull/14) | Guarded indexed sprite authoring | `75c53d1e853df21152261ecb5bf44ddb1088527c` | `swarm-b/objf-lifecycle-catalog` |
| [#15](https://github.com/rndrntwrk/wonderland-/pull/15) | Cooperative native EOD games and original-handler oracle | `c322e402d5b2c16cc18ed21c87dd4fecf82c28e2` | `swarm-b/guarded-sprite-workflow` |
| [#16](https://github.com/rndrntwrk/wonderland-/pull/16) | Verified cooked releases and actual runtime replay | `9a1fe5369adfe456c400e6113750733d9c175bb8` | `swarm-b/cooperative-eod-games` |

The final handoff branch, `swarm-b/buildout-verified`, adds the aggregate runner
integration, extends input-stability coverage to four regressions, and adds this
record and the evidence files above PR #16. The stack remains draft and unmerged.

| Immutable reference | Value |
| --- | --- |
| Original source baseline | `4c6b3e8f5835b228723caea3c9f683c62f244f73` |
| Previous published B continuation | `04f0a407dd81acf0685453e7367764bf75b5c092` |
| Actual Swarm A simulation | `8a0e251d19e222a0a6833d7408ca629f674e1729` |
| Aggregate verification-input SHA-256 | `0616fa54d752f6373023ebe1c6b356a7092316dbecb63f6c031099fc76376996` |
| Published feature tree through PR #16 | `9846392b88686493a7e3dd0ac5a3190d80ea949e` |

Original C# and assets, A-owned simulation, shared contracts and root workspace
files are unchanged by this buildout. The separate runtime assembly exports the
exact pinned A subtree and links B-owned sources; it does not replace the VM.

## Reproduce the complete run

Requirements are Rust/Cargo 1.90.0 with rustfmt and Clippy, Python 3, Mono
(`mcs` and `mono`), a GNU C++98-capable compiler, Node with `node:wasi`, and the
`wasm32-unknown-unknown` and `wasm32-wasip1` Rust targets. The recorded environment
used Node 24.19.0 and Mono 6.8.0.105.

```sh
rustup toolchain install 1.90.0 --component rustfmt --component clippy
rustup target add --toolchain 1.90.0 wasm32-unknown-unknown wasm32-wasip1
git fetch --no-recurse-submodules origin 8a0e251d19e222a0a6833d7408ca629f674e1729

# Needed once when the bridge dependencies are not already cached.
python3 tools/swarm-b/runtime-bridge.py /tmp/wonderland-runtime-deps fetch --locked

bash tools/swarm-b/verify.sh \
  --with-parity --with-source-oracle --with-runtime-bridge \
  --logs-dir /tmp/wonderland-b-buildout-verification
```

Run from the repository root and choose an unused dependency-assembly path.
`--cargo /absolute/path/to/cargo` selects a specific executable. The aggregate
selects toolchain 1.90.0, uses fresh temporary build directories per package,
disables incremental compilation and debug information, and removes only its
own temporary builds. Runtime checks use locked offline dependencies after
preparation. The log directory retains the result and commands.

The input digest covers B code, package locks, authored tests and fixtures,
compatibility metadata, the actual flat original IFF corpus, Iffinator archive,
SPR2 reader/writer and FAR3 sources. It now also covers **all 18 original EOD
handler/framework dependencies** read by the oracle. The digest is compared
before and after all 47 gates. Four regressions exercise mutation, addition and
removal, exact EOD dependency membership, and changed bytes behind an original
source symlink. Documentation under `docs/swarm-b` and build outputs are not
verification inputs, allowing the final evidence to be recorded without a
self-referential digest.

## Exact test accounting

The counts below come from successful test summaries in the retained command
logs, rather than inferred test names or overlapping all-target runs.

| Package or explicitly executed harness | Rust tests passed | Ignored in that invocation |
| --- | ---: | ---: |
| Formats standard suite | 96 | 3 |
| Indexed IFF explicit original corpus | 2 | 0 |
| Content resolution and packs | 14 | 0 |
| Actual manifest module | 7 | 0 |
| Actual interaction module | 55 | 0 |
| Native EOD ordinary package suite | 109 | 0 |
| Original EOD example boundary regressions | 3 | 0 |
| Creator library and executable workflow | 47 | 0 |
| Asset cooker | 24 | 0 |
| Actual content/runtime bridge | 82 | 0 |
| **Total** | **439** | **3** |

Python contributes **14 catalog tests, 9 assembly integrity/path tests,
4 verification-input tests and 21 EOD comparator/protocol tests: 48 total**.
The EOD CLI independently gates the same comparator suite internally; the
aggregate counts it once. Cargo's ordinary package test command does not run
the source-oracle example's unit tests. Those three are explicitly executed
and counted once from `native-boundary-tests.stdout`.

Three compile-fail documentation tests are included in the Rust total: one
private EOD boundary and two creator invariants. The formats suite intentionally
ignores three corpus-dependent tests; the source oracle explicitly runs two.
The older optional semantic-corpus test remains separate, while the pinned
content census is regenerated and compared in full in this run.

All package formatting and strict Clippy gates pass. Formats, content,
interactions and the runtime bridge's library compile for
`wasm32-unknown-unknown`. The authoritative EOD library correctly rejects that
target with its native-only diagnostic. The expected compiler rejection is a
passing boundary gate, distinct from an unexpected build failure.

## Cooperative games and private recovery

[cooperative-eod.md](cooperative-eod.md) defines the delivered native contract.
PaperChase, PizzaMaker and TwoPersonJobObjectMaze now run through `NativeHost`
with one bounded shared private group per game, fixed participant roles and a
separate recorded controller capability. A group advances once per host tick.
UI messages cannot select trusted identities, controller callbacks, tuning or
private random seeds.

| Handler | Source-derived behavior covered |
| --- | --- |
| PaperChase | Three-role lobby, choice retention, source combination order, immediate and repeated final packed-letter events, seven-field view, strict 421/91-tick transitions |
| PizzaMaker | Four stations, original constructor deck, private inherited hands, contribution/reinsertion order, automatic choices, recipe selection, signed timer tuning, no-op close and typed controller callbacks 6/7/8 |
| TwoPersonJobObjectMaze | Two private roles, 8×36 generation and BFS order, source color-pool and wall encoding, Logic-only map/exit/solution, Charisma-only cell, last-join cooldown and queued round/reaction transitions |

Format 3 stores group state, role references and random streams. Existing
formats 1/2 remain accepted on their existing paths. Restored groups are
detached and paused until the recorded controller and every retained role
rebind. Controller teardown and lost-role recovery aborts revoke attachments
and clean up private delivery. Typed callbacks check scope, epoch, controller
and readiness before execution.

Join, command, callback and tick effects are admitted with their candidate
state. A failed admission cannot consume a card, advance a stream, retain a
new role or publish partial outputs. The **28 cooperative acceptance tests**
exercise role authority, once-per-group timing, private output, source phase
behavior, recovery and rejection paths through the real host.

Native random streams use explicitly recorded SplitMix64 behavior. These tests
do not assert C# `System.Random` sequence identity. The named phase,
initialization, rejoin, controller and transactional-admission policies remain
explicit. An actual authoritative 30 Hz VM integration and original UI session
are separate qualification gates.

## Original C# EOD component comparisons

The [oracle contract](eod-source-oracle.md) and
[machine report](buildout-evidence/eod-source-oracle/eod-source-oracle.json)
record execution of **18 unchanged, hash-pinned C# source files**. The same
authored scenarios execute through Rust `NativeHost`. Adapters supply explicit
VM, private transport, storage and controlled FIFO task scheduling boundaries.

| Original handler | Scenarios | Common stages | VM effects | Private UI effects | Logical provider effects |
| --- | ---: | ---: | ---: | ---: | ---: |
| Timer | 6 | 53 | 27 | 25 | 0 |
| DanceFloor | 2 | 21 | 13 | 9 | 0 |
| Signs | 7 | 50 | 21 | 28 | 23 |
| Scoreboard | 4 | 35 | 18 | 25 | 20 |
| PermissionDoor | 8 | 64 | 27 | 29 | 18 |
| **Total** | **27** | **223** | **106** | **116** | **61** |

There are **226 complete stages**: 223 common stages and three fully asserted
native-policy differences. The differing cases cover pre-initialization Signs
writes, malformed UTF-8 Signs input and malformed persisted Scoreboard data.
They require exact source and native outcomes and no unexplained residual
effects. The comparator additionally checks **77 literal channel assertions**.

VM/provider effects retain global order within each stage; private messages
retain order per recipient, reflecting the independent delivery channels.
Original and native traces each repeat twice identically. All nine mutations
reject: changed VM code, private bytes or kind, changed save bytes, reordered
VM/private effects, a dropped stage/provider effect and a misrouted recipient.

The native adapter completes its drain/checkpoint/provider boundary even when
the main operation returns an expected error. A provider error still attempts
the final output drain; primary and completion errors remain visible. Three
actual-host regressions ensure a queued disconnect, a no-UI Scoreboard save or
an earlier successful provider response cannot disappear behind an error.

- Oracle input SHA-256: `473fddace7f76296116405ca648ddcb7c42bbe0bcafaac2f755f9acd32ac130d`.
- Common comparison SHA-256: `599b98108be8c7d4cf7460ee4c9525c84a3b6147c80a20c8302caf54ea96cb15`.

The oracle qualifies handler effects under controlled adapters. It does not
qualify the original application/UI, concurrent live invocations, real
LotServer callback failure handling, VM/private checkpoint atomicity or a
durable production provider. The three new cooperative games have
source-derived native acceptance evidence; they are not part of this
five-handler executable C# comparison.

## Guarded sprite authoring

[creator-sprites.md](creator-sprites.md) documents all five commands:
`sprite-export`, `sprite-import`, `sprite-pixel`, `sprite-palette` and
`sprite-alpha-mode`. The strict schema-1 package binds the whole source IFF,
SPR2 resource and required same-IFF palettes with SHA-256 guards. It retains
ordered frame identities, indexed color, straight alpha, optional depth and
source metadata.

Bounded dimension, position and supported channel edits are allowed with
matching planes. Unknown/duplicate fields, positional structures, missing
required-null depth, wrong dependencies and unsupported flags reject. Frame
count/order and palette identity/size remain fixed. The package ceiling is
16 MiB plus stricter caller limits; allocation admission includes retained
document/package/candidate state, the writer and reopening workspace.

An import validates one source snapshot, encodes changed resources, rebuilds
the original-aware resource map, reopens the candidate and publishes atomically.
Stale guards and late failures leave source, destination and in-memory state
unchanged. No-op and palette-only edits preserve exact SPR2 bytes. Explicit
source-style alpha quantization reports changed samples and is applied before
the no-op comparison; arbitrary alpha is not silently rounded.

The **47 Creator tests**, including 17 new authoring cases, cover both SPR2
versions, multiple palettes, transparent index 257, geometry/channel edits,
quantization boundaries, malformed packages, stale guards, output failure and
indexed map rebuilding. The acceptance fixture invokes the actual cooker:
dependent cache keys change after authoring while unrelated payloads and an
independent-source group provide unchanged controls. This proves recooked
identity propagation; live-renderer cache eviction is separate work.

## OBJf semantics and exact lifecycle inventory

[objf.md](objf.md) describes the bounded lossless decoder/encoder and semantic
dispatcher integration. Supported headers, condition/action ordering,
padding, unusual versions and trailing bytes survive codec round trips.
The existing cooker admits this semantic variant through its normal critical
resource path. Runtime import retains the actual VM's narrower 256-function
and exact-payload restrictions.

The regenerated [content corpus](../compat/content-corpus.json) retains
**471 OBJf tables and 14,685 entries**. Lifecycle selection now honors
`UsesFnTable`: required missing or ambiguous OBJf tables remain unresolved
instead of falling back to OBJD entrypoints. Raw identity multiplicity and
exact resource ordinals preserve which table is being referenced.

Independent review reconstructed all tables from source bytes and checked
**1,820 cohort references and 1,817 matrix entrypoints**. The generated census
checks and 14 catalog tests pass in the aggregate. The content-corpus bytes have
SHA-256 `f5448fac68278e237b36b5a30705395b58b30152df4042cfca67d0eb8ab2878f`.

The denominator remains the pinned checked-in source: 745 IFF/PIFF paths,
730 distinct hashes, 1,327 OBJD instances and 1,602 object leaves. Authored test
fixtures do not expand that denominator. All object gameplay evidence remains
unverified; reference resolution and executable gameplay are distinct gates.

## Cooked releases and actual native/WASI runtime execution

[cooked-runtime.md](cooked-runtime.md) defines the complete workflow:

1. `release-prepare` verifies selected content and seals an explicit draft into
   a binding containing the actual expected A content/tuning descriptor.
2. `release-plan` verifies the independently selected binding and manifest,
   then returns required packs and dependency selection.
3. `release-load` verifies complete selected pack membership, hashes, codecs,
   criticality, scopes, provenance and tuning before semantic conversion.
4. `release-replay` runs an explicit bounded scenario through the actual pinned
   simulation, isolated query, snapshot restore and accepted ticks.

The binding records source/runtime revisions, the digest of the exact manifest bytes, explicit
scope/resource order, object and namespace identities and normalized metadata.
Unsealed input cannot yield executable content. Load recomputes and compares
the mandatory runtime descriptor. Source and cooked provenance reports remain
distinct; the cooked path does not manufacture original IFF encounter order or
whole-file identity. The CLI rejects observed symlinks, parent traversal,
nonregular inputs and pre-existing binding destinations under its stable-path
contract.

The new parity driver cooks real authored IFF/PIFF and layered tuning, removes
original source files, omits unrelated packs, and runs from selected verified
packs. WASI preopens only the cooked release. Literal results are checked before
cross-target equality; equal but wrong results cannot pass.

| Actual runtime probe | Original-source routine | Cooked authored release |
| --- | --- | --- |
| Query | Source BHAV 4110 returns true in 3 instructions | Returns true in 3 instructions with temps 37/41 |
| Query input snapshot | Unchanged | Unchanged |
| Accepted replay | Completed tick 1→2; attributes `[0,0,30,0]` | Completed tick 2→3; attribute `[41]` |
| Snapshot/state agreement | Native/WASI and authority/replica agree | Native/WASI snapshots and state agree |
| Effect dispatch | 0 durable replica dispatches | 0 dispatches |
| Negative controls | 2 rejected | 3 rejected, including corrupt selected pack |

Exact results are retained in
[runtime-replay.json](buildout-evidence/runtime-replay.json), with their gate
logs. The cooked state hash is
`d73de0a5492c86856824210438b57324e557f90c5bc3cc0ddda22697003cab56`;
its snapshot SHA-256 is
`621ce538f35e35a782fe2399702ba01fac12c7bb0c3dbee9af9b2d2f23e77813`.
The original-source state/snapshot hashes remain
`d52e0091425b12e7d820dcb7f1b883fb36606059b7a121bb5cf9218cc272f62b`
and `49094c9adc9b1e3df73264ec221c323ce0027ec231af4f3e38b2b006fe0ca43d`.

The **82 bridge tests** also cover private→semiglobal→global calls, owner-specific
tuning, strict metadata, malformed/mixed bindings, budgets and actual Creator
inspection. Conversion's shared 32 MiB admission cap is separate from host input
pack buffers and runtime snapshot allocations; it is not a process-memory cap.
The public harness uses explicit Replica fixture state and complete accepted
ticks. Full queue/check-tree providers, authenticated server admission,
instruction stepping and executed-instruction traces remain integration work.

## Retained source and portability evidence

The buildout reruns the earlier source and cross-target evidence:

| Gate | Fresh verified result |
| --- | --- |
| Original-aware indexed IFF writer and historical C++ reader | 198 indexed originals; 195 exact passthroughs; 3 strict duplicate rejections; 192 editable maps; 3 ambiguous-map edit rejections; 13,084 supported entries; 960 v0 and 5 source-derived v1 edits; 302,174,293 emitted bytes checked |
| Unchanged C# SPR2 encoder | 257 vectors, including 254 nontrivial alpha inputs; command-stream SHA-256 `fc27c24370758905ed03846d98ff9862b2bf89cd421c84d6a713d30e49640ce0` |
| Original SPR2 reader methods | 18 cases, 34 frames and 294,034 decoded pixels; extracted-method SHA-256 `0d5a24fe4449de91f4413dba38208d5ee3a134ed614beb10bdbb4a0ae97479ba` |
| Unchanged C# FAR3 reader | Source Persist/QFS authored 73-byte vector decodes to `abcdefg` |
| Authored native/WASI content/interaction probe | All 17,150 canonical bytes match; SHA-256 `42fe83da0cace7c343fd3736713893014187bc593cecb8694b8948c25ce15782`; altered-opcode control rejects |
| Deterministic cooker | Demo, independent recook and digest-selected verification pass; manifest and all pack bytes match |

The Iffinator archive lacks a header and uses one obsolete allocation spelling;
its harness supplies declarations/accessors and that compilation adjustment
while retaining the historical parsing/validation bodies. The SPR2 reader
comparison executes four unchanged extracted methods; the writer compiles the
complete original encoder with simple frame/color containers. Existing C#
compiler warnings remain visible in the evidence. These are codec/component
comparisons, with rendering, presentation and full-client integration still
outside their scope.

## Independent review and corrections

All six buildout tasks received independent review. The following concrete
findings were corrected and rechecked before the final aggregate:

| Finding | Correction and evidence |
| --- | --- |
| Malformed duplicate OBJf or absent ordinal could leave a false lifecycle selection | Require raw and decoded identity uniqueness and exact source ordinals; independent source reconstruction and focused catalog regressions pass. |
| Pizza recovery admitted unreachable deadlines or inactive fractional frames | Validate each phase against signed tuning and require canonical inactive frames; legitimate zero/negative tuning still restores. |
| Returned Pizza contributions could exceed the actual pool multiset | Check that pool counts cover every returned contribution. |
| Maze recovery could send a private view that had never initialized | Respect saved per-role initialization flags during rebind. |
| Full PaperChase Lobby restored into a state that could not start | Require incomplete Lobby and full later-phase rosters; valid zero/one/two-role lobbies still restore. |
| The final RNG draw could produce a successful but unrestorable state | Reject before the reserved terminal count; repeat failure preserves checkpoint state, outputs and identity counters. |
| Maze pre-play position could disagree with its recorded origin | Require Waiting/Ready position to equal solution origin; valid moved Solving/Reacting checkpoints remain accepted. |
| An expected oracle error hid queued disconnects, writes or earlier provider output | Always complete the stage boundary and final drain; preserve both primary/secondary errors; three real-host regressions and the reviewer's unchanged fault injection pass. |
| Opening a FIFO before checking file type could hang the cooked CLI | Check the regular leaf before open and check metadata again afterward; both FIFO binding and digest-named pack independently reject promptly. |
| Aggregate input tracking and test counts needed the new oracle | Bind all 18 actual original dependencies; count the explicitly run three example tests once; four stability regressions and independent Cargo metadata inspection pass. |

Creator's exact 11-file reviewed snapshot matched the frozen implementation;
the reviewer independently reran all 47 tests and inspected strict parsing,
allocation accounting, atomicity and recooking. The cooked review inspected
binding trust, complete dependency admission, scope/tuning correlation,
allocation bounds and the parity comparator. No P1/P2 finding remains open in
the delivered scope.

## Remaining acceptance and owner handoff

The registry contains **8 native implementations, 22 unsupported registrations,
0 full original-runtime-qualified handlers and 0 production-provider-qualified
handlers**. The five-handler component oracle is a separate recorded success.
No checked-in object is marked gameplay-complete.

| Owner | Remaining integration |
| --- | --- |
| Swarm A — Simulation | Compose the actual content bridge; complete query/advertisement mapping, `WorldProvider`, `CheckTreeProvider`, `QueueRuntime`, routing, reservations and accepted-tick callbacks. |
| Swarm C — Presentation | Render decoded sprite/geometry/animation data, preserve event and float-bit semantics, execute audio/HIT and run visual reference comparisons. |
| Swarm D — Browser UX | Implement trusted manifest selection, network/cache lifecycle, authenticated intent transport and graphical Creator workflows. |
| Swarm E — Online | Bind stable host scopes and identities, private transport, fencing, coherent VM/private checkpoint barriers and durable idempotent providers. |
| Swarm F — Integration | Adopt standalone packages into the owned root workspace and W00 contracts; freeze the full installation denominator; run integrated native/browser/original-runtime qualification. |
| Swarm B continuation | Implement the remaining 22 EODs, qualify complete object scenarios, and extend graphical authoring, legacy formats and instruction-debugging surfaces as their upstream contracts become available. |

WarGame's real-time callback behavior needs its own explicit scheduling contract.
The current guarded sprite workflow has no graphical editor, frame creation or
removal, palette remapping or external palette import. Whole-object playability,
durable production behavior and original UI parity remain explicit follow-up
work. The complete package-by-package status is in [README.md](README.md).
