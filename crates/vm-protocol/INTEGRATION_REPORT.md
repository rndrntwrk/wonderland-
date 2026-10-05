# Source VM protocol increment

## Implementation

New owned crate `wonderland-vm-protocol` only. Dependencies: serde derive and
flate2 with portable Rust backend; test-only serde_json. Root owns workspace,
gateway and browser manifest/app integration. Exact API/type agreement was sent
to `original_services_build` and `source_world_build` before integration.

Implemented whole-input `decode_tick_list`, `decode_direct_command` and
`decode_snapshot`. There is no command scanning or guessed length skipping.
All original source enum0–48 command layouts are validated before returning;
unknown commands/version/nested variants reject the entire packet. EOD text and
binary records preserve authenticated-actor addressing fields for gateway gating.
StateSync uses its actual source layout without an ActorUID prefix and includes
bounded optional trace records. Command wire offset and consumed extent are exact.

FSOv38 TSO snapshots decode the full context/architecture/terrain/wall/floor/roof,
entity/object/avatar/platform/animation/outfit/motive/relationship,
thread/queue/callback/routing/direct-control/async/EOD/scheduler,
multitile/global/lot-platform/tuning layout. Compressed snapshots have bounded
explicit-length, CRC-checked single-member GZIP; all members in a packet share an
aggregate expansion budget. The entire consumed source body is retained. UInt64
and Int64 values serialize as exact decimal strings.

Snapshot semantics are RefreshOnly. No conversion to Swarm A's deterministic
interpreter snapshot is implemented or claimed. Original frames/RNG/scheduler and
all source quirks require a compatible restore/execution adapter before accepted
native commands can prove a working deterministic replica. Source snapshot
projection and exact event observation are available now. Gateway owns actor,
lot-session incarnation and tick replay; the world mapper owns normalized
provenance, correct native terrain edges, ID distinctions and missing resources.

## Initial verification

`cargo test -p wonderland-vm-protocol` final run: **18 passed**, zero failures,
zero warnings (11 snapshot tests and 7 tick/direct command tests).

The first source-wire tests were exercised against stubs and observed RED before
implementation: four FSOv/StateSync tests failed with the unimplemented v38/parser
error, and four EOD/direct/tick tests failed with unsupported18. Then the actual
readers made them green. Expanded tests exercise real production decoding over
source-serializer wire fixtures, not mocks; no fixtures enter production code.

Meaningful coverage includes:

- Full object snapshot in both uncompressed and explicit-length GZIP form;
  actual heights/grass/floors/walls/roof/remap, GUID/pose, queue and ownership.
- Rich avatar snapshot with exact64-bit/named outfit identities, animation and
  motives, all three stack-frame families and all four async-state families,
  callback Int16 width, EOD plugin events, chat/job/tuning state.
- All known command serializer extents, then a following EOD command to prove
  alignment through variable source layouts. Source payloads containing apparent
  command enum bytes remain payloads.
- Exact source tick seed, direct command no tick header, missing/truncated fields,
  unknown-command atomic rejection after a valid EOD, malformed count/string/
  binary length, unsupported version and trailing input.
- Every truncated prefix of the rich routing/inventory snapshot; GZIP CRC error,
  expansion bounds, cumulative expansion across two StateSync commands,
  architecture dimensions, configured-budget sentinel overflow and unknown stack/async variants.
- JSON retains original long-key/clock precision and omits the large raw body.

Scoped tests were used as root explicitly prohibited full workspace builds in
this worker; shared build-directory contention and disk exhaustion were reported
to root. Root/gateway/world workers own their integrated tests and browser gates.
No successful live login/city/lot/snapshot connection is claimed by these fixture
or parser tests.

## Explicit compatibility edges

Supported FSOv version38 TSO only; TS1/other snapshot versions, older persisted
avatar versions, invalid environment/EOD clipped lengths and ambiguous null
BlueprintRestore reject transactionally. A source callback serializer/reader width
disagreement is documented: parse actual emitted Int16 wire, preserve raw state,
retain RefreshOnly status. See README for exact source anchors and API semantics.

Shared test-only golden binary fixtures were added at root request under tests/fixtures. They include the exact uncompressed fsov(false) builder bytes, the same GZIP body, direct StateSync and tick-list StateSync. The fixture equality test verifies the emitted uncompressed bytes byte for byte; gateway/world tests can independently reference them. No production reference or bundling exists.

## Independent review fixes: allocation and count consistency

The follow-up fixes address review findings I1 and M1. Every variable collection
now validates its declared count against remaining bytes and the smallest source
record size before reserving storage. Wall/floor levels and multitile offsets
receive equivalent checks. Byte-counted collections now apply `max_count` as
well as the aggregate entry budget. No limits or source identifiers changed.

Nine new regressions were observed failing before the production edits. Eight
measure real allocator requests, with an observer that delegates unchanged to
the normal system allocator: incomplete ticks, commands, entities, frames,
queues, trace ticks, trace strings and architecture levels previously requested
large buffers before truncation. The command case measured a 44 MiB reservation
from a 21-byte input. The ninth regression proves that one byte-counted EOD entry
rejects with `max_count=0` and remains accepted with `max_count=1`.

Fresh focused command after the source edits:
`cargo test --locked --offline -p wonderland-vm-protocol`: **27 passed**, zero
failures and no warnings (9 allocation/count tests, 11 snapshot tests and 7
tick/direct-command tests). The protocol library recompiled for this run. Full
workspace, WASM and browser gates remain root's responsibility; this check does
not claim them.

Review finding M2 remains an explicit evidence limitation: the golden fixtures
are synthetic handwritten source-wire models, not independent C# serializer
output or observed live-server responses. `RefreshOnly` semantics are unchanged.
