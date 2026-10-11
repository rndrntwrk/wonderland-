# Content and native EOD integration candidate

This branch composes Swarm B19 (`60c121b8`) with the renderer and native Buy/Build
integration at `4e5de6c1429afcd97361c6e3c096fe8277a01aad` (PR #56).
It does not deploy, merge the default branch, or claim complete game readiness.

## Reconciliation

The branches copied common content sources without sharing those additions in Git
ancestry. Treating every add/add conflict as a new implementation would overwrite
later runtime hardening. The recovered B17 revision `3d6b04e0` provides the exact
content baseline: unchanged files take B19's changes; B19-unchanged files retain
the integrated F/A changes; files changed on both sides receive a three-way merge
and review. All 45 conflicts were explicitly classified.

Preserved changes include content import bounds, global interaction metadata,
TTAB advertisements, archive validation, skeleton hierarchy validation and queue
semantics. The merged runtime exposes the actual `sim_core::interactions` module
through the bridge. It does not ship the temporary `wonderland-interactions-check`
crate as a second simulation implementation. The native-only EOD library joins the
native workspace, not the browser's dependency graph.

The imported creator/runtime family includes guarded source inspection,
interaction projections, Vitaboy legacy codecs and round trips, mesh and animation
interchange, city-image authoring, native EOD handler families and original-source
fixtures. Native handlers being present is distinct from their transport/provider
and browser UI being connected.

## Evidence and limits

- The full workspace and every target compile with the pinned Rust 1.99 compiler.
- The optimized browser and worker WASM build successfully through Trunk.
- The compiled creator CLI imports an authored OBJ, exports GLB, and imports it
  back to byte-identical FSOM. A stale source hash is rejected without creating
  the destination file.
- Fresh compiled-browser Buy/Build and checkpoint-tail journeys pass against the
  controlled gateway and real Rust authority/replica; purchases remain RAM-fixture
  transactions, not durable production accounting.
- The source-family executable records 576 routine/context executions across
  chair, bed and appliance resources in both TS1 and TSO modes. Every family
  explicitly reports `complete_gameplay: false`. Missing global/semi routines,
  animation metadata, normalized routing slots and topology remain visible in
  its output; no successful gameplay is fabricated from isolated routine runs.
- `SIM_CORE_REVISION` remains the existing recipe compatibility baseline, not a
  claim that this integrated executable equals that historical Git revision.
  Release evidence must bind the actual source tree and built artifact hashes.
- Original content coverage, native EOD provider/transport wiring, corresponding
  browser interactions, independently authenticated multiplayer and production
  persistence are still separate acceptance work.

PR #56's previously pending full hosted browser checks are now verified: Native
browser run 38124670691 and Browser UI run 38124670597 passed. All 37 recorded
source hashes and both WASM hashes matched the downloaded artifacts. Those
receipts qualify PR #56 only; this new integration needs its own hosted run.
