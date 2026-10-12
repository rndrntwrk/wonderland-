# Existing Rust implementation and remaining integration

Source inspection on 11 October 2026 at planning head `7bd1e6c4d963e6a782b1ea6e1b482d78c9478a63`. No tests were rerun for this report. These findings refine the implementation sequence; they do not establish hosted or full-game acceptance.

## Do not rebuild the merged foundations

The original-source obligations are not a list of wholly absent Rust algorithms. Much of the 3D, avatar, activity and creator implementation already exists. Older README statements can be stale. Inspect the actual call path and missing authority/provider before assigning a rewrite.

### Rendering and content

- `crates/render-3d/src/reconstruction.rs` provides `reconstruct`, `select_params` and `derivation_key`; `reconstruction/simplification.rs` supplies source QEM simplification. `reconstruction/resolution.rs::MeshResolver` handles bounded override/cache resolution.
- Full FSOm decoding exists in `crates/legacy-formats/src/reconstruction.rs::{decode_fsom,decode_fsom_payload}`. `render-3d/src/objects.rs::PreparedFsom::prepare` preserves material/dynamic groups, textures and masks. A single `MeshCandidate` cannot substitute for a complete normalized FSOm object.
- The missing browser connection is the source resource provider assembling DGRP/SPR2/OBJD/FSOR, decoded FSOm and textures into the existing normalized world/render contracts. Production browser use of the reconstruction resolver was not found in this inspection; test consumers are not that integration.
- Camera/mode implementations already exist in `render-core::ViewMode`, `render-iso/src/view.rs::ViewState` and `render-3d/src/camera.rs::{OrbitCamera,FirstPersonCamera}`. The web/world-view path currently selects orbit; full mode switching and first-person integration remain.
- Contrary to the older `world-view/README.md`, a genuine GPU path exists: `world-view/src/renderer/gpu.rs::{prepare_gpu,resolve_gpu_pick}`, `renderer.rs::update_gpu`, `apps/web-shell/src/world_renderer.rs::WorldViewport` and `public/world-gpu.mjs`. It creates WebGL2 color/depth/stencil targets, material passes, ID-buffer picks, generation fencing and context recovery.
- A concrete source-fidelity discrepancy remains: `PreparedTexture::sampling` declares `LinearClampMipmaps`, while the inspected browser upload uses NEAREST filtering without matching mipmap generation. Fix and verify the real material pipeline rather than replace the renderer.
- Room lighting/shadow atlas preparation exists in `world-view/src/lighting.rs` and is consumed by GPU packets/shaders. `render-3d/src/environment.rs` has weather, sky, atmosphere and visibility algorithms. The missing pieces are live source room/light/occluder/weather inputs; the legacy snapshot converter emits `lighting: None`.
- Facade generation and browser worker-backed exports exist. Directory publication/refresh, neighboring subworld assembly and the complete city facade/foliage connection do not follow automatically from an export button. The city screen currently uses its separate terrain/marker path and bundled city_0100 resources.

Required acceptance: load an original multi-material object through the browser provider, exercise embedded/replacement/reconstructed alternatives and cache invalidation; use the existing GPU path across modes/floors/occlusion/materials and actual interaction picks. Qualify hardware performance separately.

### Native avatar animation

The native lane is more advanced than legacy restored snapshots. `native_lot.rs::project_avatars` uses `NativeAvatarPoseHistory`, then `native_avatar.rs::NativeAvatarProjection::{prepare_retained,apply}` to sample imported animation/carry layers and skin geometry at accepted ticks. `world-view/src/renderer/native_pose.rs::native_pose_update` admits fenced pose updates without inventing content revisions.

Actual remaining gaps include container-bound avatars, noncanonical adult/custom/pet rigs and TS1 inline-outfit adapters, which currently reject explicitly. Preserve the working native animation path and keep the original-network snapshot lane distinct.

### Activities and authoritative economy

All 30 registered EODs have native handler implementations in `crates/eod-runtime/src/{registry,native_host,plugins}`. Casino modules contain real game rules and private recovery. This is valuable existing code, not evidence of a deployed activity service.

`native_provider.rs::NativeProvider` deliberately does not supply accounts, balances, inventory or property persistence. The implementations located by this audit were test adapters. `GameRuntime`/`native_lot` do not yet wire the native EOD host, its private checkpoint store and accepted VM/EOD barrier. Legacy gateway forwarding is a separate path.

Secure trade already has frozen offers, property/furnishing fields, acceptance resets, mutual consent, delayed execution and recovery. Preserve `plugins/service/trade.rs` and its types. Tests returning `Reply::Trade { error: 0 }` establish handler behavior, not database settlement.

The real provider must preserve `LotServerGlobalLink.SecureTrade`'s all-or-nothing effects: two-way money and inventory ownership, fresh property ownership checks, community-lot denial, roommate removal/reassignment, new owners, neighborhood move dates, exact furnishing-count checks and post-commit live-lot notifications. A two-property furnished exchange must not send premature roommate removal that duplicates or misplaces objects. A receipt-only mock cannot qualify this provider.

Player-shop handlers and wardrobe controls also exist, including stock/purchase/price/delete operations. Real catalog/stock/account providers, atomic purchase/ownership transfer and player-object sales remain. Source business-lot discount policy was not found in the native authority.

### Remaining original obligations: reuse versus gaps

| Obligation | Existing work to preserve | Actual missing integration/behavior |
|---|---|---|
| Job worlds | JobInfo/person-data mapping; cooperative/nightclub EOD handlers | Grade/role/ignore-aware matcher, job-lot allocation and complete OnlineJobs external provider |
| Dynamic earnings | Newspaper handler, DynamicPayouts request and funds-request decoding | Historical aggregation, durable tuning periods, earning settlement |
| Visitor bonuses/rankings | Directory/query/UI boundaries | Durable visit intervals, Top 100 computation, once-only bonus periods |
| Wear/repair/upgrades | Source fields/codecs, upgrade editor, broken/repair eligibility | Quarter-day wear, repair/harvest arithmetic and authoritative upgrade costs/tuning/reinitialization |
| Property growth | BuildAuthority/BuildState, receipt/reconciliation, browser construction | Original size/floor/roommate cost schedules, object budgets and business discounts |
| Community lots | Source permission/donation controls and command encoding | Native community validator, civic ownership, donation value/wear and mayor property transitions |
| Skills/relationships | SkillPolicy multiplier/write, skill locks, relationship algorithms | Authenticated profile controls, lot/category feeds and durable cross-lot state |
| Complex avatar interaction | Routes/slots, queues, outfit state and native visual projection | Remaining external choice callbacks, held/contained/custom-rig presentation |
| Events/cooldowns | Cooldown handler, source scope modes, atomic request and private checkpoint | Global reservation store, schedules, participation/reward dedup, gifts and game-token attributes |
| Elections | Query/command codecs and original-service controls | Native nomination/vote/count/mayor state machine and durable scheduled announcements |
| Volcanic | Guarded IFF/BHAV/resource editors, undo/redo, meshes/upgrades/city/export; SimDebugProvider | Instruction-level trace/break/step/resume: trace currently rejects; isolated step advances a whole tick |
| Content breadth | Cooker/import/dependency manifests and public distribution gate | Qualified, distributable deployed object-family coverage and complete dependency closure |

`crates/persistence/src/lib.rs` is an isolated MySQL ownership-claim/fencing leaf. It is not the native Cloudflare account/economic/save implementation.

## Implementation consequences

1. Keep Task 1's coverage ledger source-backed and reconcile these reusable modules instead of turning each original requirement into duplicate code.
2. Prioritize the genuine native host/durable acceptance boundary and real NativeProvider/VM-EOD barrier. Private activity state must remain separate from public lot projection.
3. Connect source content to existing full FSOm/reconstruction and camera/mode APIs; retain existing WebGL, picking, lighting and accepted-tick animation.
4. Correct stale documentation from actual source/evidence; do not promote controlled-provider tests to hosted acceptance.
5. Qualify the original player journeys across these boundaries, including running a shop, completing a matched shift, exchanging furnished property, repairing/upgrading an object and debugging a live behavior.

This report neither reduces full-game scope nor claims that every listed reusable algorithm is fully source-equivalent. Existing tests/provenance stay relevant; integrated provider/browser evidence remains required.
