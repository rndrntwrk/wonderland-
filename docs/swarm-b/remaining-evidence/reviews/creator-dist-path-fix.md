# Creator configurable output directory correction

## Result and scope

The scoped path correction is complete. Relative build target, distribution and
browser evidence directories resolve against the Creator package root, independent
of the process working directory. Absolute overrides retain their locations.
The browser hashes the configured artifact, passes its normalized distribution
directory to the child server, and asserts that the bytes served over HTTP have
the same SHA-256.

Only these implementation files changed in this follow-up:

- `tools/creator-web/scripts/build.mjs`
- `tools/creator-web/scripts/serve.mjs`
- `tools/creator-web/tests/browser.mjs`

No Rust build, Cargo change, other source/test edit or Git write was performed.
Existing workflow counts and permission/state assertions remain intact. The only
new browser report field is `served_wasm_sha256`, a fixed 64-character digest.

## Reproduced defect

The old build script resolved overridden output and bindgen-input paths in the
caller directory while Cargo ran in the package directory. The standalone server
also resolved relative overrides in its caller directory. The browser spawned its
server in the package directory but always hashed default `dist/pkg`, disregarding
the configured distribution directory.

A copy of the existing completed release was placed outside the repository at:

`/workspace/scratch/378e4c36af7b/creator-dist-path-review/configured-dist`

The configured relative value was:

`../../../creator-dist-path-review/configured-dist`

Both browser runs were launched from repository cwd, not the package cwd:

`/workspace/scratch/378e4c36af7b/wonderland`

For each run, the original ignored `tools/creator-web/dist` directory was moved to
an unused temporary backup. The harness asserted that default `dist` was absent
before starting the browser runner and restored it in a `finally` block. Its
restored WASM digest was checked against the original copy. No fallback default
artifact was available to satisfy either run.

## RED evidence

The old browser runner exited **1** before any workflow ran, with
exactly the reviewed missing-default-artifact error:

```text
Error: ENOENT: no such file or directory, open '/workspace/scratch/378e4c36af7b/wonderland/tools/creator-web/dist/pkg/wonderland_creator_web_bg.wasm'
```

The configured copy existed throughout. Default `dist` was restored after the
failure. Evidence is retained in:

- `creator-dist-red/report.json`
- `creator-dist-red/runner.log`
- `creator-dist-red/path-check.json`

## Implementation

`build.mjs` now resolves both `CREATOR_WEB_TARGET_DIR` and
`CREATOR_WEB_DIST_DIR` against its module-derived package root before passing any
paths to Cargo, wasm-bindgen or filesystem operations. Defaults remain `target`
and `dist` within that package.

`serve.mjs` uses the same module-derived package root for
`CREATOR_WEB_DIST_DIR`, including a direct launch from repository cwd.

`browser.mjs` resolves its distribution and evidence directories against the
package root, passes the absolute distribution path to the server, reads WASM
from that configured directory and compares its digest with the server's HTTP
response. Default evidence remains `test-results/browser` within the package.
An HTTP failure or artifact mismatch rejects the run before workflow verification.

## GREEN evidence

The corrected runner exited **0** and passed **all 25 existing
browser workflows**, including the startup failure path, using the configured
outside-repository bundle with default `dist` absent. Browser:
**141.0.7390.37**. Completed: **2026-10-05T21:10:56.745Z**.

Configured local artifact, browser report and actual HTTP-served WASM all had the
same SHA-256:

```text
c2cf91400bf653188682cc8aa9df8347241e5d5252f4f41d07a9ba0cfc8e8a59
```

The run recorded zero unexpected page errors and zero external requests. The
original default distribution was restored in `finally` and its WASM hash was
unchanged. Evidence, exported test files and screenshots are retained under:

- `creator-dist-green/report.json`
- `creator-dist-green/runner.log`
- `creator-dist-green/path-check.json`

A separate direct `serve.mjs` launch from repository cwd with the same relative
configuration returned HTTP 200 and the identical digest. This verifies the
standalone server entry point without relying on the browser's normalized child
environment; see `creator-dist-green/standalone-serve.json`. That temporary server
was terminated in `finally`.

All three modified scripts also passed `node --check`. No fresh compiler build
was needed or performed; browser validation used the already completed release.

## Reproduction command

From the repository root, using the preserved copied release:

```sh
CREATOR_WEB_DIST_DIR='../../../creator-dist-path-review/configured-dist' \
CREATOR_WEB_EVIDENCE_DIR='/workspace/scratch/378e4c36af7b/wonderland/.superpowers/sdd/REMAINING/creator-dist-green' \
CREATOR_PLAYWRIGHT_MODULE='/workspace/scratch/378e4c36af7b/creator-browser-tools/node_modules/playwright' \
node tools/creator-web/tests/browser.mjs
```

Relative directory values intentionally refer to the Creator package root;
the caller may run these scripts from another working directory. The evidence
paths used for the acceptance runs were absolute, preserving the coordinator's
aggregate invocation contract.
