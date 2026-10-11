# Facade receiving integrity and browser publication

## Delivery boundary

This continuation applies to PR36 at
`2298b7e4bef62c922e1f94ac47db7c26534da303` (tree
`b32181283818c06c535cf6f0aa36c76668bdb12f`). It does not replace the published
facade generator, source material renderer, lighting, PNG export, other screens,
shared native protocol or simulation. It does not finish all Swarm C work.

The receiving boundary checks a visual derivative against **three independently
trusted inputs**: the admitted normalized `WorldDocument`, export options, and
artifact SHA-256 from the trusted job result/manifest. An FSOf file and its own
JSON receipt cannot authenticate one another. Computing a digest from an
untrusted receipt and passing it as the expected digest defeats this trust model.

No account/lot claim, operation result, snapshot or ownership permission is
created. In particular, `source_hash` is a disposable content/provenance key,
not a native authority epoch or authenticated actor. The verifier cannot establish
that an untrusted renderer drew the correct pixels; use the separate rendering
comparisons and a trusted worker for that qualification.

## Implemented API

`wonderland_world_view::verify_world_facade(expected_world, options,
trusted_artifact_sha256, bytes, metadata_json)` returns a decoded `Fsof` only
after all checks complete, or `WorldError` without publication or state changes.

It checks the version-1 gzip header; 16 MiB raw and decoded budgets; 2048 texture
dimension, two-million texture-pixel and existing geometry budgets; supported
RGBA8/single-supplied-light-state content; the independently expected digest;
strict required receipt fields, duplicate/unknown keys and canonical decimal
revision fields; exact source provenance, options and source fingerprint; and
bounded diagnostic and work/region counts. The same byte-bounded, streaming
source fingerprint routine is now shared with generation. Its prefix,
serialization order, producer FSOf bytes and receipt semantics are unchanged.

The receipt limit is 64 KiB before deserialization. The original bounded codec
is reused unchanged, including gzip checksum, trailing-data, dimensions and
geometry checks. Receipt count checks do not substitute for actual codec limits.
Callers must limit network/file input *before* buffering it, and implement their
own authentication, request authorization, queue bounds and I/O deadlines.

## Read-only native receiver

With the pinned repository compiler and lockfile:

```sh
cargo run -p wonderland-world-view --example facade_verify --locked -- \
  TRUSTED_WORLD.json FACADE.fsof RECEIPT.json "$TRUSTED_FACADE_SHA256" 4
```

The last argument is independently requested pixels per tile (1–8), not an
option chosen by the received metadata. The digest must be 64 lowercase
hexadecimal characters. The command reads bounded regular-file inputs (world
32 MiB, facade 16 MiB, receipt 64 KiB), validates them and prints a summary. It
never creates or overwrites an output. Callers select private files and provide
I/O deadlines; this is not a sandbox for adversarial path or filesystem owners.

## Browser download guard

`makeFacadeLinks` is now asynchronous. It checks the producer's SHA-256 before
creating either Blob URL. This browser-side guard checks **transport consistency**,
not the trust of the producer or the receipt.

The helper snapshots a borrowed WASM byte view synchronously before its first
await and uses that same copy for hashing and Blob creation. Shared buffers are
rejected. At most two outstanding hashes retain copies in one module instance;
completion or failure releases the slot. A failed digest exposes no URL, and a
failure creating the second URL revokes the first.

The real Rust `WorldFacadePanel` imports the Promise-returning helper and awaits
it. Its existing source/generation guard remains after that await: cancellation,
replacement or disposal releases late URLs instead of publishing a stale result.
Missing WebCrypto fails closed with a status message. No artificial browser clock,
changed image/selection threshold or substituted old WASM binary is used.

## Tests and reproducibility

```sh
cargo test -p wonderland-world-view --test facade_admission --locked
cargo test -p wonderland-world-view --example facade_verify --locked
node --test apps/web-shell/tests/gpu/facade.test.mjs \
  apps/web-shell/tests/gpu/facade-integrity.test.mjs
cargo test -p wonderland-render-core -p wonderland-render-iso \
  -p wonderland-render-3d -p wonderland-world-view --all-targets --locked
cargo clippy -p wonderland-render-core -p wonderland-world-view \
  --all-targets --locked -- -D warnings
cargo clippy -p wonderland-web-shell --target wasm32-unknown-unknown \
  --lib --bin wonderland-web-shell --locked -- -D warnings
cargo fmt --all -- --check
```

The standalone helper browser test uses Node 22 and a locally installed Chromium,
without added package dependencies:

```sh
WONDERLAND_CHROMIUM=chromium \
  node apps/web-shell/tests/gpu/facade-integrity-browser.mjs
```

This is only helper/WebCrypto conformance. The existing optimized-application
`application.mjs` gate must also be rerun on a newly built WASM application before
browser acceptance of this continuation. Copying the new asynchronous helper into
an old synchronous-binding release is **not** a valid integration or verification.

The local Chromium attempt returned `net::ERR_BLOCKED_BY_ADMINISTRATOR` during
localhost navigation, before importing the production helper. Its retained report
has failed status and zero executed checks. It is not a passing browser result.
No policy bypass was attempted. The earlier PR36 Browser UI run **37590949151**
passed for its older source; that does not cover this new asynchronous binding.

## Cross-runtime artifact compatibility

The new native verifier was exercised against three actual facade downloads from
that earlier built-application run, artifact **11468558600**. Native generation
of its two distinct lit/shadowed cases produced byte-identical FSOf data; complete
parsed receipt fields also match. The archive retains pretty-printed receipts,
whereas the native producer emits compact JSON, so raw receipt byte equality is
not claimed.

The application test explicitly advances `shadowSource.lighting.revision` to
`"2"` before import. The archived standalone shadow fixture is revision `"1"`.
The receiving test first correctly rejected that stale source, then reconstructed
exactly the actual application request from the published test instruction.
The reconstructed input, original fixture and failure log are all retained.
Neither the receipt nor verifier was altered to make the stale input pass.

This is new native execution against previously produced WASM/browser artifacts,
not fresh WASM execution or complete original-content parity. All fixtures here
are the existing explicitly synthetic normalized test worlds.

## Review and publication

Regression-first evidence covers the original missing browser checksum check,
the missing native receiving path and the missing native file verifier. Temporary
mutation tests remove the trusted digest, source fingerprint and browser checksum
guards separately; each corresponding regression fails. Production source is
restored before final tests. Self-review is not independent approval.

No dependency, lockfile, original C# source, SQL state or shared authority contract
was changed. The current GitHub tool set exposes reads only; this continuation is
packaged as a cumulative patch plus every changed source file and evidence, not
claimed as a remotely published commit. Apply it to the exact base, rebuild, run
browser acceptance and obtain independent review before merging.
