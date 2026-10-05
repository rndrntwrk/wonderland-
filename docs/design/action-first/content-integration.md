# Original avatar content integration

The creator uses the original game's independent head and body outfit keys. Its resource importer, appearance composer and browser renderer are separate from account state and saved characters. Loading resources does not grant account, wardrobe or lot permissions.

## Imported source

| Local crate | Source in this fork | Integration change |
| --- | --- | --- |
| `wonderland-render-core` | Commit `ca79bbd251491277d9fd5838d3f84d247675709a`, `crates/render-core` | Join the browser workspace; preserve overflow checks in root profiles |
| `wonderland-avatar-view` | Commit `ca79bbd251491277d9fd5838d3f84d247675709a`, `crates/avatar-view` | Join the browser workspace; preserve overflow checks in root profiles |
| `wonderland-legacy-formats` | Commit `04f0a407dd81acf0685453e7367764bf75b5c092`, `crates/legacy-formats` | Join the browser workspace |

The imported portable crates retain their MPL-2.0 source and tests. The root `Cargo.lock` governs this workspace. Imported standalone lockfiles record their source provenance but are not used by the workspace build. The Bevy and Fyrox fixture hosts are not imported as the player interface.

The integration also updates matrix iteration and the WAVE duplicate-data match to satisfy this workspace's pinned Rust 1.99 clippy gate. Matrix operations retain their order and precision; existing inverse and audio regression tests verify the adapted code.

Three source-format compatibility fixes were established against the original client readers and installed resources:

- FAR3 archives may mark a raw entry as compressed. The importer checks the bounded payload for the QFS signature and otherwise validates it as raw data; the archive data-type byte is not a compression discriminator.
- Original QFS headers can count the complete nine-byte-header-plus-command payload. The decoder accepts that convention and the existing command-length convention while retaining strict source bounds, terminal, output-length and trailing-byte validation.
- A Vitaboy mesh's second vertex-count field is metadata. The original `Mesh.Read` reads it without using it as the real-vertex array length. The actual arrays and binding indices remain validated against their operative counts.

Each change has a regression that failed before the correction. Resource, output and memory bounds remain separate from game limits.

`wonderland-avatar-content` bridges the bounded legacy readers to the appearance library. Collection, purchasable-outfit and hand-group readers follow the original `TSOClient/tso.vitaboy.model` formats. Their IDs remain resource IDs; a historic illustration name is not converted into an invented outfit ID.

## Resource path

The original content chain is collection → purchasable outfit → outfit → appearance → binding → mesh and texture. The adult skeleton supplies the rig. Head and body selections are composed independently, with skin-specific appearances and hands where supplied. The importer reports missing dependencies instead of substituting a different character.

The legacy reader's FreeSO coordinate conversion is applied once. The appearance library prepares and skins the mesh; the browser draws its actual vertices, indices, UVs and decoded texture. WebGL2 is used when available. A Canvas2D fallback projects and depth-sorts the same textured triangles when the browser cannot create a WebGL2 context. It retains edge clamping and source alpha; collapsed UV faces use a representative source texel, and painter ordering can approximate intersecting surfaces. Binding bone names remain provenance and must not introduce a second attachment transform.

## Content boundary

This repository contains addon avatar resources. It does not contain the original adult skeleton and complete base head/body collections. Those resources must come from an installed game or an explicitly selected local resource set. Original game assets used for local verification are not committed to this change.

Binary resources stay outside persisted account projections. Historical prototype saves retain their portrait references and data during migration. A saved reference can be shown as historical artwork while original resources are absent, but cannot be presented as a rendered result of the new appearance selections.

Account creation, live wardrobes and lot simulation still require their own authoritative adapters. A working local character renderer does not make those services complete. The full preservation scope remains in [the capability map](player-capability-map.md).

## Verification

The required build gates remain formatting, native workspace tests, native clippy, browser-target clippy and the release WASM build. Importer tests cover source parsing, independent composition inputs, missing-resource outcomes and resource bounds. Browser verification must additionally establish visible textured rendering and interaction; a compiler or a diagnostic mesh alone is insufficient.

Local verification used 22 original static avatar archives plus the adult skeleton: 21,425 resources. The two male creator collections contain 361 choices and the two female collections contain 461 choices. All 2,466 head/body skin-variant dependencies resolved. Actual selected male and female avatars each composed four mesh parts (head, body and both hands), with distinct skin textures. This is dependency and composition evidence, not a claim that every possible combination was visually inspected.

One unrelated wing mesh was rejected for a bone index outside its bone table. The creator collections above remain ready; unrelated resource diagnostics do not block valid selections.

The final browser check visibly rendered original male and female avatars, changed independent head/body/skin selections, paged through source choices and rotated the actual model. The character stage is verified separately from the still-unconnected production world renderer. Details are in [preservation QA](preservation-qa.md).
