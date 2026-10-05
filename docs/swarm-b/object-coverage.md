# Checked-in object corpus and provider handoff

## Result and acceptance boundary

The source inventory contains **1,327 decoded OBJD definitions and 275 source-only cohorts**, giving **1,602 deterministic leaf tickets**. Every leaf inherits explicit **unverified** enter/use/cancel/leave/save/reconnect evidence. No object gameplay parity is claimed. W06.2 and W06.4 remain open until actual content resolution, VM/routing/animation/snapshot/reconnect providers and original-runtime traces satisfy their acceptance cases.

The inventory denominator is **745 checked-in IFF/PIFF paths at source commit `4c6b3e8f5835b228723caea3c9f683c62f244f73`**. Identical file bytes share one source cohort, retaining every path alias: there are **730 distinct source SHA-256 hashes**. This is an exhaustive inventory of that checked-in subset. The W00 full installation baseline, shipping patch manifest and installed catalog denominator are unavailable. The content census also records five XML tuning OTF files and distinguishes a font named `.otf`; these are not object-source denominator entries.

The pinned denominator comes from `git ls-tree -r --name-only <source-baseline>`. New commits, authored cooker fixtures and current branch HEAD do not enlarge it. The generator verifies every source SHA-256 and byte count, exact path-set coverage and absence of modifications to pinned original assets/source anchors. A new installation baseline requires an explicit scope revision.

## Generated records

| Artifact | Purpose |
|---|---|
| `docs/compat/content-corpus.json` | Rust parser output: source hashes, resource ordinals/IDs, OBJD fields/GUIDs, decoded BHAV opcode histograms, TTAB entries, global names, patch metadata, bounds and exclusions. |
| `docs/compat/object-matrix.json` | Compact normalized rows with stable leaf IDs, cohort references, local object IDs/entrypoints, effective-identity gates and shared scenario/ticket templates. Approximately 1.66 MB. |
| `fixtures/objects/cohorts/index.json` | Every distinct source cohort, all source path aliases and leaf references. |
| `fixtures/objects/cohorts/<sha256>.json` | One copy per source hash of source/resource/OBJD/BHAV/TTAB/primitive registration evidence, object dependency indexes and provider requirements. These contain metadata only. |
| `fixtures/objects/core/{chair,bed,appliance}/descriptor.json` | Three concrete W06.2 source descriptors, with local IDs, selected TTAB entries, exact opcode registrations, unresolved resource gates and integration checklist. |
| `tools/swarm-b/object-census.py` | Deterministic metadata consumer, source-baseline validator and exact leaf-ticket expansion. It does not parse legacy asset formats. |

Object row identity is `sha256:<source-sha256>:OBJD-<chunk-id>`. Source-only cohort identity is `sha256:<source-sha256>:SOURCE`. Leaf IDs derive deterministically from those identities. They are source identities; **`effective_content_id` stays null**. No directory ordering, filename ordering or deduplicated alias order is treated as shipping patch precedence. Concrete ordered manifest resolution must establish effective object identity before runtime acceptance.

Every row's `scenario_status_ref` resolves to a six-entry template in the same matrix. Every `ticket_template_ref` resolves to the declared contract gate, predecessors, outcome, test status and review fields. `--leaf <leaf-id>` materializes an exact ticket including source anchors, allowlist, fixture path, named acceptance test and all six evidence statuses. Those named Rust gameplay tests are planned acceptance targets; they do not exist merely because inventory generation succeeds.

## First source descriptors

Selection requires both an actual checked-in filename and a decoded OBJD label to identify the requested family. This selects a concrete candidate, without assigning semantic behavior to a name or declaring its execution correct.

| Requested family | Checked-in IFF under `TSOClient/FSO.Content.TSO/Content/Objects/` | OBJD / GUID | Source SHA-256 | Raw TTAB / SLOT IDs |
|---|---|---|---|---|
| Chair | `Chair_fso_Bouncy_Beach_Ball.iff`, label `FSO - Chair - Bouncy Beach Ball` | `16807` / `0462db31` | `ab9975d947e64ea19ac194679dee6080f99a5b4c0a4c89f03f719d75113d57a7` | `129` / `128` |
| Bed | `k8capbedts.iff`, label `Caprize Double Bed` | `16831` / `f8ea4345` | `c8289640be973e53a79a7a0d4430809f8e306cc7e6b38bec0c7eaa187a5808bb` | `130` / `0` |
| Appliance | `k8oblfridgehd.iff`, label `Oblique Kitchen Fridge` | `16809` / `46ea4345` | `7a2eb22e4958dc12bce6b470a263e87ecfa0cd57019deafd3efb24270a41a145` | `129` / `128` |

The chair descriptor preserves the decoded GLOB name `skillobjects`, TTAB action/test references and a source opcode 45 (`VMGotoRoutingSlot`) dependency. The bed and fridge likewise include their decoded TTAB action/test references and source routing/animation opcode requirements. SLOT `0` is a raw field value, not proof that a bed requires no slots. OBJf resources remain opaque in the census; the chair's `UsesFnTable=1` is an explicit unresolved lifecycle-function gate.

The descriptor lists the original-runtime, native/WASM, content, queue, routing, animation, rendering and snapshot/reconnect work needed to execute these cases. It contains no original asset bytes and does not replace catalog BHAVs with hardcoded chair/bed/fridge behavior.

## Dependency evidence and limits

`VMThread.cs` lines 526–540 and 566–578 establish that instruction opcodes at least 256 are routine calls, with global/private/semiglobal ranges, and that missing primitive registrations follow a source-defined result path. `VMStackFrame.cs` lines 160–162 uses the same routine scope ranges. `VMContext.cs` supplies the exact primitive registrations and TS1/TSO branches. The generated matrix hashes these source anchors; cohort records preserve source line anchors for registrations.

For object rows, the generator follows statically encoded private calls from decoded OBJD fields and the exact selected TTAB. For source-only cohorts, every decoded BHAV/TTAB is inventoried as source evidence without applying it to an unknown target. The shared per-source BHAV opcode histogram is preserved once; per-object dependency lists refer to its routine IDs and opcodes. These are dependency upper bounds over all instructions in the referenced routines, including potentially dead branches. They do not establish executed control flow, slot selection, animation choice, motives, EOD plugin ID, indirect-call targets or runtime availability.

The inventory currently has 1,012 leaves with decoded primitive dependencies and 491 with the exact registered route handler opcode 27 or 45. Eighty-one leaves contain missing or unsupported private references, and 1,018 contain global or semiglobal references. Fifty-six distinct source-only cohorts contain decoded BHAV or TTAB evidence. These counts describe structural evidence, not completed implementations. Global/semiglobal resources, unapplied PIFF additions and opaque OBJf lifecycle functions remain integration gates. The original content census's strict duplicate-envelope and semantic-layout exclusions remain recorded in cohort source parse errors.

The Python tool only consumes bounded JSON output from the real Rust content parser. It has a 128 MiB JSON input limit, 256 MiB source-file limit, 10,000-file limit and 100,000-entry top-level resource-vector limit. The Rust census retains its own bounded parsing/retry evidence. Unsupported or excluded resources are carried as unresolved metadata, never silently considered successful gameplay fixtures.

## Reproduction and integration

From the repository root:

```sh
cargo run --manifest-path crates/content-ir/Cargo.toml --bin content-census -- . docs/compat/content-corpus.json
python3 tools/swarm-b/object-census.py
python3 tools/swarm-b/object-census.py --check
python3 -m unittest discover -s tests/compat/catalog -p 'test_*.py'
python3 tools/swarm-b/object-census.py --leaf OBJ-ab9975d947e64ea19ac194679dee6080f99a5b4c0a4c89f03f719d75113d57a7-OBJD-16807
```

The eight catalog tests cover deterministic source aliases/row counts, exact dependencies, all six unverified statuses, stale baseline/hash/path rejection, bounded JSON input, stale generated files, complete real-corpus coverage, exact ticket expansion and preservation of the pinned denominator after a new branch HEAD and authored fixture commit in an isolated test repository. `--check` reproduces all generated JSON and rejects changed corpus hashes, source hashes or descriptors. It is an inventory check, not `catalog_parity`.

Before a leaf can be accepted, the content owner must provide its ordered effective manifest and W00 contract/installation scope; Swarm A must execute the actual routine/primitive/queue/route/snapshot pipeline; Swarm C must provide animation events and renderer evidence; Swarms E/F must provide reconnect and controlled original-runtime/native/WASM traces. The descriptor checklist makes these dependencies concrete. Until those providers exist, every enter/use/cancel/leave/save/reconnect evidence entry remains unverified and all commit/PR/reviewer/trace fields remain unset.
