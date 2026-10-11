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

## Character and Home authoring fixtures

The next UI increment adds **20 individual PNGs** in `authoring/`, generated on 5 October 2026 with the built-in OpenAI ImageGen tool. They extend the approved visual family: ten identity-preserving wardrobe variants, a distinct empty Home scene, six directional furniture views, and three decor sprites. Existing `art/<identity>.png` files remain the Everyday looks. These contributed artwork files use the repository's MPL-2.0 terms.

| Asset group | Files | Use |
| --- | --- | --- |
| Wardrobe | `<identity>-smart.png`, `<identity>-active.png` for Maya, Jules, Nico, Amara and Leo | Full-body creator/wardrobe stage, profile cards, HUD and world character. |
| Home | `home-scene.png` | Distinct unfurnished coastal room; fixed 1672×941 presentation plane. |
| Directional furniture | `armchair-front/back.png`, `coffee-table-front/back.png`, `bookcase-front/back.png` | Independently generated front/rear views; quarter-turn presentation may mirror whole sprites. |
| Decor | `floor-lamp.png`, `fern.png`, `woven-rug.png` | Symmetric furniture/catalog sprites; the rug is rendered on the floor plane. |

[authoring-manifest.json](authoring-manifest.json) records exact bytes, SHA-256, dimensions, alpha bounds, source references, prompts and targeted-edit lineage. Selected PNGs are copied unchanged: no cropping, alpha normalization, recoloring, or geometric image processing was applied. Some generators store colored RGB beneath zero-alpha pixels and faint edge alpha; normal browser compositing, not raw RGB inspection, determines their visible edge. The rug camera and bookcase framing were corrected through targeted image generation, and Leo's active shoes were made plain.

These remain illustrated preview assets with approximate fixed-camera perspective. They are not rigged avatars, a full appearance library, production 3D furniture, or an imported FreeSO catalog. The Home grid and sprite ground anchors are a presentation calibration; authoritative placement and real camera projection come from the future renderer/service adapters described in the [integration handoff](../../../../docs/integration/character-home-ui-handoff.md). The roughly 34 MB of added high-resolution art is retained for review; production delivery/LOD work is separate.

## Nunito font

`fonts/nunito.ttf` is the variable Nunito font (weights 200–1000), obtained from the [Google Fonts distribution](https://github.com/google/fonts/tree/main/ofl/nunito) on 5 October 2026. Copyright 2014 The Nunito Project Authors. The font retains its **SIL Open Font License 1.1**, included verbatim as [fonts/OFL.txt](fonts/OFL.txt). SHA-256: `bb55a5ca5c2042335b3991af27c4d0705d0ef41cac6164ac737fd8f2a1e85207`. It is bundled locally; the runtime does not request a font CDN.

## Tabler icons

The selected SVG files are unmodified assets from [`@tabler/icons` 3.35.0](https://www.npmjs.com/package/@tabler/icons/v/3.35.0), with filled variants preferred and outline variants used where needed. Their **MIT license** is included verbatim in [icons/LICENSE](icons/LICENSE). [icons/sources.json](icons/sources.json) records each original package path and the package archive hash. UI tinting is applied through CSS to the original icon files. No custom icon geometry is substituted.
