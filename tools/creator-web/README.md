# Wonderland Creator in the browser

A Rust/Leptos editor backed by the actual `wonderland-creator` and
`wonderland-legacy-formats` libraries. The browser opens user-selected files,
inspects their stored values, applies source-guarded edits and downloads new
files. Imported game assets are not bundled with the application.

## Build and run

Use Rust 1.90.0, Node 22 or later, and the `wasm32-unknown-unknown` target. The
`wasm-bindgen` CLI must match version **0.2.129** in this application's lockfile.

```sh
rustup target add wasm32-unknown-unknown --toolchain 1.90.0
cargo +1.90.0 install wasm-bindgen-cli --version 0.2.129 --locked
cd tools/creator-web
npm ci
RUSTUP_TOOLCHAIN=1.90.0 npm run build
npm run dev
```

The development server prints its local address, normally
`http://127.0.0.1:8877`. Build output is in `dist/`. It can be served by a static
HTTP server with the correct JavaScript, CSS and WebAssembly content types.
There is no application backend or external font dependency.

The build script accepts `CREATOR_CARGO`, `CREATOR_WASM_BINDGEN`,
`CREATOR_WEB_TARGET_DIR` and `CREATOR_WEB_DIST_DIR` for an existing toolchain or
isolated build. `CREATOR_WEB_PORT` changes the development port. `--debug` selects
a development Rust build; the default is the optimized release profile.

## IFF workflow

1. Choose **Open IFF** and select an IFF resource file. The resource rail displays
   its real types, identifiers, labels and byte lengths. Filter by type, label
   or identifier and use the paged list to choose a resource.
2. **Inspect** shows actual instructions and control flow, strings, tuning,
   palette colors or routing slots. The raw byte preview remains available for
   unrecognized or invalid resources. The details column displays source and
   resource hashes and a decoded format version where available.
3. **Edit** supports BHAV branch destinations and operands, strings, constants,
   slot offsets, palette colors and complete raw replacements of unrecognized
   types. Expand **Resource metadata** to change an identifier, flags or exact
   label bytes, or remove the selected resource. **Add a resource** also works
   when an imported IFF is empty.
4. For SPR2 resources, export the source-bound sprite package, edit its indexed,
   alpha and depth planes or frame metadata, then import it from **Edit**.
   Source and dependency hashes must still match. The inspector validates the
   actual sprite and palette dependencies before offering this workflow.
5. **History** restores prior or subsequent complete document revisions.
   **Export IFF** downloads the current bytes. An unchanged document exports
   byte for byte, including an opaque envelope. A malformed imported resource
   can be preserved; export alone does not certify all its semantics.

BHAV destinations 253, 254 and 255 retain the source alternate/error,
return-true and return-false meanings. The graph shows the first 32 instructions;
the paged table exposes all admitted instructions. Opcodes are displayed as
their actual numeric values.

## Session limits and behavior

The IFF session admits files up to 8 MiB, resources up to 4 MiB and 4,096 entries.
Its operation budget is 128 MiB and includes reservations for the retained
document, history and working snapshots. History retains at most 64 changes
within 16 MiB. When necessary, the farthest revisions are evicted while keeping
the latest accepted change reversible. An edit whose resulting document cannot
be retained for undo is rejected before the candidate is published.

Typed inspection reserves a separate 16 MiB of working memory and admits at most
1 MiB of estimated JSON output before constructing the display graph. An
over-budget typed resource falls back to a 2 KiB raw preview and exact export.
Browser sprite editing packages are limited to 1 MiB; raw replacement fields to
64 KiB. The corresponding native Creator tools provide separate, configurable
limits for larger authoring work.

A stale guard, invalid edit or failed import leaves the current document and
history intact. File reads have generation tickets. A successful newer import,
edit or restore invalidates an older pending read. Controls are disabled while
the browser reads a new file. Navigating away from a changed session uses the
browser's normal unsaved-work warning; exporting does not overwrite the
original file or persist the session.

## Verification

```sh
cargo +1.90.0 test --locked --manifest-path Cargo.toml
cargo +1.90.0 clippy --locked --manifest-path Cargo.toml --all-targets -- -D warnings
npx playwright install chromium
npm run build
npm run test:browser
```

The browser runner starts and stops its own local server. It uses the actual
file chooser and download API, then checks the exported binary values. Reports
and desktop/mobile screenshots are written under `test-results/browser/`.
`CREATOR_WEB_EVIDENCE_DIR` changes that directory;
`CREATOR_PLAYWRIGHT_MODULE` selects an already installed Playwright module.

The native session tests cover atomic guards and publication, asynchronous-read
races, bounded history, sprite admission, raw preservation and inspection
limits. The browser suite covers real IFF edits and reimports, tab keyboard
navigation, literal Unicode/markup, malformed files, metadata/add/remove,
empty-file undo/redo, sprite edits, a 390-pixel viewport and startup failures.

This application is an authoring surface. The actual isolated runtime watch,
whole-tick and live-inspection APIs and their current UI integration boundary
are documented in
[the runtime extension contract](../../docs/swarm-b/runtime-extension-contract.md).
Instruction break/step/resume requires the specified runtime capability.

## Design and licenses

The layout uses a dark resource rail, a light work surface, indigo actions and
visible keyboard focus. Tables scroll within their own containers; narrower
layouts stack navigation, editor and details. All interactive content uses DOM
controls or SVG, with reduced-motion support and semantic status/error output.

The four bundled Inter font files are unmodified Fontsource Inter 5.2.8 Latin
faces, licensed under SIL OFL 1.1. Their copyright and complete license are in
[`public/fonts/OFL.txt`](public/fonts/OFL.txt). The application code is MPL 2.0.

## Additional authoring workbenches

The tool navigation keeps six independent sessions mounted, including the IFF
editor. Switching tools preserves their imported sources, histories and drafts.
Each additional workbench admits a source file up to 4 MiB, at most 32 history
entries within 8 MiB, and a 32 MiB operation budget that reserves its retained
sources, attachments, history and bounded graphical preview. Source guards,
failed-candidate atomicity and upload generations apply to all of them.

| Workbench | Working controls and actual output |
| --- | --- |
| Upgrades | Inspect files, groups and upgrade levels; edit tuning substitutions and level name/price; apply exact path transactions to configuration or other fields; preserve unknown fields and export the original JSON format. |
| City painter | Decode PNG/BMP maps; inspect pixels; paint source categorical colors, density, elevation or vertex color; draw/erase paired road edges and corners; export PNG or opaque BMP. Paint/road authoring requires an opaque 512 × 512 map. |
| Neighborhoods | Inspect source-order identities and locations; edit GUID, name, optional description and coordinates; find the nearest neighborhood with original source tie behavior; export JSON without dropping unknown fields. |
| Assets | Choose an explicit Vitaboy/FSOm/NBHm source format; inspect exact metadata; edit stored vectors or references; project admitted stored mesh triangles; exchange FSOm OBJ/MTL or source-bound GLB/glTF. Animation GLB/glTF requires its actual skeleton attachment and retains event metadata. |
| Patches | Open the original IFF; attach ordered official/user PIFFs; change the exact case-sensitive source name; inspect applied/suppressed provenance; download the effective IFF while retaining the original source export. |

Advanced field edits accept an array of `path`, `remove` and `value` operations.
Paths contain explicit string field names and canonical array indices. Source
integers remain exact in Rust, including references beyond JavaScript's safe
integer range. Binary float metadata exposes unsigned IEEE-754 bits; the typed
transform controls accept decimal values. Interchange imports require matching
source identity and reject incompatible topology or protected metadata.

Patch attachments are limited to 32 files and 4 MiB in total. Skeleton and patch
attachments remain local. They are included in the unsaved-work warning and are
never sent to another user, a runtime or a provider. Closing or reloading the
page ends these local editing sessions; export the source files you need.

The mesh projection shows stored geometry before skinning and material rendering.
The workbenches do not claim live animation playback or a connected running-lot
debugger. Provider and runtime integration requirements are recorded separately
from working source-file authoring.
