# Remaining Swarm B implementation, followed by Swarm D

## Authority and baseline

The user assigned this worker Swarm B and instructed it on 5 October 2026 to continue and take on D after B. This continuation starts at published PR #17, commit `3d6b04e0a600cb5f947145123cddf6cb2c75c5d7`. The previous verification report describes that immutable milestone only; it is not evidence that this continuation or the complete rewrite is finished.

The scope remains the original W01/W06/W16 packages, followed by W11/W12. The frozen original-source pin is `4c6b3e8f5835b228723caea3c9f683c62f244f73`; the current A provider is `8a0e251d19e222a0a6833d7408ca629f674e1729`. C was `f6f78be1fef247f2db47e19f56d94054f0c9e88c` when this continuation began. A 5 October 2026 recheck observed C at `a4c8bbaacde50cc5b1b271639b5fe454ac317e02`, with runtime source `d247ebb94d1d4de79247983b4f62fffb7e06427d`; A and the UI heads were unchanged. D continues the existing UI head `7ee14583ca10ed86511d717566295ea44d481bf7` after B, preserving its established feature identities and direct manipulation workflows.

## Implementation and acceptance ledger

| Lane | Remaining implementation | Acceptance evidence required |
| --- | --- | --- |
| W01 formats/content | Source-preserving Vitaboy encoders, purchasable outfits, BCF/CMX/BMF and missing sprite/string/table layouts, exact dependency closure | Source anchors, bounded malformed-input tests, encode/decode and original-asset comparison, exact float/order checks, native/WASM compilation |
| W06 native EOD | Remaining 22 registered casino, cooperative, music, service, deck, outfit and trade handlers through NativeHost | Real registry construction and event routing, authorization and recovery tests, private provider outbox, source event/timing/payout comparisons; live durable-provider acceptance tracked separately |
| W06 interaction/runtime | Actual content-to-queue adapter, native state/offer projection, accepted intent preparation, meaningful original chair/bed/appliance probes | Tests against the pinned A runtime and exact B action queue, full failed-operation atomicity, actual dispatch results and precise unsupported seams |
| W16 authoring/interchange | Upgrade documents, city images/roads/neighborhoods, mesh/animation and avatar interchange, effective provenance | Guarded candidate publication, edit/export/reimport checks, correct codecs and coordinate policies, actionable validation errors |
| W16 graphical tools | Six workbenches: IFF resources, upgrades, city maps, neighborhoods, avatar/mesh assets and ordered patches; typed editing, source guards, independent bounded histories and actual export | Real uploaded fixture bytes flow through the Creator Rust library; keyboard/responsive browser checks, malformed/stale/over-budget rejection, exported-file validation and three reproduced review regressions |
| W16 debugger | Inspection/watch and isolated preview supported by actual A; explicit extension contract for instruction break/step/trace | Distinguish whole-tick stepping from instruction execution, preserve isolation and source identity; do not claim unsupported pause/history |
| W11 UI after B | Existing Rust/Leptos shell, account/avatar/city/lot/build/social/EOD/settings states and typed intent/projection adapters | Real ack/rejection/reconnect state, focus/IME/keyboard and responsive flows, provider boundaries and screen-state inventory |
| W12 browser after B | GPU artifact selection, verified asynchronous content cache, input/lifecycle cleanup, bounded reconnect/catchup, compatible service-worker generations | Cache corruption/quota/interruption tests; background/offline/reload/GPU-loss checks; measured physical-device qualification remains separate |

## Execution order and ownership

The EOD coordinator integrates three disjoint family implementations. Separate workers own the format/content, Creator/interchange, and interaction/runtime lanes. The coordinator owns graphical Creator, global evidence and publication. Only the coordinator stages, commits, changes refs or publishes. Native private EOD state remains outside common VM snapshots and client projections. Source C# and original assets remain unchanged. Root workspace contracts and A/C/E-owned implementations are not modified under B ownership.

After B-owned code is reviewed and verified, D work starts from the current UI implementation in an isolated branch/assembly; no existing UI feature is retired implicitly. Each publication remains a reviewable draft PR. Merging, production-provider certification and physical-device claims are distinct from implementation.

## Known integration constraints

The current A runtime public query result contains stop, temporary registers, instruction count and diagnostics. It omits action strings, advertisements and changed query state/RNG. RuntimeHost is private, StartBehavior does not push interaction action/special-result frames, and the public API has no resumable pause or executed-instruction history. B will provide exact source-anchored extension requirements and complete all adapters the real public API supports. It will not replace that runtime with a mock and call gameplay complete.

Native account, inventory, budget, durable trade, bulletin, cooldown and city operations require the appropriate providers. Implemented state machines use explicit authenticated native operations, with pending/acknowledged/replayed/failed state tested. A fake provider does not establish production persistence or full original-application compatibility.

## Graphical Creator design

The implementation reference is the generated complete Creator resource-editor concept from 5 October 2026. It uses a dark resource rail, white work surface, indigo selection/actions, a table-based BHAV inspector, a resource-details column, and a compact status bar. Native controls use locally bundled Inter with system sans-serif fallback, 6 px radii, restrained borders and visible keyboard focus. Narrow screens stack resource navigation, editor and details without removing actions. All content and controls are DOM/SVG; no screenshot is shipped as the interface.

Allowed primary copy: Wonderland Creator; Open IFF; Export IFF; Resources; Filter resources; Inspect; Edit; History; Instructions; Control flow; Resource details; Type; ID; Format version; Source hash; Resource hash; Edits validate before export; Local workspace. Filenames, IDs, labels, values and instruction rows come from imported bytes. Empty/import/error/undo states add only text required to complete the actual editing workflow. The concept's illustrative opcode labels/branch values and desktop window chrome are not authoritative bytecode or browser controls: implementation uses decoded source values, explicit return markers and actual browser chrome.

No-op export must preserve the imported envelope exactly. Failed edits and stale guards must preserve document bytes, selection and history. Undo/redo is bounded by an aggregate retained-byte budget. Opening a new file invalidates in-flight older reads. The browser may export a new file; it cannot silently mutate an original disk file or a runtime/provider state.

The design now extends across upgrades, city maps, neighborhoods, avatar/mesh assets and ordered patches. The six navigation destinations retain independent documents and histories. Typed forms use decoded values; uploaded skeletons remain associated with source-bound mesh/animation exchanges. Whole-document JSON is guarded against stale source changes. Dirty-document detection includes hidden workbenches. The [browser verification record](creator-web-verification.md) describes the delivered controls, responsive behavior and source-bound acceptance cases.
