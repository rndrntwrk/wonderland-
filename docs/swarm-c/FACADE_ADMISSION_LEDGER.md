# Facade admission continuation — 7 October 2026

Exact published baseline: PR36 `2298b7e4bef62c922e1f94ac47db7c26534da303`,
tree `b32181283818c06c535cf6f0aa36c76668bdb12f`. Local Git baseline is a
source-only archive reconstruction, not the original commit. No source from
previous lost work is counted as recovered.

Scope: preserve generated FSOf bytes and metadata, add transport checksum
verification before browser download, and provide bounded receiving verification
against an independently trusted source document, options and artifact digest.
No shared authority/wire schema, gameplay clock, database, parent branch or
original C# source is changed. A receipt is not authentication or a game save.

Ownership: `world-view/src/facade*`, facade verifier example/tests;
`web-shell/public/facade-links.mjs`, `src/world_facade.rs`, facade-only tests;
the standalone Chromium helper runner, this ledger, verification/handoff
documents and evidence. No new dependencies.

Ruling: Reuse the published codec and source hashing format. Do not infer
pixel correctness or authorized generation from an untrusted receipt. A trusted
artifact digest must be supplied independently; verification of a self-reported
hash is only transport consistency. Pixel/source semantic parity remains in the
existing rendering comparison tests.

## Evidence

- Baseline affected Rust tests and 40 Node tests passed on the restored source.
- `facade-integrity-red.log`: two meaningful tests fail because the existing
  browser helper accepts changed bytes and receipts with no checksum.
- After asynchronous hashing, all eight initial facade transport tests pass.
- Native receiver and read-only CLI implemented; adversarial receipt/source/codec
  tests and actual-file verification pass.
- Actual WASM-target client Clippy passes with the Promise-returning import and
  awaited result; a fresh compiled-application browser run is still required.
- Three temporary guard-removal mutations are detected; source was restored.
- Local Chromium navigation was blocked by administrator policy before test
  initialization; the zero-check failed report is retained, not reclassified.
- Existing actual application artifact downloads verify through the new native
  receiver. Two distinct native regenerated FSOf payloads match archived WASM
  output byte-for-byte. Receipt formatting and the application’s explicit shadow
  revision increment are documented in the receiving guide and evidence.
- Final checks and source packaging are recorded in the accompanying verification
  manifest; remaining browser/independent-review/publication gates stay explicit.

Current connector discovery exposes only GitHub reads; no remote publication is
claimed by this local branch. Original planning scope is not reduced by this leaf.

## Final local checkpoint

Affected native all-target suites: **360 passed, zero failed/ignored**. Browser
helper/protocol Node suites: **48 passed, zero failed/skipped**. Formatting,
strict affected native Clippy and actual web-shell WASM-target Clippy pass.
The inherited proc-macro-error2 future-Rust warning is retained in the lint log.
This is not a whole-workspace or optimized browser release result.

Author self-review checked source/digest trust separation, receipt ambiguity and
budget checks, the asynchronous WASM-view copy, publication cleanup and the real
Rust await/generation boundary. No independent reviewer was available. Browser
acceptance of the new compiled client and remote publication remain pending.
