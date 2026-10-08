# Content preflight: publication and integration record

This publishes the eight-file content-preflight handoff as ordinary repository
source above PR #45 at `e32387449850b8ea2ffb93b77a48110b7b65ab65`.
The eight source/workflow blob identities were verified against the delivered
patch before publication; no temporary source-transfer/assembly workflow is
required. This additional document records the change in delivery status.

## Historical versus current evidence

`CONTENT_PREFLIGHT.md` retains the prior implementation session's local results
and tool-access limitations verbatim. Its statements that the work was unpublished
and that no hosted run was performed describe **that earlier handoff**, not the
new publication. Do not apply that patch again on top of this branch.

The original scope checker, tests, fixed report and runner are unchanged from the
handoff. A fresh local integrity check verified all 63 packaged files, and the ten
Python evidence tests were executed successfully during publication. Previous
Rust/native/WASM local results remain attributed to the earlier session; they
are not a new local engine rerun.

Hosted execution is supplied by `Swarm F content prerequisite evidence`, using
the default **clean-tracked-checkout** mode, the full base ancestry and the
repository-pinned compiler. Read the named PR-head workflow and retained
`execution.json` before claiming it passed. This static document deliberately
does not turn a pending run into a successful result.

## Exact transferred source

| Path | Git blob |
| --- | --- |
| `tools/replay/content_scope.rs` | `56b8d511f0e16545318128819c50f66f0e3fe0d8` |
| `tools/replay/content_preflight_probe.rs` | `c110bad968cfc2d0bac3f4905f1ee41487ce68b9` |
| `tools/replay/run_content_preflight.py` | `b5d7046ff49a6ed1d7857e2e7e8841a1c0aeec20` |
| `tests/parity/content_scope_cases.rs` | `b36ffb55a6b59319d0a105848e386d3402e54efb` |
| `tests/parity/test_content_preflight_evidence.py` | `50014118dbf9db919e540e7643fb646d76ac2419` |
| `fixtures/reference/content-preflight.expected.json` | `3cad26742a3c584a63cd6c322f108994cb05069c` |
| `docs/swarm-f/CONTENT_PREFLIGHT.md` | `a9376e63f036651221e3647213d08c5a866332d4` |
| `.github/workflows/swarm-f-content-preflight.yml` | `585f0e15a8f4bfbab8a47dafe3a9c35e61c7cdde` |

The expected report remains 30,820 bytes, SHA-256
`d05ecf34f43a397706ed9cbe8594f8ad5141e90893aad3bfed28d8aba5cfa3bf`.
A new tracked-checkout run must reproduce it without refreshing the expectation.

## Acceptance does not change at publication

All four complete original resource inputs are deliberately **blocked for static
dependency admission** when installed global/semiglobal resources are absent.
The bar's three multipart definitions, the other three original definitions,
77 present behavior programs and 113 direct call sites remain preserved.
A passing verification run means that exact missing-dependency graph was
reproduced; it does not mean these objects are now usable or placed in a world.

Clarification to the earlier implementation wording: each file's hash is checked
before that file is decoded/inspected, not in a separate up-front four-file pass.
Any input hash failure prevents a successful report from being emitted. The
checker never writes the resources or manufactures missing providers.

The next original-content gate needs an authorized effective resource cohort:
`global.iff`, `wetbarsemiglobal.iff`, `skillobjects.iff`, and the chosen mode's
actual patch/tuning inputs. These are not supplied by publication, a checked-in
patch renamed as an IFF, or a guessed semiglobal filename. Use B's existing
content pipeline and reviewed geometry/slot metadata before qualifying original,
native and WASM placement and object lifecycles.

Client/avatar/terrain/audio work remains with local Codex. This branch changes
no existing application or runtime source, original resources, workspace files,
lockfiles, dependency versions, inherited workflows or deployment settings.
Publication is not independent approval, merge, deployment, original VM execution,
rendered-browser/device qualification or completion of W00/W17.
