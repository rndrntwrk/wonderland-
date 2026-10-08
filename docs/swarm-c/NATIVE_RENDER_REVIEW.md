# Independent review package: C renderer with accepted native player

Status: author integration and local verification; independent reviewer not yet
available. This is not a review approval. No merge/deployment is included.

Base and reused source: see NATIVE_RENDER_INTEGRATION.md. Review the cumulative
patch against PR47; most files are reused unchanged from PR41. Concentrate on:

1. The two explicitly resolved source conflicts in renderer.rs/scene.rs: both
   native Vitaboy composition and source normal/portal passes must survive.
2. WLB1 appearance_v1.rs: old ordered bytes round-trip, limits/canonical decoder
   remain, unsupported new lighting cannot be dropped or shifted into lot data.
3. renderer/native_pose.rs: accepted newer poses are allowed without granting a
   static-content exemption; stale keys/bytes/owners cannot pass. Failed mutation
   must leave the old frame and its pick tickets intact.
4. renderer/gpu.rs and world-gpu.mjs: schemas 1–3 remain usable, 4 requires explicit
   addressing, interpolation precedes wrapping, per-draw state always resets;
   lightmap and material alpha/stencil/ID paths remain independent.
5. Native WorldViewport use: same camera/DOM lifecycle, no old Canvas2D readback,
   input cancellation, current native entity validation and receipt deadlines.
   Test actual picking during continuous native ticks and GPU loss/recovery.
6. Native screenshot witnesses: ordinary pixels, unchanged gameplay, no fabricated
   clocks/images; keep real audio output and same-voice foreground-load witness.
7. Run both normal CI journeys on one built source tree. No sibling pass certifies
   this integration. Native GLES verifies shaders/state only, not browser behavior.

The handoff retains relevant red/green logs, initial failure records, immutable
input pins, source hashes, all changed source and a directly applicable patch.
The source includes no parent overwrite, optional-scope retirement, raw game/font
addition, registry version change or write-enabled publication workflow.
