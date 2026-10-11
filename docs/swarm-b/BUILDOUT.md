# Swarm B cooperative games and authoring implementation plan

> **For agentic workers:** Use `superpowers:subagent-driven-development` for the
> independently owned tasks. The coordinator alone stages, commits, changes
> refs and publishes. Continue through implementation, review and verification.

**Goal:** Deliver three cooperative native EOD games, usable guarded sprite
authoring, verified cooked-content runtime loading, and original EOD comparisons.

**Architecture:** Extend the existing scoped native host, isolated creator and
portable content bridge. Keep private game state behind native capabilities;
bind authoring and runtime inputs to explicit source/content hashes.

**Tech Stack:** Rust 1.90, standalone Cargo packages, Python 3, Mono C# adapters,
the pinned original source, and the exactly pinned real simulation.

**Spec:** [BUILDOUT-DESIGN.md](BUILDOUT-DESIGN.md), W01/W06/W16 in the supplied
`FreeSO-Parallel-Work-Packages.md`, and [README.md](README.md).

## Global constraints

- Base: `04f0a407dd81acf0685453e7367764bf75b5c092`; working branch `swarm-b/games-sprites-release`.
- Source baseline: `4c6b3e8f5835b228723caea3c9f683c62f244f73`.
- Simulation revision: `8a0e251d19e222a0a6833d7408ca629f674e1729`.
- Do not modify original C# or assets, A-owned simulation files, shared contracts, or the root workspace.
- Native EOD code remains native-only; formats, packs and bridge libraries remain WASM portable.
- Count lengths, identities and retained/temporary allocations before admission; failure leaves caller-visible state unchanged.
- Original-handler comparison, full original application/UI execution and production-provider qualification are distinct evidence claims.
- Implementation and publication of reviewable draft PRs are authorized; do not merge.

## Review focus

1. A shared game must tick once, route only to recorded roles/controllers, and roll back its random state when output admission fails.
2. A checkpoint cannot reattach foreign/duplicate/dangling seats or reveal private hands, maps, door codes or seed state.
3. Sprite package no-ops and palette-only edits preserve unrelated raw bytes; stale or late-invalid edits publish nothing.
4. Cooked scopes cannot mix unbound order, unrelated provenance, wrong codecs or omitted semantic dependencies into a valid runtime descriptor.
5. Original-source comparisons must run unchanged code with deterministic adapters and detect deliberate output mutations.

## Task 1 — Cooperative native EOD host

**Files:** `crates/eod-runtime/src/games/{mod,paperchase,pizza,maze,rng}.rs`,
`src/game_host.rs`, `src/{host,protocol,lib,checkpoint,registry}.rs`,
`tests/cooperative_games.rs`, `tools/swarm-b/eod-census.py`, generated EOD
registry/coverage, and `docs/swarm-b/cooperative-eod.md`.

**Interfaces:** `GameControllerInput::{PaperChase,PizzaMaker,Maze}` with private
seed; `GameControllerRequest { object, invoker, input }`;
`NativeHost::{connect_game_controller,rebind_game_controller,deliver_game_event,join_game}`.
`GameControllerTicket` binds scope/epoch/instance. `GamePlayerInput` records
source slot/station/role and trusted invocation registers. `GameVmInput` permits
only Pizza controller callbacks 6/7/8. Existing receive/tick/disconnect/rebind
and private delivery/checkpoint APIs serve the participants.

- [x] Add failing native-host acceptance tests for PaperChase 3 roles, Pizza 4 stations and Maze 2 roles; match source event names/order and phase transitions.
- [x] Implement one bounded group state and one participant ticket per role; tick groups once and keep all random/private role state native.
- [x] Implement typed controller callbacks, connection/teardown/revocation behavior, source choices/timing and private role UI.
- [x] Extend checkpoint format 3 while preserving old formats; test complete restore/rebind, paused recovery, wrong capabilities, dangling seats and impossible private state.
- [x] Test unknown/malformed input, cross-host/epoch/role attempts, output-pressure rollback including RNG, timeout/disconnect and controller teardown.
- [x] Run the whole EOD suite, fmt and strict Clippy; update exact generated coverage and independently review before root commits.

Published Tasks 1 and 5: [draft PR #15](https://github.com/rndrntwrk/wonderland-/pull/15),
commit `c322e402d5b2c16cc18ed21c87dd4fecf82c28e2`, tree
`c5c644121de6d396139b04704a71774f47ae1c01`. All independent findings are
resolved; the fresh aggregate includes 109 ordinary EOD tests, 3 example
boundary tests, and the complete 27-scenario original-handler comparison.

## Task 2 — Guarded SPR2 creator workflow

**Files:** `tools/creator/src/editors/{mod,sprites}.rs`, light
`src/{lib,main,resources}.rs` integration, `tests/tools/authoring.rs`,
`docs/swarm-b/creator-sprites.md`, and creator documentation.

**Interfaces:** `SpritePackage::{from_json,to_json,set_pixel,set_palette_color,set_alpha_mode}`;
`ResourceDocument::export_sprite(id,&Limits) -> SpritePackage`;
`ResourceDocument::import_sprite(&SpritePackage,&Limits) -> SpriteImportReport`.
CLI commands: `sprite-export INPUT SPR2_ID PACKAGE_JSON`,
`sprite-import INPUT OUTPUT PACKAGE_JSON`,
`sprite-pixel PACKAGE_JSON OUTPUT_JSON FRAME X Y INDEX ALPHA DEPTH`,
`sprite-palette PACKAGE_JSON OUTPUT_JSON PALETTE_ID INDEX R G B`,
`sprite-alpha-mode PACKAGE_JSON OUTPUT_JSON exact|source`.

**Format:** schema 1, whole source SHA-256, exact/source alpha mode, guarded SPR2
identity/version/default palette, ordered frame index/dimensions/flags/raw and
effective palette/transparent index/position with index/alpha/depth hex planes;
guarded required PALT identity/version/reserved bytes and RGB hex colors.
Depth is required-null only without depth. Package cap is 16 MiB plus stricter
caller limits. Frame identity/count/order and palette identity/count are fixed;
dimensions, position, flags 1/3/5/7 and matching planes may be authored.

- [x] Add failing export/import and CLI acceptance tests for versions 1000/1001, no-op exact bytes and an indexed IFF edit.
- [x] Implement bounded required-palette discovery followed by the existing full decoder; preserve raw source metadata and guarded package state.
- [x] Implement strict map-only bounded JSON and pixel/palette/alpha-mode editing; reject unknown/duplicate/positional/malformed inputs before large allocation.
- [x] Apply all guards to one source, validate final palette/frame relations, encode only changed resources, rebuild the original map, reopen and atomically publish.
- [x] Test multi-palette defaults, transparent index 257, resize/position/channel changes, exact alpha 123/124 and explicit source quantization, aggregate pixels and empty-frame limits.
- [x] Test late failure/no partial publication, stale guards, palette-only SPR2 passthrough, unknown byte preservation and recooked cache identity changes.
- [x] Run the whole creator suite, fmt and strict Clippy; independently review before root commits.

Published Task 2: [draft PR #14](https://github.com/rndrntwrk/wonderland-/pull/14),
commit `75c53d1e853df21152261ecb5bf44ddb1088527c`, tree
`e2a04c2cd1bbfae5e4b315b5d9d36b18ce5de0a8`. Independent review and the exact
11-file snapshot agree; 47 tests, formatting and strict Clippy passed.

## Task 3 — OBJf semantic support

**Files:** `crates/legacy-formats/src/semantic.rs` and `semantic/objf.rs`, focused
format/cooker tests, `tools/swarm-b/{content-census.rs,object-census.py}`, catalog
tests, generated content/object/cohort descriptors, and OBJf/object coverage docs.

**Interfaces:** `Objf`, `ObjfFunction { condition, action }`,
`decode_objf(bytes,&Limits)`, `encode_objf(&Objf,&Limits)`,
`DecodedSemantic::Objf`. Preserve pad/version/entries/trailing bytes, require
the supported `fJBO` magic, check counts/lengths and retained allocation.
The existing cooker semantic dispatcher consumes the new variant unchanged.

- [x] Add failing decode/encode/dispatch tests with condition-before-action, unusual version/padding, trailing bytes, truncation and tight budgets.
- [x] Implement bounded OBJf and retained-heap accounting; test exact round trip and deterministic critical semantic cooking.
- [x] Run affected formats/content/cooker suites and portable checks; update any deliberately changed inventory evidence and independently review before root commits.

Published Task 3: [draft PR #13](https://github.com/rndrntwrk/wonderland-/pull/13),
commit `bb4258be8b45aaa22b325a36824f774e39063f59`, tree
`e85cfb254f58cc044d00cbff2a3513f171101a42`. Independent review resolved malformed
raw duplicate ambiguity and missing resource ordinals; fetched tree matches.

## Task 4 — Verified cooked release to actual runtime

**Files:** `crates/content-runtime-bridge/src/{cooked,content,lib,main}.rs`,
bridge manifests/locks as required, `tests/integration/swarm_b_runtime/`,
`tools/swarm-b/runtime-bridge*`, and `docs/swarm-b/cooked-runtime.md`.

**Interfaces:** strict `CookedRuntimeBindingV1`, hash-selected binding decode,
release planning, selected verified pack admission and actual `ContentSet`
load. CLI `release-prepare`, `release-plan`, `release-load` and `release-replay`
seal or consume an explicit binding SHA and manifest. Retain the existing source importer and isolated runtime APIs.
Task 3 supplies semantic OBJf; do not invent an opaque critical-resource exception.

- [x] Add a failing authored IFF+PIFF+tuning cook → source removal → selected pack load → real BHAV/snapshot/replay test.
- [x] Define bounded explicit ordered scope/object/tuning/runtime metadata bindings and verify the binding, manifest, dependency closure, packs and membership before decoding.
- [x] Share internal source/cooked conversion without fabricating original IFF identity; keep provenance reports distinct and validate direct calls and scope ownership.
- [x] Support source-backed OBJf through Task 3 and actual runtime bounds; retain explicit normalized slots/geometry and required tuning semantics.
- [x] Test corrupt/absent/wrong/untrusted/mixed inputs, wrong codecs/criticality, ordering, duplicate keys/scopes, bounds and source-independent complete load.
- [x] Run native and WASI pack-only execution comparison, actual bridge suite, fmt/Clippy and browser-target library check; independently review before root commits.

Published Task 4: [draft PR #16](https://github.com/rndrntwrk/wonderland-/pull/16),
commit `9a1fe5369adfe456c400e6113750733d9c175bb8`, tree
`9846392b88686493a7e3dd0ac5a3190d80ea949e`. The FIFO input finding is fixed and
independently rechecked. All 82 bridge tests, static/portable checks and both
source/cooked native-WASI replay gates pass in the fresh aggregate.

## Task 5 — Five original C# EOD handler oracles

**Files:** `tools/swarm-b/eod-source-oracle.py`,
`fixtures/eod/source-oracle/**`, `crates/eod-runtime/examples/source-oracle.rs`,
and `docs/swarm-b/eod-source-oracle.md`.

**Interfaces:** a documented bounded scenario/trace line protocol for the
unchanged original handlers and the Rust NativeHost. Compare complete ordered
output channels at explicit operations; record original source hashes and
adapter boundaries. Controlled Task scheduling must preserve async source
behavior, particularly EODPersist/Scoreboard load and continuation order.

- [x] Establish scenario coverage for Timer, DanceFloor, Signs, Scoreboard and PermissionDoor, including malformed commands and source parsing/encoding quirks.
- [x] Compile actual unchanged source files with explicit deterministic VM/transport/storage adapters and execute the same scenarios through Rust NativeHost.
- [x] Compare VM/private/persistence/disconnect traces with documented native-envelope projection; separately assert intentional native policy differences.
- [x] Prove the comparator rejects changed public and private outputs; pin the exact source dependency set and repeatable command.
- [x] Independently review the oracle and report any discovered production differences for targeted fixes before root commits.

## Task 6 — Aggregate verification and publication

**Files:** `tools/swarm-b/verify.py`, input stability regressions,
`docs/swarm-b/BUILDOUT-VERIFICATION.md`, machine-readable results and current status.

- [x] Integrate new tests/parity/oracle gates and actual source inputs into the frozen-input verification digest.
- [x] Resolve all actionable task reviews and complete independent cross-component review.
- [x] Run the full aggregate with parity, source oracles and actual runtime bridge; record all test/gate counts and precise unsupported behavior.
- [x] Commit independent increments, publish draft follow-up PRs, fetch each tree and verify exact equality with locally tested content.
- [x] Update the public handoff and leave the local working tree clean.

The full fresh aggregate passed **47 gates, 439 Rust tests and 48 Python
tests**, with unchanged verification inputs.
[BUILDOUT-VERIFICATION.md](BUILDOUT-VERIFICATION.md) records exact accounting,
source comparisons, independent review corrections and remaining qualifications.

## Execution decisions

The established user instruction and current request to proceed authorize this
continuation. Independent file sets run concurrently under the active team
workflow. Dependent APIs, aggregate verification, staging and publication remain
sequential. The existing clean dedicated checkout uses a new feature branch;
the freshly verified previous tree is the baseline. This plan does not create
another design-approval checkpoint or a merge authorization.
