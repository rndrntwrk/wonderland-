# Swarm B buildout recovery handoff

> **Historical record — recovery completed on 2026-10-05.**
>
> The saved checkout was recovered, every buildout review finding was resolved,
> and the full fresh run passed **47 gates, 439 Rust tests and 48 Python tests**.
> The implementation and evidence are published in draft PRs
> [#13](https://github.com/rndrntwrk/wonderland-/pull/13),
> [#14](https://github.com/rndrntwrk/wonderland-/pull/14),
> [#15](https://github.com/rndrntwrk/wonderland-/pull/15),
> [#16](https://github.com/rndrntwrk/wonderland-/pull/16) and
> [#17](https://github.com/rndrntwrk/wonderland-/pull/17).
>
> Current branch: `swarm-b/buildout-verified`; current commit:
> `3d6b04e0a600cb5f947145123cddf6cb2c75c5d7`.
> See the [completed verification and handoff](https://github.com/rndrntwrk/wonderland-/blob/3d6b04e0a600cb5f947145123cddf6cb2c75c5d7/docs/swarm-b/BUILDOUT-VERIFICATION.md)
> for the delivered scope and remaining integration work. The draft stack is
> unmerged. This recovery branch still contains only the historical handoff;
> the implementation is on the branches above. Everything below describes the
> earlier outage and is retained for provenance.

Recorded 2026-10-05 UTC after the execution service returned `409 environment_offline: Environment is not connected`.

## Status and authority

The user authorized continuing the exhaustive Swarm B implementation for `rndrntwrk/wonderland-` and publishing reviewable draft PRs. Do not merge. This recovery branch contains this handoff only; it does **not** contain the unpublished feature changes described below.

The existing published baseline is draft [PR #11](https://github.com/rndrntwrk/wonderland-/pull/11), head `04f0a407dd81acf0685453e7367764bf75b5c092`, tree `f73294cdb795b20e2ffc5b30c3470e11c35f6b63`. GitHub confirmed it remains open, draft, unmerged, and mergeable during the outage.

Continue the existing W01/W06/W16 architecture. Original C# and assets, Swarm A implementation, shared contracts, and the root workspace are outside the mutation scope. Private EOD implementation stays native-only; formats, content and the runtime bridge stay portable. Root alone stages, commits, changes refs and publishes. Independent agent files share one checkout.

### Last confirmed workspace

- Repository: `/workspace/scratch/378e4c36af7b/wonderland`
- Current branch: `swarm-b/games-sprites-release`
- Local committed increment: short commit `741e195`, message “Add bounded OBJf semantics and source-selected lifecycle inventory”
- Local branch `swarm-b/objf-lifecycle-catalog` points to that commit.
- That commit contains 744 reviewed files, 139,078 insertions and 8,148 deletions. It was **not pushed** before the outage.
- Other streams have saved but uncommitted changes. Inspect the actual working tree before editing; do not reset, clean, overwrite, or recreate it.
- Original source pin: `4c6b3e8f5835b228723caea3c9f683c62f244f73`
- Actual simulation pin: `8a0e251d19e222a0a6833d7408ca629f674e1729`, unchanged on Swarm A draft PR #6.
- Implementation design/plan: `docs/swarm-b/BUILDOUT-DESIGN.md` and `docs/swarm-b/BUILDOUT.md`.
- Local progress, task briefs, reports, patches and reviews: `.superpowers/sdd/BUILDOUT/`.
- Rust/Cargo 1.90: `/root/.cargo/bin/cargo`; the default PATH Cargo was older.
- Use nonincremental builds and no debug information. Fresh package target directories are required for the final aggregate: reused targets produced stale rlib/rmeta results twice during this turn.
- Last disk observation: approximately 2.0 GiB free. Final aggregate already uses temporary, sequential package targets with cleanup.

## Task 3: bounded OBJf semantics and lifecycle inventory — committed and approved

Implemented a bounded lossless codec in `crates/legacy-formats/src/semantic/objf.rs`, integrated through `DecodedSemantic::Objf` and existing cooker dispatch.

`Objf` preserves padding, version, condition-before-action entries, and trailing bytes. Only supported `fJBO` magic is admitted. Counts, complete declared slices, retained allocations and full encoder output are bounded before allocation. Strict magic admission and lossless writer behavior are documented differences from the original reader/writer.

The content census retains decoded function tables and exact resource ordinals. The object inventory selects same-ID OBJf when UsesFnTable is nonzero, otherwise OBJD fields. Missing, unsupported or ambiguous required tables do not silently substitute OBJD. Source-only references remain explicitly unapplied structural evidence.

Two independent-review findings were fixed and re-reviewed:

1. Required-table selection must consider the raw resource cardinality as well as successful decodes. A malformed duplicate in either order must not appear uniquely supported; raw and decoded ordinals must match, and alias consistency must include raw OBJf identities.
2. Every OBJf reference must retain its resource ordinal, including repeated source-only table IDs.

Confirmed verification:

- Formats: 96 passed, 3 optional corpus tests ignored in the ordinary suite; fmt, strict Clippy, and browser-target library check passed.
- Content: 14 passed; fmt, strict Clippy, and browser-target check passed.
- Cooker: 24 passed; fmt and strict Clippy passed. The authored critical OBJf import/cook/load/reopen path passes and malformed declared counts reject.
- Catalog: 14 passed, including six new OBJf regressions; regenerated object-census `--check` passed.
- Independent binary reconstruction matched all 471 original tables and 14,685 entries.
- Independent generated-data review matched all 1,820 cohort OBJf references and 1,817 matrix entrypoints to exact source evidence.
- Review `task-3-review.md` is APPROVED, with no remaining P1/P2 finding.
- Logs: `/workspace/scratch/378e4c36af7b/swarm-b-objf-verification`.

Corpus denominator remains 745 IFF/PIFF paths, 730 source hashes, 1,327 OBJD rows, and 1,602 leaves. There are 456 leaves with OBJf references, 1,315 with primitive dependencies, 568 with route dependencies, 80 with missing/unsupported private references, and 1,339 with shared-scope references. All gameplay qualification remains unverified.

`docs/compat/content-corpus.json`: 8,218,357 bytes, SHA-256 `f5448fac68278e237b36b5a30705395b58b30152df4042cfca67d0eb8ab2878f`.

Committed scope: semantic codec/dispatch/tests, cooker OBJf test, content/object census tools and tests, OBJf/object coverage docs, generated content/object matrices, 730 cohort descriptors and selected chair/bed/appliance descriptors. No original source or assets were changed.

## Task 1: three cooperative EOD handlers — implemented, additional fixes pending

Saved implementation:

- `crates/eod-runtime/src/games/{mod,paperchase,pizza,maze,rng}.rs`
- `src/game_host.rs`
- Integration in `src/{host,protocol,lib,checkpoint,registry}.rs`
- `tests/cooperative_games.rs` and narrow checkpoint-version maintenance in `tests/adversarial.rs`
- `tools/swarm-b/eod-census.py` and generated registry/coverage

Implemented PaperChase (three seats), PizzaMaker (four stations), and TwoPersonJobObjectMaze (two private roles). Shared games tick once per authoritative host tick. Native controller tickets bind host scope, epoch and the recorded invoker; UI messages cannot deliver VM callbacks. Role/seat/tuning inputs come from the trusted VM integration.

Checkpoint format 3 retains private game state, hands, phase, RNG streams and role bindings while retaining formats 1/2 compatibility. Restore detaches transports/controllers and pauses each group until its retained bindings return. Rejected output admission rolls back game state, RNG and identity counters. Private native randomness is explicitly not C# System.Random sequence parity. WarGame remains deferred because it needs a real timer callback contract.

Last confirmed focused result: **21 cooperative acceptance tests passed**. Whole-package tests, formatting checks, strict Clippy and independent final review remain required. Generated census now distinguishes 8 native-enabled registrations and 22 unsupported registrations; all 30 still lack full original-application/provider qualification.

### Accepted pending fixes — not applied before outage

- **Pizza deadline reachability:** reject impossible checkpoint timers/frames while preserving legitimate negative/zero tuning. Lobby is initial timer 0 or later -1 with zero frames; Phone must fit its configured deadline or the post-ring -1 state; Contribution and Break must fit their own configured timers; Bake must be -1 with zero frames. Add targeted corruption tests.
- **Pizza contribution multiset:** each selected contribution was immediately returned to the pool, and no pool draws occur during Contribution/Bake/Break. Require the pool multiset to cover contributions. Four forged identical bonus contributions cannot be admitted when only two such cards exist.
- **Maze preinitialization rebind privacy:** rebind currently reconstructs map/cell regardless of preserved initialization flags. A Charisma-only Waiting checkpoint must not receive a cell before Logic existed. Preserve the flags when rebuilding private output; add a targeted regression.
- Document `COOP-ADMISSION-ATOMIC`: original Pizza updates tuning before duplicate-station rejection, while native rejected admission leaves state unchanged.
- Complete `docs/swarm-b/cooperative-eod.md`, report/allowlist, formatting, whole suite and review.

The attempted deadline-test patch failed before application with environment_offline. No associated test build started.

## Task 2: guarded SPR2 creator workflow — implemented, whole-package verification pending

Confirmed saved files:

- `tools/creator/Cargo.toml`, `Cargo.lock`
- `src/{lib,main,resources}.rs`
- New `src/editors/{mod,sprites}.rs`
- New `tests/tools/authoring.rs`

Implemented `SpritePackage::{from_json,to_json,set_pixel,set_palette_color,set_alpha_mode}`, `ResourceDocument::{export_sprite,import_sprite}`, and five CLI commands:

- `sprite-export INPUT SPR2_ID PACKAGE_JSON`
- `sprite-import INPUT OUTPUT PACKAGE_JSON`
- `sprite-pixel PACKAGE_JSON OUTPUT_JSON FRAME X Y INDEX ALPHA DEPTH`
- `sprite-palette PACKAGE_JSON OUTPUT_JSON PALETTE_ID INDEX R G B`
- `sprite-alpha-mode PACKAGE_JSON OUTPUT_JSON exact|source`

Strict object-only schema 1 binds whole-source and per-resource SHA-256. It rejects unknown/duplicate/positional/wrong-type inputs, requires explicit nullable depth, and bounds total input/planes/frames. Source/frame/palette identities and ordering are immutable; dimensions, positions, flags 1/3/5/7 and matching bounded planes can be edited. Same-IFF required palette discovery precedes the full sprite decoder.

No-op and palette-only edits preserve raw SPR2 bytes. Exact alpha is default; explicit source quantization reports changed values. Export **and import** reject source pixels that the source encoder cannot represent without changing hidden RGB. Late invalid edits and stale guards publish nothing. Indexed resource maps are rebuilt and reopened atomically.

Last confirmed result: **17 authoring acceptance tests passed**, including actual recooking/cache-identity controls, tight caller bounds, short-row representability, and aggregate writer/reopen allocation accounting. The latter fixed a real regression where a 2,400,000-byte caller limit was applied separately to writer workspace and retained live state. The shared `commit_candidate` helper changed, so the entire existing creator suite must pass before publication.

A formatting operation completed; a fmt-check result is still required. The new full-suite target `swarm-b-creator-sprites-verify.xhOcp3`, exec session 35709, was last seen compiling dependencies; its completion is **unconfirmed**. Strict Clippy, independent review, report and final package counts are pending.

A complete `docs/swarm-b/creator-sprites.md` write was submitted in a pending tool call; application is **unconfirmed**. Check file existence/content before retrying. Existing creator.md and sprite-authoring.md were not yet updated. Generic raw SPR2 inspection remains raw and must not claim semantic validation. No live-renderer cache eviction is established by the offline recook tests.

## Task 4: verified cooked release into the real runtime — core implemented, executable path pending

Confirmed saved files:

- Bridge Cargo.toml/lock (cooker dev-dependency and integration targets)
- `src/{budget,content,lib}.rs`
- New `src/{cooked,cooked_json,cooked_metadata}.rs`
- `tests/integration/swarm_b_runtime/content_import.rs`
- New `cooked_release.rs`, `cooked_rejection.rs`, `support/cooked_fixture.rs`
- Runtime assembly runner's cooker symlink support
- A child-created metadata integration test with 15 tests was visible, but not yet built/targeted

Portable APIs:

- `PreparedDraft::from_json(draft,manifest,limits).seal(&[PackBytes{digest,bytes}],limits)`
- Final `CookedRuntimeBindingV1` and bounded `canonical_bytes`
- `PreparedRelease::from_binding(binding,&trusted_binding_digest,manifest,limits)`
- `required_packs`, `plan`, `load` returning an actual ContentSet and distinct cooked provenance report

Final bindings require exact simulation/source pins, manifest digest, ordered named scopes, explicit resource membership, object GUID/OBJD identity, private/semiglobal/global associations, tuning identity, full normalized runtime metadata/options, and the expected actual content/tuning descriptor hashes. Verify hashes/closure/packs/membership before semantic decoding. Source and cooked imports share internal conversion without manufacturing source-IFF identity. Task 3's semantic OBJf decoder feeds the narrower actual-runtime bound.

Fresh focused command passed:
```sh
python3 tools/swarm-b/runtime-bridge.py --cargo /root/.cargo/bin/cargo \
  /workspace/scratch/378e4c36af7b/runtime-bridge-assembly \
  test --locked --offline --no-default-features \
  --test cooked_release --test cooked_rejection --test content_import
```

Result: **4 cooked release + 6 rejection + 10 source import tests passed**. One authored fixture cooks IFF+PIFF+tuning, removes source files, then loads verified packs and executes real query/snapshot/accepted-tick behavior. An earlier source-content first-empty-GLOB regression was fixed and tested.

Not yet implemented/saved: CLI release commands, replay scenario module, cooked native/WASI parity tools, `docs/swarm-b/cooked-runtime.md`, positive shared-scope fixture. The main.rs CLI still contains the older source-replay/capabilities commands. The attempted task-4-report write is unconfirmed.

Planned commands: release-prepare, release-plan, release-load, release-replay, with explicit binding SHA, bounded scenario, and a flat release directory containing `manifest.json` plus digest-named `.wlp` files. Planned aggregate gate:
```sh
python3 tools/swarm-b/runtime-bridge-cooked-parity.py --assembly DIR --cargo /root/.cargo/bin/cargo
```

Require pack-only native/WASI execution, literal query/replay outcomes and changed-output negative controls. The intended authored probe expects query count 3, temps [37,41], and replay attributes [41]; these are **planned assertions, not a completed parity result**. Full default bridge checks must run after creator sources freeze. Preserve the existing source parity gate.

## Task 5: original C# handler comparison — executable, review correction required

Saved scope: `tools/swarm-b/eod-source-oracle.py`, `fixtures/eod/source-oracle/**`, native `crates/eod-runtime/examples/source-oracle.rs`, and `docs/swarm-b/eod-source-oracle.md`.

The tool compiles 18 byte-identical pinned original sources under Mono, with explicitly documented VM/transport/storage/unregistered-handler adapters and controlled asynchronous scheduling. It runs Timer, DanceFloor, Signs, Scoreboard and PermissionDoor scenarios through original and native implementations.

Last executable evidence before the open finding:

- 27 scenarios and 226 stages
- 223 common stages; 106 VM events, 116 private UI effects, 61 logical provider effects
- 77 literal channel assertions
- Three fully declared native policy differences: pre-load Signs write, malformed UTF-8 Signs write, malformed Scoreboard load
- Two identical executions of both implementations; nine comparator mutation controls rejected
- Comparison SHA-256 `599b98108be8c7d4cf7460ee4c9525c84a3b6147c80a20c8302caf54ea96cb15`
- Latest strict protocol/declaration unit suite: **20 passed**
- Rustfmt and strict example Clippy passed before the final boundary fix
- Latest policy-guard evidence: `/workspace/scratch/378e4c36af7b/eod-oracle-policy-guard`

### Open P2 — confirmed false pass; fix not applied

The native adapter uses `execute(...).and_then(finish_stage)`, so expected operation errors skip stage completion. Its provider-error path also skips residual draining. Terminal policy scenarios can therefore conceal queued VM/private/provider effects.

The independent reviewer injected a real NativeHost disconnect alongside expected PluginNotReady in temporary adapter copies. The old oracle still passed all 226 stages despite queued VM -1 and private eod_leave. Draining those effects revealed the mismatch and made the comparator reject. This is a confirmed oracle gap, **not an unexpected production behavior difference**.

Accepted correction:

- Always finish the stage after an operation error.
- Observe queued VM/private effects and pending writes across the required checkpoint/provider boundary.
- Drain residual effects after provider failure.
- Preserve primary and secondary errors explicitly.
- Add native example regressions for queued disconnect under primary error, a no-UI Scoreboard save under primary error, and provider failure after earlier load-generated UI.
- Add those native example tests to the oracle CLI gate and include their real counts in aggregate evidence.
- Rerun targeted tests/oracle and obtain independent GREEN review before approval.

The boundary correction and tests were **not applied** before the outage. Review remains changes required. A local task-5-review.md could not be written while offline. Existing evidence must not be called final complete coverage until this finding is resolved.

## Task 6: aggregate verification and publication — partly integrated

Saved `tools/swarm-b/verify.py` edits add the 18 exact original EOD dependency paths to frozen-input hashing, the comparator Python gate, original EOD executable gate, and updated source-oracle help.

Saved `test_verification_inputs.py` regressions bind every source dependency's addition/mutation/deletion and original symlink target bytes. New cases were observed RED (19 failures), then all **4 input-stability tests passed** after the implementation.

Pending:

1. Confirm saved files after reconnection and resume the accepted fixes above.
2. Finish remaining CLI/docs, actual cooked native/WASI execution and full package verification.
3. Independent review of creator, game recovery and cooked runtime; close the oracle finding.
4. Integrate cooked parity command and real native oracle-example test counts into verify.py.
5. Freeze sources and run:
   ```sh
   bash tools/swarm-b/verify.sh --with-parity --with-source-oracle --with-runtime-bridge
   ```
   Use fresh package targets; require input digest unchanged before/after.
6. Write BUILDOUT-VERIFICATION.md/JSON, update README/package status and preserve prior historical evidence.
7. Commit separate reviewed increments; publish draft follow-up PRs, verify fetched remote trees equal tested local trees; do not merge.
8. Leave a clean local working tree after aligning publication refs.

Do not add the above focused counts to the old aggregate and call it a new result. The prior published 44-gate /347-Rust /19-Python result belongs only to the previous input digest `1e3a4d35e82aeca4e73c78154cf50ee78f404a6912c05f867c27daf44e2a9cf6`.

Publication helper `/workspace/scratch/378e4c36af7b/github-transfer.py` exports immutable commits through authenticated Git Data APIs. Its whitelist still needs checking/extension for the Task 3 census/cohort/test paths: the attempted edit after the commit failed on the first disconnected exec call. Generated JSON is large; split tree updates within the API payload limit and require final tree equality. No feature tree transfer occurred before this handoff.

## Scope that remains outside demonstrated completion

Full interaction check-tree/queue integration, running-lot preview, routing/rendering, browser delivery/cache lifecycle, original full-application/UI execution and production persistence/effect providers remain distinct acceptance gates. Eight native-enabled EOD registrations are not eight qualified original applications. Structural object dependencies do not make the 1,602 gameplay leaves complete.

This handoff is a durable record of confirmed progress and exact unfinished work. It does not replace the unavailable source checkout or claim that unpublished changes have been backed up by this branch.
