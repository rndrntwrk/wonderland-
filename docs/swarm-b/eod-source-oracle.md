# Unchanged C# EOD handler component oracle

`tools/swarm-b/eod-source-oracle.py` compiles **18 unchanged original C# files**
and compares their observable handler effects with execution through the actual
Rust `NativeHost`. The covered handlers are Timer, DanceFloor, Signs, Scoreboard
and PermissionDoor. Inputs are authored synthetic fixtures; no installation
content, application UI, real lot VM or durable service is started.

This establishes a controlled **original handler component comparison**. It does
not establish full original application, UI, object-content, VM-adapter,
restart, multiplayer persistence or production-provider parity.

## Reproduce

Requirements: Python 3, Mono `mcs` and `mono`, and Rust/Cargo **1.90.0**. The
runner checks the toolchain, prepends the chosen Cargo directory to `PATH`, uses
offline locked Cargo builds, disables incremental compilation and debug info,
and needs no additional Cargo or Python dependencies.

```sh
python3 tools/swarm-b/eod-source-oracle.py \
  --cargo /root/.cargo/bin/cargo \
  --target-dir /tmp/wonderland-eod-oracle-target \
  --logs-dir /tmp/wonderland-eod-oracle-evidence

PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover \
  -s fixtures/eod/source-oracle -p 'test_*.py' -v
```

The target and log directories are optional. Without them the runner uses
temporary directories and removes the C# executable and native target after
comparison. Supplying a target directory permits subsequent runs to reuse the
build cache. Evidence includes the exact input tape, original/native traces,
repeat traces, build logs and `eod-source-oracle.json`.
The CLI also runs the native example's three error-boundary regression tests
and verifies their exact names and pass count. `native-boundary-tests.stdout`
contains the test results; `native-boundary-tests.stderr` contains the Cargo
build log. The report's `native_boundary_tests` and `comparator_tests` fields
record actual pass counts and log filenames for aggregate verification.

The checked-in corpus has **27 scenarios, 226 stages, 77 literal channel
assertions and three explicitly asserted policy-difference stages**. The other
223 stages compare 106 VM events, 116 private UI messages and 61 logical
persistence effects. Both implementations execute twice; each complete trace
must be identical to its repeat. Counts describe this corpus, not general
handler or application coverage.

## Original source identity

The baseline is `4c6b3e8f5835b228723caea3c9f683c62f244f73`.
[`sources.json`](../../fixtures/eod/source-oracle/sources.json) lists every
original input and its SHA-256. The runner compares the working-tree bytes with
both that digest and `git show BASE:path` before compiling and after execution.
The compiler receives the original paths directly; source methods are neither
extracted nor rewritten.

| Linked original group | Files relative to `TSOClient/tso.simantics/NetPlay/` |
| --- | --- |
| Handler framework | `EODs/VMEODHost.cs`, `EODs/VMEODServer.cs`, `EODs/Handlers/VMEODHandler.cs`, `EODs/Model/VMEODEvent.cs` |
| Five handlers | `EODs/Handlers/VMEODTimerPlugin.cs`, `VMEODDanceFloorPlugin.cs`, `VMEODSignsPlugin.cs`, `VMEODScoreboardPlugin.cs`, `VMEODPermissionDoorPlugin.cs` in that directory |
| Source fallback declaration | `EODs/Handlers/VMEODStubPlugin.cs` |
| Source data and helpers | `EODs/Handlers/Data/VMEODSignsData.cs`, `EODs/Archetypes/VMBasicEOD.cs`, `EODs/Utils/EODLobby.cs`, `EODs/Utils/EODPersist.cs` |
| Source command support | `Model/VMSerializable.cs`, `Model/VMNetCommandBodyAbstract.cs`, `Model/Commands/VMNetEODEventCmd.cs`, `Model/Commands/VMNetEODMessageCmd.cs` |

The original registration dictionary is retained. Explicit declarations for its
25 out-of-scope handlers throw if instantiated. They provide no behavior, and
the input dispatcher permits only the five selected plugin IDs. The original
fallback file is compiled to satisfy the original server; the corpus does not
claim fallback coverage.

`source_manifest()` exposes this inventory to aggregate verification. The
oracle also hashes its harness, fixtures, runner, native example, crate source
and Cargo files before and after execution. An edit, addition or deletion
during the run rejects the result instead of retaining a stale pass.

## What actually executes

The C# side uses original `VMEODHost.Connect`, original server construction and
registration, original connect/disconnect/shutdown order, original handler
event dictionaries and ticks, and original serializers for private payloads.
Inbound UI operations go through original `VMNetEODMessageCmd.Verify`, including
its handler-exception catch boundary, then original host/server delivery.
Emitted object commands execute original `VMNetEODEventCmd.Execute` against the
test thread connection containers.

The Rust side is
[`examples/source-oracle.rs`](../../crates/eod-runtime/examples/source-oracle.rs).
It uses public `NativeHost` connection, receive, tick, private-output,
object-output, checkpoint and persistence APIs. It does not call the private
Rust handler implementations or set their private state. This makes a broken
native dispatch, wrong recipient, missing event or incorrect persistence effect
observable through the same boundary a VM/transport adapter would consume.

Linked files have methods beyond this scope. Compiling an entire original file
does not mean that every method ran. In particular, legacy network byte framing,
application UI execution, lot object/BHAV execution, full host resynchronization
and unrelated handler registrations are not covered.

### Explicit adapters

| Adapter | Supplies | Does not supply |
| --- | --- | --- |
| VM/entity containers | Positive Int16 ObjectIDs, UInt32 persistent IDs, invocation registers, avatar permission values, object lookup and EOD connection containers | Simulation state, BHAV execution, content loading, renderer or authoritative permission discovery |
| Synchronized-event boundary | Captures original command invoker/code/temp arguments and executes the original event command; consumes emitted events and clears ended thread connections after each stage | A real synchronized VM commit or inferred register changes from object scripts |
| Private transport sink | Captures original recipient, plugin ID, event, payload kind and exact bytes | Client UI execution or the original transport serialization/protocol |
| Deferred global link | One explicitly queued load callback, a snapshot at request time and bounded in-memory save bytes | Real storage failures, access control, fencing, atomic durability or background service scheduling |
| Queued task scheduler | Runs actual .NET `Task`, `TaskCompletionSource` and `ContinueWith` work on a deterministic FIFO scheduler | Thread-pool timing, arbitrary concurrent callbacks or a replacement `EODPersist` implementation |
| Native authority/store/provider | Authenticated fixture actors, real checkpoint-gated writes and in-memory revision/receipt checks | Production authentication, a durable checkpoint barrier or a qualified storage service |
| Unused serializer/type declarations | Compile-time dependencies; all unused runtime paths throw | Fake renderer, IoBuffer serialization or out-of-scope handler behavior |

The VM adapter deliberately receives new registers from the input tape. It
does not interpret the emitted object event as a complete object script. Its
ended-connection cleanup corresponds to the completed boundary in original
`VMInvokePlugin.cs:39–57`; this is an explicit adapter rule, not execution of
that primitive. Invariant culture is selected explicitly for C# fixture runs.

### Task and persistence stages

Scoreboard's original `EODPersist.Patch` uses `ContinueWith`, as does its
connection state response. Replacing these with an immediate fake would hide
load and callback ordering. The harness instead invokes each operation as a
task on `QueuedScheduler`, so original continuations capture that scheduler.
It pumps only ready tasks; an unresolved `TaskCompletionSource` remains
unresolved until the fixture explicitly releases its original load callback.
The source helper itself is unchanged.

A named stage has this boundary:

1. Apply one fixture operation. Tick operations may request several exact ticks.
2. The source side pumps ready original continuations. The native side drains
   emitted VM and private UI queues into the trace.
3. The native side records a test checkpoint, then drives persistence. A write
   therefore crosses the actual native checkpoint gate before the provider
   observes it. UI output created by a completed load is drained in this stage.
4. Finish the stage after all currently ready effects have been observed.

A native operation error still completes that checkpoint/provider phase.
Otherwise an erroneous queued write could be hidden by a terminal error stage.
Provider failures also run the final VM/private drain, since an earlier load
in the same provider call can already have queued a UI message. The trace
preserves the operation error followed by any completion error; an expected
error never excuses an extra output, save or secondary error. Drain failures
are explicit adapter errors, distinct from expected native handler errors.

The source load callback is asynchronous. The native API polls, so its adapter
keeps one logical read pending: the first attempt records `request` and returns
`Retryable`; intermediate polls record no new logical operation. `release`
allows that retained snapshot to return and records `result`. A source save
and a native checkpoint-gated save are compared within the same named mutation
stage. Their exact relative timing against *other channels* is not equated.

The corpus serializes persistent invocations and has at most one outstanding
persistent load per scenario. This does not qualify overlapping asynchronous
patches, concurrent loads or source multiplayer persistence behavior.

## Scenario and trace protocol

[`scenarios.json`](../../fixtures/eod/source-oracle/scenarios.json) is the
reviewable input. Each case starts with `reset` and contains ordered string
arrays. The runner supplies the case name and zero-based stage number to both
executables. Lines are ASCII with tab separators:

```text
case_name<TAB>stage_number<TAB>operation<TAB>arguments...
```

Arbitrary text/binary bytes use lowercase hexadecimal, so embedded NULs,
whitespace, commas and UTF-8 are unambiguous. `-` denotes zero bytes. A missing
provider record is distinct from a present zero-length record.

| Operation | Ordered arguments |
| --- | --- |
| `reset` | None; starts an isolated source/native host and provider |
| `seed` | Plugin alias, persistent object ID, bytes |
| `connect` | Label, plugin alias, VM object ID, persistent object ID, invoker ID, actor persistent ID, avatar ObjectID, permission enum value, native owner/edit authorization (`0`/`1`), comma-separated registers |
| `controller` | Label, floor ObjectID, invoker ObjectID; the original avatar is null |
| `release` | None; release the pending provider result and ready continuations |
| `registers` | Label, four to eight signed Int16 registers |
| `tick` | Count from 1 through 10,000; exact calls to each host's tick API |
| `text`, `binary` | Label, event name, exact payload bytes |
| `disconnect` | Label; authoritative invoker disconnection |
| `policy_binary`, `policy_release` | Same arguments as their ordinary counterpart; permit a recorded error only for a case with exact declared policy expectations |

Plugin aliases are `timer`, `dance`, `signs`, `scoreboard` and `door`. Connect
registers remain original signed values; native typed mode/authority inputs are
derived only from these trusted fixture fields. They never come from UI text.

Every output stage has `BEGIN case stage` and `END case stage` records, with
the same tab separators. The observable records between them are:

| Tag | Ordered fields after the tag |
| --- | --- |
| `VM` | Invoker ObjectID, signed event code, comma-separated signed temp arguments or `-` |
| `UI` | Actor persistent ID, eight-digit plugin hex ID, `text`/`binary`, event name, payload bytes |
| `PROVIDER request` | Persistent object ID, plugin ID |
| `PROVIDER result` | Persistent object ID, plugin ID, existence (`0`/`1`), bytes |
| `PROVIDER save` | Persistent object ID, plugin ID, bytes |
| `ERROR` | Exact exception/error name for a declared policy stage |

The parser rejects missing/duplicate/reordered stages, missing final markers,
unknown tags, extra columns, invalid payload kinds, noncanonical numbers/hex and
oversized records. Empty stages are compared too. Scenario input is bounded to
2 MiB and 4,096 stages; individual lines to 300 KiB; payloads to 128 KiB; output
to 16 MiB and 100,000 rows. Actual `NativeHost` limits still apply and can reject
smaller inputs; the corpus stays inside those native bounds.

### Comparison projection

Within each stage, VM effects preserve global order, including save and
disconnect events. Logical provider effects preserve global order. Private UI
messages preserve order **for each recipient**. Independent private recipients
can be drained separately; the comparison does not invent a single transport
order between them.

The projection retains every object event, every private event and payload
kind, every recipient/plugin identity, every load request/result and every save
payload. Connect/disconnect codes `-2`/`-1`, `eod_enter` and `eod_leave` remain
in the comparison. Native scope/epoch/ticket/sequence/authentication envelopes,
checkpoint bytes, CAS revisions and idempotency receipts are explicit native
boundary metadata and are not represented as original C# protocol fields.

## Covered behaviors

| Handler | Fixture coverage |
| --- | --- |
| Timer | Raw register byte casts versus internal mode default; short/empty/invalid/trailing payloads; packed time; start/pause; fifth stopped tick; already-running pause; retained `Tock`; running-tick reset; stopwatch and countdown zero behavior; Off-before-Update; close |
| DanceFloor | Null-avatar controller, two independently addressed UI participants, actual avatar ObjectID in object events, byte parsing with whitespace/NUL and invalid input, absent controller, player close, controller termination and remaining participant behavior |
| Signs | Deferred initialization; default/loaded flags; roommate/visitor/read/owner modes; denied-read/write-mode override; private redaction; UTF-16 supplementary character counting; zero maximum; multibyte string-length prefix; ignored tail bytes; malformed message no-op; empty writes; reload after save |
| Scoreboard | Show before asynchronous load; no state before callback; missing and loaded six-byte state; ignored persisted tails; signed Int16 wrapping before clamp; valid/invalid/undefined numeric and symbolic enums; updates versus sets; color changes without UI echo; unknown team still persisting; Unicode enum trim and numeric NUL rules; reopen |
| PermissionDoor | Deferred initialization; Edit/View/CodeInput; bounded code/state/fee/flags; stale cached code after a save; reload seeing the new code; code privacy; matching/wrong/malformed/preinit attempts; decimal load fallback; full UInt32 persisted code versus narrower edit bound; Save-before-disconnect |

The literal assertions supplement the complete source/native comparison. They
pin key observations independently, including exact raw Timer bytes, fifth-tick
silence/update, recipient data, sign flags, score wrapping, absent color UI,
door privacy and close ordering.

## Intentional native policy differences

These differences are not broad exceptions to the comparator. The fixture
declares the **complete source and native output stage** for each, and both
must match exactly. Every other stage in the same case still compares normally.
The loader requires a named policy and a one-to-one pairing between each
`policy_` operation and its source/native outcome declaration; an unmarked
exception stage or an undeclared policy operation is rejected before execution.

| Policy | Controlled original component observation | Native observation |
| --- | --- | --- |
| Signs write before permission initialization | Accepts the Erase-mode write, emits writing event and saves the supplied data | `PluginNotReady`; no write or object event |
| Invalid UTF-8 in a Signs message | `BinaryReader` replacement-decodes the invalid byte, then saves the replacement-character text and emits writing event | Accepts the source-malformed message as a no-op; no save or object event |
| Truncated Scoreboard provider record | Original `EODPersist` load callback throws `EndOfStreamException` under this provider adapter | `InvalidPluginData`; no state response |

The last row describes the original **callback**, not full application storage
recovery. Original `LotServerGlobalLink.cs:215–237`, which is not linked here,
wraps callback execution and can catch an exception and supply a null result.
Different original providers can therefore recover or fail differently.

Further exclusions stay explicit in the evidence JSON:

- Native transport authentication, scope, epochs, sequences, rate/queue/idle
  limits and invalid envelope/payload-kind admission have no matching original
  protocol assertion here.
- Native checkpoint fencing, CAS, idempotency and restart recovery remain
  covered by native boundary tests; this harness uses only an in-memory test
  checkpoint and successful serialized writes.
- A Signs UTF-16 truncation splitting a surrogate is rejected atomically by
  the native handler. The original can partially mutate its private state
  before its encoder throws. That private source state is not exposed by this
  output-only oracle. The existing native test
  `signs_rejects_a_split_surrogate_without_mutating_session` covers the guard.
- Persistent session exclusivity, overlapping patches/loads, unsupported raw
  invocation modes and unauthorized owner/edit invocations are outside the
  common handler input domain.

## Evidence against false passes

The 21 comparator/protocol unit tests exercise missing stages, output mutation,
VM/UI reordering, recipient changes, payload-kind changes, persistence loss,
unknown records, malformed hex, unadvertised columns and unpaired policy
declarations. Exact-policy regressions reject residual VM/private/provider
effects and a second error alongside an expected error. The initial no-op
comparator failed the effect tests before implementation.

Three native example tests were observed failing before the stage-completion
fix and passing afterward. They queue real `NativeHost` disconnect outputs,
queue a Scoreboard color write without a UI echo, and arrange two ready
provider results where the first queues private UI before the second fails.
The last case checks both a successful operation and a primary operation error,
so the adapter must preserve both errors and the earlier output. These native
adapter regressions do not extend the original-source corpus's stated
single-load concurrency scope.

Each successful executable run also mutates the **actual compared traces** and
requires all nine negative controls to fail comparison: changed VM code,
changed private bytes, changed private kind, changed save bytes, reversed VM
effects, reversed private effects, dropped stage, dropped provider effect and
misrouted private recipient. A matching empty or partially compared trace cannot
pass the literal assertions, complete-stage checks and mutation controls.

The C# compilation currently reports the unchanged source's existing CS0414
warning for `EODPersist<T>.CurrentData`; the field is assigned but never read.
The harness does not suppress or edit that source warning. Build and execution
logs retain it alongside the pinned source inventory.

The machine report keeps `original_application_runtime`, `ui_runtime`,
`vm_adapter_integration` and `production_provider` explicitly `unverified`.
Promoting one of those claims requires evidence from the corresponding real
integration, not another pass of this controlled component oracle.
