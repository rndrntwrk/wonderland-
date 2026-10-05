# Swarm B remaining implementation — verified handoff

The final stable-source run passed **59 gates,
654 Rust test executions, 51
Python tests and 25 real-browser workflows** on
5 October 2026. All requested optional gates ran. The final verification-input
digest matches the digest captured before the run. This is a verified B
implementation handoff; live provider integration and complete original-game
qualification remain the explicit boundaries below.

The exact command/result record is [remaining-verification.json](remaining-verification.json).
Its log and details paths are relative to [remaining-evidence](remaining-evidence/).
The [evidence index](remaining-evidence/evidence-index.json) binds retained logs,
reports, reviews and screenshots by SHA-256. Command arrays preserve the actual
execution paths. Compiler binaries and oversized generated test downloads are
reproducible outputs and are not copied into Git.

## Publication and immutable inputs

The continuation branch `swarm-b/remaining-implementation` is stacked above
published draft [PR #17](https://github.com/rndrntwrk/wonderland-/pull/17),
`swarm-b/buildout-verified`. Publication retains the reviewed source tree and
remains a draft; no merge is part of this handoff. Earlier verification records
describe their own immutable milestones.

| Reference | Value |
| --- | --- |
| Previous B milestone | `3d6b04e0a600cb5f947145123cddf6cb2c75c5d7` |
| Original source/assets | `4c6b3e8f5835b228723caea3c9f683c62f244f73` |
| Actual A simulation | `8a0e251d19e222a0a6833d7408ca629f674e1729` |
| Existing UI preserved for D | `7ee14583ca10ed86511d717566295ea44d481bf7` |
| Verification-input SHA-256 | `d297337d4a117845241cfb847e4f073cacdc0026ae81cbf72ac490214aa8b85c` |
| Creator WASM SHA-256 | `c2cf91400bf653188682cc8aa9df8347241e5d5252f4f41d07a9ba0cfc8e8a59` |
| Complete runtime-family JSON SHA-256 | `fe480079aaffc1c167feb0ae5438e324b9bdebd3ac51ed4e101141b421b3e2be` |

Original C# and assets, A/C/E implementations and root workspace/contracts are
unchanged. Only B-owned format/content/EOD, interaction adapters, tools,
fixtures, tests and evidence paths are changed. The runtime assembly exports
and verifies the pinned real A implementation and links B's actual sources.
It does not substitute a fixture interpreter for that runtime.

## Delivered behavior

| Area | Implemented and verified | Detail |
| --- | --- | --- |
| Formats/content | Source-preserving Vitaboy/legacy companion writers, references, table/sprite/string variants, bounded FSOm gzip, neighborhood metadata, PNG and explicit visual codecs | [Format authoring](format-authoring-completion.md) |
| Native EOD | All 30 registered native handlers through the real host, including casino/social/music/wardrobe/trade services; private capability/provider operations and checkpoint recovery | [Native boundary](eod-native-boundary.md), [casino](eod-casino.md), [social](eod-social.md), [services](eod-services.md) |
| Creator core | Guarded upgrades, city/neighborhood edits, typed avatar/mesh assets, source-bound OBJ/MTL/GLB/glTF, real skeleton/event handling and ordered patch provenance | [Extended authoring](creator-authoring.md) |
| Creator browser | Six Rust/WASM workbenches with actual uploads/downloads, independent bounded histories, typed forms, source guards, patch ordering and responsive DOM controls | [Graphical verification](creator-web-verification.md), [build/run instructions](../../tools/creator-web/README.md) |
| Actual runtime bridge | Live bounded inspection, exact TTAB/queue binding and accepted intents, read-only query proof, complete source-family execution/outcome inventory on native and WASI | [Runtime report](runtime-remaining-report.md), [extension contract](runtime-extension-contract.md) |

The normal corpus test checks **455 original payloads**: 50 animations,
120 appearances, 120 bindings, 121 meshes and 44 FSOm payloads. Compressed FSOm
is compared after expansion; equal gzip bytes are not asserted. This is one
Rust corpus test with 455 assertions of source coverage, not 455 additional
unit tests.

The actual runtime runs **144 original private BHAV routines in two dialects
and two caller contexts: 576 cases**. Native and WASI results match in full.
The [complete report](remaining-evidence/runtime-family-native.json) contains
1,318,573 bytes; the [compact inventory](runtime-source-family-results.json)
retains every observed outcome and unresolved call. Unsupported primitives,
missing global routines and incomplete original content remain visible.
These cases are not added to the unit-test total or labelled complete objects.

## Exact test accounting

Counts come from successful test summaries in retained logs, including the
explicitly executed nested source-oracle tests. Clippy compilation does not
count tests a second time.

| Gate/package | Rust tests passed | Ignored in that invocation |
| --- | ---: | ---: |
| `formats-tests` | 123 | 3 |
| `original-indexed-iff-reader` | 2 | 0 |
| `content-tests` | 14 | 0 |
| `manifest-tests` | 7 | 0 |
| `interactions-tests` | 55 | 0 |
| `eod-tests` | 221 | 0 |
| `original-eod-handlers` | 3 | 0 |
| `creator-tests` | 73 | 0 |
| `cooker-tests` | 26 | 0 |
| `creator-web-tests` | 23 | 0 |
| `runtime-bridge-tests` | 107 | 0 |
| **Total** | **654** | **3** |

Python contributes 21 EOD comparator/protocol tests, 14 catalog tests, nine
runtime-assembly integrity/path tests and seven verification-input tests.
The browser count is separate: fourteen IFF workflows, seven extended-workbench
flows, three reproduced review regressions and one startup-failure check.

The ordinary formats suite records three opt-in skips. Two indexed-IFF tests
are subsequently run and pass through the explicit source oracle. The separate
`repository_semantic_corpus_probe` remains opt-in; the pinned content census
is regenerated and compared in this aggregate. Three compile-fail documentation
tests are included in the Rust total. The native-only EOD target rejection is
also a passing boundary gate with its expected diagnostic, not a hidden failure.

Every package formatting and strict all-target Clippy gate passes. The Creator
also passes strict release WASM Clippy. Formats, content, interactions, runtime
bridge and the Creator build/check their named WASM targets. Source, cooked and
family runtime comparisons execute native and WASI programs and reject their
deliberate negative controls.

## Independent review and corrected failures

The retained [review reports](remaining-evidence/reviews/) cover allocation and
source preservation, browser sessions, real-runtime ownership and native EOD
authorization/recovery. Material corrections include bounded glTF graph work,
sparse OBJ indices in both directions, aliased animation sample consistency,
opaque BGRX interoperability, private operation recovery/peer readiness and
transactional native cleanup. Reviewers reproduced targeted faults and checked
the covering corrections; the final aggregate then verifies the integrated tree.

Graphical regressions first demonstrated a stale draft editing a replacement
source, an unchanged neighborhood form inserting a missing description and a
typed mesh edit rebuilding the selected-element control. Corrected model/DOM
behavior preserves source guards, absent fields, selected element and visible
FSOm bounds. Skeleton attachments participate in unsaved-work detection.

A final script review found inconsistent relative target/distribution paths
and a hardcoded default artifact hash. The old runner was observed failing
with the configured bundle present and default distribution absent. The fix
normalizes paths against the package root; all 25 workflows then passed with
the default absent. The final aggregate also builds and serves a relative
custom output directory. Local and HTTP-served WASM digests must match before
browser workflows begin. The tested artifact is 2,985,883 bytes.

Two earlier aggregates are retained under [earlier-attempts](remaining-evidence/earlier-attempts/).
The first stopped at temporary-cache cleanup after 46 passing gates; the cache
was being repopulated in the synchronized workspace. Moving compiler temporaries
outside that directory resolved cleanup. The second reached gate 50 and rejected
a stale parity lockfile under `--locked`. Regeneration added the PNG/gzip records
already present in the reviewed formats lock, without changing existing package
versions. Locked native/WASI parity then passed and rejected its altered-opcode
control. Neither partial report is relabelled PASS; this final full run supersedes
both and repeats every gate on the final inputs.

## Reproduce

Use Rust/Cargo 1.90.0 with rustfmt, Clippy, `wasm32-unknown-unknown` and
`wasm32-wasip1`; Python 3; Mono; a C++98-capable compiler; Node 24.19.0; matching
wasm-bindgen 0.2.129; and Playwright 1.56.1 with Chromium 141.0.7390.37.
Prepare the pinned runtime's locked dependencies as described in
[runtime-bridge.md](runtime-bridge.md). Install the Creator browser tools using
its checked-in package lock. Run from the repository root:

```sh
mkdir -p /tmp/wonderland-b-builds
TMPDIR=/tmp/wonderland-b-builds \
CREATOR_WEB_DIST_DIR=../../../creator-web-verified-dist \
CREATOR_PLAYWRIGHT_MODULE=/absolute/path/to/node_modules/playwright \
CARGO_BUILD_JOBS=2 \
python3 tools/swarm-b/verify.py \
  --cargo /absolute/path/to/cargo \
  --wasm-bindgen /absolute/path/to/wasm-bindgen \
  --logs-dir /absolute/path/to/verification-output \
  --with-parity --with-source-oracle --with-runtime-bridge --with-creator-web
```

An omitted Creator distribution override uses its normal `dist` directory.
Relative overrides resolve from `tools/creator-web`; absolute paths retain
their location. Unique runner-owned temporary targets bound compiler storage.
Native runtime test artifacts are removed only after their completed logs are
retained, before independent portable builds. No gate is skipped to reclaim disk.

The input digest binds code, locks, fixtures, tests, compatibility metadata,
the original object/avatar corpus and reader/compiler dependencies, family
source manifests including symlink target bytes, and the frozen numeric probe.
Generated browser output and evidence documentation are excluded so recording
the result does not create a self-referential digest. The final equal-digest
comparison is required for aggregate PASS.

## Remaining integration and qualification boundaries

- All 30 native EOD handlers are implemented, but the census still records
  **16 required production provider integrations with zero qualified**, and no
  full original-runtime EOD qualification. Native fixture providers establish
  operation/recovery behavior, not production storage or economy correctness.
- Five unchanged C# handlers have full controlled component comparisons.
  Casino/social/service oracles have their precisely stated source/helper
  boundaries; the casino probe does not execute complete original casino
  handlers or production providers. Recorded RNG and policy differences remain
  explicit rather than being relabelled parity.
- A's public interfaces still lack the full interaction action/special-result
  frame/resume path, query advertisements/action-string/mutated-state outputs,
  and resumable instruction debugging. Whole-tick isolated replay and stored
  inspection do not imply instruction break/step or a running-lot editor.
- Primitive-specific Volcanic dialogs, full OBJD/TTAB authoring UI, contextual
  legacy sprite/draw-group UI, indexed-source patch-chain generation and the
  other explicit utility/format dispositions remain as listed in
  [tools.json](../compat/tools.json). Source corpus scope does not qualify absent
  original TS1 assets, arbitrary scene retargeting or complete animation playback.
- Renderer/audio/presentation, authoritative transport, integrated whole-object
  gameplay and physical browser/device/screen-reader qualification remain with
  their actual providers and integration gates. D begins from the preserved UI
  branch after this B handoff; this record makes no D completion claim.
