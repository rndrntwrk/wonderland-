# Original VM bootstrap: Swarm F execution ledger

Base: PR #43, `15775b0608cab56cff15654a01850966f7d3f9d9`.
Original C# source: `4c6b3e8f5835b228723caea3c9f683c62f244f73`.

The approved next checkpoint moves beyond #43's clock/RNG component harness:
compile the complete original SimAntics assembly and its real dependency closure,
then instantiate the original VM and execute controlled BHAV behavior with all
fixture inputs and host dependencies recorded. Never replace VMThread.Tick or a
primitive with a fixture implementation and call that full-engine execution.

## Execution order

1. Inspect/build the original project closure in an isolated temporary copy.
   Preserve original source bytes and declared package versions. Retain errors
   and unresolved external/content dependencies rather than silently bypass them.
2. Add a test-only headless entry point with deterministic initialization, no
   account/database endpoints and no production global-link effects. Original
   content versus authored fixture metadata must remain distinguishable.
3. Exercise interpreter/branch/scheduling behavior. Repeat each controlled run;
   bind the exact assembly/source/content identities and complete traces.
4. Add a scoped differential comparison against the real Rust runtime only when
   canonical observable state and command semantics actually align.
5. Run the new gate on the published tree, preserve failures and publish exact
   evidence and remaining qualification. No source/library/client merge or deploy.

## Constraints and decisions

- The original assembly is .NET Framework 4.5 and includes renderer/content/audio
  references even for headless execution. Those dependencies must build or be
  supplied as their actual declared binaries; a fake renderer type must not
  accidentally shadow an original VM or content class.
- Build exploration may run on a Windows hosted runner with the original classic
  project files, reference assemblies and package restore. That is a distinct
  host from #43's Mono execution and must not be conflated with it.
- Source and build artifacts remain separate. The verified source checkout stays
  read-only; package restore, intermediate outputs and fixture files live under
  a new runner-temporary directory. Retain text reports/logs and hashes, not game
  assets, package caches or credential-bearing machine state.
- No full-game content installation is currently supplied by this checkpoint.
  A synthetic content cohort can qualify its declared scenarios, not the original
  human-art or all-object corpus. Missing source dependencies are blockers to the
  corresponding execution claim, not permission to substitute simplified logic.
- The parent baseline checker and the existing original-component reference remain
  separate. Local Codex keeps application/avatar/terrain/audio integration.

Current status: source startup/dependency requirements inspected; isolated full
assembly build and runtime acceptance not yet executed.
