# Original-runtime reference intake

Status: **inventory only; no original runtime execution claimed by this leaf**.
The F baseline gate does not run C#, Rust, WASM, browser journeys or the tests
referenced in its boundary map. Their presence and committed file identity are
checked; their execution is not inferred.

## Existing source-reference work to reuse

The current client tree contains
`crates/sim-core/tests/avatar_source_reference.rs`. Its ignored test
`avatar_extracted_mono_reference` compiles original-source components and extracted
method bodies against a declared test harness, runs them with Mono, and compares
the resulting records. The companion Rust-vector test is not by itself a fresh
execution of C#. The ignored gate can be run explicitly where the required Mono
compiler/runtime and original source are available:

```sh
cargo test -p sim-core --test avatar_source_reference --locked \
  avatar_extracted_mono_reference -- --ignored --exact
```

This tests extracted/avatar-method behavior, **not the complete original FreeSO
engine**. Its test source identifies the original paths, extracted regions and
harness inputs. F must preserve those distinctions in any collected report.

Swarm A's historical PR #6 at
`8a0e251d19e222a0a6833d7408ca629f674e1729` also describes extracted numeric and
lifecycle probes and a native/WASM replay harness under `tools/swarm-a/`.
`docs/swarm-a/VERIFICATION.md` names those artifacts and their original limitations.
Those tool directories are not present in the inspected #39/#40 client tree;
retrieve and reconcile the pinned branch before integrating them. Do not
silently replace the current toolchain with the older standalone core settings.

Swarm B's published format/content work supplies bounded readers and source
corpus evidence. The inspected tree contains `tools/swarm-b/content-census.rs`
and cooked-content/replay code in `crates/content-runtime-bridge`. Locate the
corresponding branch-local reference fixtures and provenance before declaring a
combined oracle ready. A decoded resource, authored miniature fixture or fixed
expected vector is not an actual original-engine run.

## W00.3 and W00.4 acceptance still required

The future original-runtime harness must bind source commit, exact effective
content/patch identity, seed, commands, external-result schedule, compiler/runtime
and canonical trace. External effects must be controlled and production writes
prohibited. Repeated identical inputs must expose nondeterministic legacy cases
rather than concealing them with relaxed thresholds.

Only scenarios whose original execution is available may be labelled
original/native/WASM equivalence. The differential runner must report the first
divergent tick and replicated field. A deliberately injected branch or RNG fault
must fail and localize; equal final totals alone are insufficient. Presentation
pose history and private EOD state need their own explicit boundaries rather than
being mixed into public simulation hashes.

No full C# engine bootstrap, authorized original-content cohort, new C# trace,
three-way differential result, or W00.3/W00.4 completion is delivered here.
