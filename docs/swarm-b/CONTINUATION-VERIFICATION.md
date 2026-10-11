# Swarm B continuation verification

This is the historical continuation record. The later cooperative-game,
sprite-workflow and cooked-runtime buildout is recorded in
[BUILDOUT-VERIFICATION.md](BUILDOUT-VERIFICATION.md).

The then-complete frozen implementation passed **44 verification gates** on
2026-10-05: **347 Rust tests and 19 Python tests**, formatting, strict
Clippy, portable library builds, source comparisons, deterministic cooking,
and both authored and actual-runtime native/WASI execution. No gate failed.

The machine-readable record is [continuation-verification.json](continuation-verification.json).
The inputs remained unchanged throughout the complete run. Their ordered
verification-input SHA-256 is `1e3a4d35e82aeca4e73c78154cf50ee78f404a6912c05f867c27daf44e2a9cf6`.

## Scope and immutable references

- Original source baseline: `4c6b3e8f5835b228723caea3c9f683c62f244f73`.
- Initial Swarm B stack: #1–#4, ending at `feaa91549acae3da2931f0b95ba768348f7d56f8`.
- Actual Swarm A simulation: `8a0e251d19e222a0a6833d7408ca629f674e1729`.
- Continuation: indexed IFF/sprite authoring, guarded creator transactions,
  four additional native EOD handlers, and the isolated actual-runtime bridge.

[VERIFICATION.md](VERIFICATION.md) and [verification.json](verification.json)
preserve the initial 35-gate/219-Rust-test result. They are historical records.
This continuation changes capabilities and test counts without expanding the
original checked-in corpus denominator or claiming production gameplay parity.

## Reproduce the complete check

Use Rust/Cargo 1.90.0 with rustfmt and Clippy, Python 3, Mono (`mcs` and `mono`),
a C++ compiler supporting GNU C++98, and Node with `node:wasi`. Both
`wasm32-unknown-unknown` and `wasm32-wasip1` must be installed. The recorded
execution environment uses Node 24.19.0.

```sh
rustup toolchain install 1.90.0 --component rustfmt --component clippy
rustup target add --toolchain 1.90.0 wasm32-unknown-unknown wasm32-wasip1
git fetch --no-recurse-submodules origin 8a0e251d19e222a0a6833d7408ca629f674e1729
```

The bridge uses a sibling `sim-core` dependency in the eventual integrated
workspace. Before integration, its runner exports the exact pinned Git subtree
into a dedicated assembly and links the current B-owned sources. It does not
clone historical repository submodules or write to the original simulation.
For a machine without cached bridge dependencies, fetch them once:

```sh
python3 tools/swarm-b/runtime-bridge.py /tmp/wonderland-runtime-deps fetch --locked
bash tools/swarm-b/verify.sh --with-parity --with-source-oracle --with-runtime-bridge \
  --logs-dir /tmp/wonderland-b-continuation-verification
```

Use a fresh dedicated assembly path if the example path belongs to another
checkout. `--cargo /absolute/path/to/cargo` selects a specific Cargo executable;
the aggregate runner prepends its directory to `PATH` and selects Rust 1.90.0.
Runtime verification uses `--locked --offline` after dependency preparation.

The runner creates and removes only its own temporary build/assembly directories.
It retains command logs and `verification.json` under `--logs-dir`. Formats,
content, interaction code, and the bridge's portable library are compiled for
`wasm32-unknown-unknown`. The authoritative EOD library must fail compilation on
that target with its explicit native-only diagnostic.

The source-input digest covers code, locks, authored tests/fixtures, compatibility
metadata, the original flat IFF corpus, Iffinator archive, SPR2 reader/writer,
and all four original FAR3 reader files. It is checked before and after the
complete run. Changes to an original input after its individual gate therefore
invalidate the aggregate result. Two regression tests cover mutations, additions,
deletions and the original-reader inventory.

## Native coverage

| Package or executed harness | Rust tests passed | Ignored in that invocation |
| --- | ---: | ---: |
| Formats: standard suite | 91 | 3 |
| Indexed IFF: explicit source corpus | 2 | 0 |
| Content resolver and packs | 14 | 0 |
| Actual manifest-module harness | 7 | 0 |
| Actual interaction-module harness | 55 | 0 |
| Native EOD host and handlers | 81 | 0 |
| Creator library and CLI | 30 | 0 |
| Asset cooker and CLI | 23 | 0 |
| Actual content/runtime bridge | 44 | 0 |
| **Total** | **347** | **3** |

The Python total comprises **8 catalog tests, 9 assembly path/source-integrity
tests, and 2 verification-input stability tests**. The aggregate also executes
the actual CLI transaction tests through the creator Rust harness.

The standard formats suite intentionally leaves three corpus-dependent tests
ignored. The indexed source oracle explicitly executes two of them. The older
optional semantic corpus test remains separate; the pinned content census is
regenerated and compared in full during every aggregate run. Three compile-fail
documentation tests remain part of the Rust total: one EOD private-channel
boundary and two creator invariants.

## Original resource-map comparison

The indexed IFF oracle first validates the original-backed Rust edit path, then
checks emitted files with the historical Iffinator C++ reader:

| Observation | Verified result |
| --- | ---: |
| Original indexed source files | 198 |
| Exact unchanged passthroughs | 195 |
| Strict duplicate-identity rejections | 3 |
| Original maps supporting checked edits | 192 |
| Ambiguous original maps rejecting edits | 3 |
| Supported original resource-map entries | 13,084 |
| Edited original v0 files accepted by the C++ reader | 960 |
| Source-derived v1 edits accepted by the C++ reader | 5 |
| Total emitted bytes checked | 302,174,293 |

The original corpus contains v0 maps; the v1 fixture is explicitly source-derived.
The archive lacks `iff.h`, so the harness supplies declarations/accessors and
changes one obsolete allocation spelling for modern compilation. It retains
the historical parsing/validation bodies. That reader does not validate every
size or complete map coverage; the Rust checks enforce those requirements too.
See [indexed-iff.md](indexed-iff.md) and
[indexed-iff-verification.json](indexed-iff-verification.json).

## Sprite source comparisons

The complete original `SPR2FrameEncoder.cs` is compiled unchanged with minimal
frame/color/byte-writer containers. **257 vectors** pass, including every one
of the **254 nontrivial input alpha values**. The command-stream SHA-256 is
`fc27c24370758905ed03846d98ff9862b2bf89cd421c84d6a713d30e49640ce0`.

A separate harness executes four unchanged, hash-checked original SPR2 reader
methods on Rust-authored resources: **18 cases, 34 frames and 294,034 pixels**.
The vectors exercise versions 1000/1001, flags 1/3/5/7, explicit/default palette
IDs, transparency, depth, offsets, zero dimensions and command/row boundaries.
The extracted-method SHA-256 is
`0d5a24fe4449de91f4413dba38208d5ee3a134ed614beb10bdbb4a0ae97479ba`.

These are CPU codec comparisons. Rendering, visual composition, palette choice
and complete original-client integration remain separate qualifications.
[sprite-authoring.md](sprite-authoring.md) describes the exact-alpha default
and explicit counted source-style quantization option.

## Actual runtime and native/WASI replay

The bridge runs the actual pinned Swarm A interpreter. Its source execution
fixture imports BHAV 4110 from the unchanged
`Casino_2-Tile_Bar_CC.iff` into an explicit four-attribute test harness. This
executes a real source routine; it does not import or qualify the whole casino
object as playable.

The expected query returns true after three instructions and leaves the source
snapshot byte-identical. Replaying an accepted `StartBehavior` tail after a
completed-tick snapshot reaches tick 2 with attributes `[0, 0, 30, 0]`, preserves
authority/replica state agreement, and dispatches no replica durable effects.
The driver checks literal outcomes before comparing the full native and WASI
results, and requires rejection of two altered-output controls.

The recorded expected state hash is
`d52e0091425b12e7d820dcb7f1b883fb36606059b7a121bb5cf9218cc272f62b`;
the snapshot SHA-256 is
`49094c9adc9b1e3df73264ec221c323ce0027ec231af4f3e38b2b006fe0ca43d`.
The fixture source SHA-256 is
`20b67e06940bfe0a89b689312fe001721ff3760cce10fca104373bf1122c4865`.

The creator adapter advances isolated accepted ticks and inspects attributes,
watches and current frame positions. It does not claim instruction stepping or
an executed instruction trace. Exhausting A's dispatch budget retains A's fault
behavior; it is not converted into a resumable instruction pause. Full
`CheckTreeProvider`/`QueueRuntime` integration remains blocked on the public
runtime's query-state, advertisement and scheduler callback surfaces. See
[runtime-bridge.md](runtime-bridge.md) for the representable import contract.

Creator inspection caps the retained report at 1 MiB, including result records,
field strings and formatted values. It counts Debug output from borrowed state
before allocating any output strings, then writes into fixed-capacity buffers.
The exact boundary succeeds; one additional byte rejects. Repeated watches of a
large stored fault cannot amplify the report beyond that limit, and accepted
reports preserve complete escaped and multibyte values.

The earlier authored content/pack/interaction native/WASI probe also remains
part of the full run. Its 17,150 canonical bytes have SHA-256
`42fe83da0cace7c343fd3736713893014187bc593cecb8694b8948c25ce15782`.

## Independent review corrections

All concrete findings below were corrected and independently rechecked before
the aggregate run. The committed suites retain focused regression coverage.

| Finding | Correction and checked outcome |
| --- | --- |
| Indexed rebuilding could allocate metadata before bounding the whole operation | Preflight source/edit metadata, clone, map and output workspace before allocation; retain conservative checked admission. |
| Empty-width sprite frames traversed every empty row | Use a bounded zero-width fast path; preserve identical encoded bytes. |
| Creator structs accepted positional arrays | Map-only visitors reject array-shaped documents, operations, keys, guards and edits; valid field reordering still works. |
| Creator sidecars geometrically grew past the charged length | Allocate the checked length exactly, use `read_exact`, probe growth with one stack byte, and charge retained capacity. |
| Tagged JSON operations buffered unknown values before rejection | Stream typed fields and reject unknown names before consuming values. The original 65,839-byte hostile spec now rejects with 503 additional peak heap bytes. |
| Restored EOD handlers and provider records could disagree | Validate handler/pending/binding correlation and canonical orphan payloads before provider dispatch. |
| A restored ordinary Signs writer could change baseline permission flags | Compare pending flags with the previously loaded binding, including an owner acting in ordinary Write mode. |
| OBJD runtime definition reads were shifted by two words | Prepend the source version words, use version-bounded raw fields, and verify actual interpreter definition reads. |
| Multipart subobjects lost inherited interactions or misread packed negative offsets | Use only the source's `SubIndex == -1` master sentinel; resolve the validated master's TTAB/TTAs for a child without its own table. |
| Different effective resources could merge into one shared namespace | Bind each global/semiglobal namespace to one full effective source hash before inserting resources. |
| Assembly refresh could overwrite an external linked file or accept extra A source | Validate directories, owned regular files, link counts and the entire exact A subtree before mutation; reject drift and unrecorded build scripts. |
| Query arguments and watch field strings were cloned before validation | Check budgets, argument cardinality and bounded canonical field syntax before restoring/cloning. |
| Runtime import budgets omitted aggregate retained allocations | Check A's count limits and cumulative decode/workspace/output admission before allocation. |
| Lossy text conversion grew beyond the charged byte length | Stream source text into a preallocated buffer; 1,000,000 invalid UTF-8 input bytes now retain exactly 3,000,000 output bytes of capacity. |
| Decoded tables, temporary indexes/envelopes and moved spare capacity escaped admission | Charge live decoded buffers before the next decode/output, temporary identity/decoder structures, and capacities of moved animation metadata. |
| A's content validation allocated a canonical serialization buffer after retaining its name index | Count the exact pinned bincode representation without allocating, account for final map keys, and reserve both the buffer and simultaneous animation-name index before construction. |
| Repeated stop watches amplified a stored fault into a large report | Borrow the stop, count its complete formatted representation, enforce the aggregate 1 MiB report limit, and allocate exact output capacities only after admission. |
| Aggregate verification omitted original oracle files from its stability digest | Hash the actual original IFF/C#/archive inputs at both ends; mutation/addition/removal regressions now reject stale evidence. |

Supplemental independent EOD review exercised 1,280 Signs configurations across
five recovery phases, all Door modes with ordinary and malformed persisted code
variants, and orphaned writes after a provider applied a request but lost its
response. The committed actual-host tests cover these failure classes with
focused deterministic cases.

## Remaining qualification

The generated registry reports **5 source-translated native EOD handlers,
25 unsupported registrations, 0 original-runtime-verified handlers, and
0 production-provider-qualified handlers**. No checked-in object is counted as
gameplay-complete. Complete object scenarios, graphical authoring, rendering,
audio/HIT execution, full interaction scheduling/routing, browser transport/cache
integration and durable production providers remain explicit integration work.

The original C# and assets are unchanged. The continuation stays in B-owned
code, tests and documentation, and leaves Swarm A's source and the root workspace
with their owners. The publication remains a draft PR stack; no merge is part
of this result.
