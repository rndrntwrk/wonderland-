# Wonderland UI assets

## Generated illustration fixtures

The ten PNG assets below were generated for this project on 5 October 2026 with OpenAI ImageGen, using the user's approved action-focused game designs as the visual references. Each is an individual scene, character, object, or marker; none contains baked application controls. Transparent character/object assets are composed with semantic controls by the browser shell. The three approved reference images are preserved in [the design folder](../../../../docs/design/action-first/README.md).

These illustrations support UI integration. They are not exported TSO assets, runtime 3D models, an imported content pack, or evidence of completed simulation/renderer behavior. The café's seated background people are decorative fixture imagery. Source PNGs are retained unchanged, including their alpha channels; the browser controls their displayed size and crop. These contributed artwork files use the repository's MPL-2.0 terms. They can be replaced by renderer-produced imagery/content manifests without changing UI intent types.

| File | Pixels | Use |
|---|---|---|
| `art/character-garden.png` | 1672 × 941 | Scenery and physical display stage for character selection |
| `art/city-map.png` | 1672 × 941 | Illustrated city terrain and neighborhood fixture |
| `art/cafe-scene.png` | 1672 × 941 | Illustrated café interior; interactive player and coffee machine removed |
| `art/maya.png` | 1024 × 1536 | Selected character, grid portrait and HUD portrait |
| `art/jules.png` | 1024 × 1536 | Selectable character and grid/HUD portrait |
| `art/nico.png` | 1024 × 1536 | Selectable character with guitar and grid/HUD portrait |
| `art/amara.png` | 1024 × 1536 | Selectable character and grid/HUD portrait |
| `art/leo.png` | 1024 × 1536 | Selectable character and grid/HUD portrait |
| `art/coffee-machine.png` | 1254 × 1254 | Independent interactive fixture object sprite |
| `art/selection-diamond.png` | 1024 × 1536 | Character selection marker |

Exact file sizes and SHA-256 hashes are recorded in [art-manifest.json](art-manifest.json). These high-resolution fixture sources are not a production startup-size budget; production content cooking/LOD and delivery measurements remain separate work.

## Nunito font

`fonts/nunito.ttf` is the variable Nunito font (weights 200–1000), obtained from the [Google Fonts distribution](https://github.com/google/fonts/tree/main/ofl/nunito) on 5 October 2026. Copyright 2014 The Nunito Project Authors. The font retains its **SIL Open Font License 1.1**, included verbatim as [fonts/OFL.txt](fonts/OFL.txt). SHA-256: `bb55a5ca5c2042335b3991af27c4d0705d0ef41cac6164ac737fd8f2a1e85207`. It is bundled locally; the runtime does not request a font CDN.

## Tabler icons

The selected SVG files are unmodified assets from [`@tabler/icons` 3.35.0](https://www.npmjs.com/package/@tabler/icons/v/3.35.0), with filled variants preferred and outline variants used where needed. Their **MIT license** is included verbatim in [icons/LICENSE](icons/LICENSE). [icons/sources.json](icons/sources.json) records each original package path and the package archive hash. UI tinting is applied through CSS to the original icon files. No custom icon geometry is substituted.
