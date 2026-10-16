# Private original-behavior runtime qualification

This is a sanitized result summary. Original assets, behavior bytes, decoded
operands and the detailed private trace are not part of this repository.

## Scope and input

- Pinned repository source contract: `4c6b3e8f5835b228723caea3c9f683c62f244f73`.
- Privately imported original version: `1.1097.1.0`, accepted directly by the
  pinned client; the separate 1239 conversion does not apply.
- Actual TemplatePerson and destination resources, effective source patches,
  global/person-global behaviors, local/global interaction tables, OBJD/SLOT
  metadata and original `tuning.dat` were loaded by the content adapter.
- A fresh TSO `SimRuntime::new` creates TemplatePerson at (24, 24, 1), assigns
  interaction authority and evaluates terrain at (56, 56, 1).
- This setup is an isolated runtime qualification. It does not exercise real
  account admission, owned lot permissions, two browser accounts or rendering.

## Observed result

1. With actor CurrentObject zero, full terrain evaluation returns Walk Here,
   Run Here, Face Here and Point Here. Two identical queries have equal results
   and leave the entire authoritative state unchanged.
2. The selected source Walk is admitted at tick 3, creating the real destination
   helper and enqueueing action 3. It starts at tick 6 through the genuine idle
   behavior.
3. At tick 17 the avatar reaches (56, 56, 1), the route reports Success and the
   selected action reports Finished Succeeded. The original idle action resumes.
4. Authority and replica outcomes and complete state compare equal at every tick
   through tick 302. No external effects occur. The source idle remains queued,
   so total queue emptiness is not the success condition.
5. An independent reviewer inspected the execution log and the probe's unchanged
   menu and authority/replica assertions and confirmed this bounded result.

The retained tightened harness fails on matched runtime errors, thread faults or
external effects, identifies the selected Walk action ID, and explicitly requires
its Finished Succeeded event, exact destination and successful source route.
Its final assertions passed after all 300 replay-compared ticks, ending at tick
302. This replaces the earlier probe's insufficient exit-code-only success test.
The private harness and trace are retained separately, identified by SHA-256:

- Harness `terrain-runtime-probe-4.rs`: `2e8841f70ecc5518e773862fcd1202dace61383483215f50321cee6a1d8907ac`
- Trace `terrain-runtime-probe-4.log`: `3206ca91108f17ae7f25502e4c9ed568cb8320ef7a281de7131783079d95747f`

## Shipping-loader confirmation

A second retained run performs the same assertions after loading the sealed
935-resource release through `PreparedRelease::from_binding` / `load` and
`GameRuntime::from_cooked`. The latter installs the actual interaction tables.
Walk again starts at tick 6 and finishes Succeeded at the exact destination at
tick 17; every asserted condition remains true through tick 302.

- Binding: `4ebafeec89dd2ab71ad2503f45d20da9e8527a62582557dbd18fd3415ebbd16b`
- Manifest: `0aec376fb0a77cc9e76291432c7ffda6400f36bb039faf0eb187f83ed7d14038`
- Sealed pre-table content: `9056c1ff67405a233a0c6275dc82e43841a0847871dad4a2fa8064c5d761af99`
- Table-attached runtime content: `6a805b7bfbb59b241d6032b3f97ed72f4e41180cff9ef26a5952ac714c02e8ab`
- Tuning: `9f99d840274b85333d492f2b9ff14428c7087b23b2c34071ad6096c2aa10bfc2`
- Private harness `terrain-cooked-runtime-probe-1.rs`: `d84eb3b1cf499f7d716f0d2203838ad18a7d5d5bd8c25c11f13c8ca8b23cff9a`
- Private trace `terrain-cooked-runtime-probe-1.log`: `5dfccb33eac1f0a2434c460aca441b0ac8748837b7f1764a6944351f53c774bb`

The sealed loader uses projected scope identities, and attaching interaction
tables changes the runtime content descriptor. These identities intentionally
differ from the direct-import probe; the tuning digest is preserved. This joins
movement qualification to the shipping loader without claiming owned-account
admission or rendered presentation.

## Source startup correction

The initial probe remained queued for 300 ticks because native fresh globals
were all zero. Pinned `VM.Init` sets globals 17 = 4, 20 = 255 and 25 = 4 for both
VM modes. Global 25 enables the source avatar idle behavior that accepts ordinary
input. `SimRuntime::new` now supplies exactly these values. Neither `from_state`
nor snapshot restore rewrites imported state. The two-mode regression and owned
admission startup assertions pass in the full 472-pass sim-core suite.

No original script, patch selection, Main termination or action-dispatch rule
was changed to obtain this result. The new genesis hash differs, so release and
bootstrap qualification must use the final candidate identity.

## Remaining qualification

- Source selection-mode side effects when CurrentObject is nonzero are currently
  an explicit unsupported terrain-query boundary.
- Actual original-content owned-account admission, two-account peer observation
  and the rendered journey require separate proof. The final authored boundary
  rerun passes native player 24, admission 14, recovery 8, host 12, terrain pick 4
  and owned startup 11 tests; the WASM check also passes.
- Route progression currently updates authoritative position/facing but does not
  emit source StartWalkAnimation/Turn timelines. Imported walk/swim name tables
  and generic animation support do not close that presentation gap; acquiring
  original animation clips alone does not establish animated source walking.
- Original assets and decoded reports remain private; this summary does not
  authorize or constitute their publication.
