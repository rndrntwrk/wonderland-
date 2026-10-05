# Preserve game capabilities in the browser redesign

The user's instruction to “fix that” authorizes correcting the prototype's reduction of FreeSO/Wonderland. The accepted direction remains a character grid, a large character stage, interactive location selection and actions attached to people, objects and the scene. It does not authorize five fixed identities, three whole looks, six catalog records or furniture-only Build.

## Implementation

1. Replace fixture rules in shared contracts, validation and state with versioned content, account and lot data. Preserve independent head/body/skin/gender, name/description/shard, owned wardrobe identity, catalog metadata, lot dimensions and levels. Separate resource-safety bounds from service policy.
2. Migrate prior preview saves without deleting Sims, balances, possessions or portrait references. Unknown account policy does not grant unlimited live creation. The original server's three-avatar rule remains an adapter policy; saved overflow remains readable.
3. Restore content-driven creator and wardrobe controls, retaining the character-centered interface. Reuse the actual portable avatar composition and skinning libraries. Resolve original collection/outfit/appearance/binding/mesh/texture dependencies, reporting missing content precisely.
4. Drive Buy, Build and view controls from supplied capabilities and explicit readiness. Maintain the complete player-surface map. Missing service or renderer integration is not a removed feature or a fake successful local operation.
5. Review the concrete correction, verify Rust and WASM behavior and update the existing draft PR. Do not merge or deploy.

## Current execution method

The execution workspace went offline before the first implementation command could run. GitHub remained available. Six regression tests were therefore prepared and run through draft PR #10's existing CI. Run 37337696279 passed formatting, executed all six tests and failed for the expected prototype restrictions: catalog membership, balance ceiling, roster size, possessions count, lot dimensions/levels and dropped independent appearance fields.

Implementation is prepared as unreferenced Git trees, reviewed, then committed to the authorized draft branch for CI. Independent source units are prepared separately: the authoring core and the original-content avatar importer have disjoint files. The controller combines their exact file deltas and owns publication. This does not relax the existing formatting, test, clippy or WASM gates.

## Preservation requirements

- Preserve source and owned identities separately. A historical preview portrait is not a real head/body outfit mapping.
- Keep operation/revision matching, atomic accepted receipts, rejection drafts and conflict-safe local saving.
- Source creation policy and field validation must not retroactively invalidate old saved profiles.
- Use source category, price, rotation, bounds, level and permission metadata.
- Keep unsupported actions visible with their reason; never invent an authenticated operation, texture or resource.
- Leave original native game sources intact.
- A complete game-parity claim requires real content and service/renderer evidence, beyond fixture tests.

See [preservation scope](../../design/action-first/preservation-scope.md) and the [player capability map](../../design/action-first/player-capability-map.md).
