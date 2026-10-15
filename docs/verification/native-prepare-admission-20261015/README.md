# Source callback prerequisite for controlled admission

## Scope

The independent controlled-mode review is retained in REVIEW.md. Inspection then found that successful PrepareAdmissionV2 would require missing original lot-permission/reconciliation semantics, not merely generic spawn plus portable state. Successful admission, deterministic admission registry, grants and public routes therefore remain unimplemented and fail closed. DEPENDENCIES.md records the next source implementation; the complete original-game scope is unchanged.

This increment isolates an explicit source-named-callback accepted transition without changing legacy callback tag/semantics, RunInMyStack behavior or snapshot layouts. It qualifies a bounded exception case rather than silently changing every VM fault policy. Exact supported fault categories and test results are recorded below.

## Original-source evidence

Fetched the pinned original FreeSO commit 4c6b3e8f5835b228723caea3c9f683c62f244f73 through the existing Git route. source-provenance.json records byte-for-byte comparisons of the checked-in original VMThread, VMEntity, VMNetSimJoinCmd, VM, VMMemory, VMExpression and lot-permission dependencies against that commit.

Original VMThread.EvaluateCheck clones temporary banks when the scheduler is not running, executes a temporary check thread, and returns false after a suppressed exception sets DialogCooldown. Under !THROW_SIMANTICS, Tick preserves preceding world mutations, signals an exception dialog and clears the check stack rather than resetting the owned entity/thread. VMNetSimJoinCmd ignores the callback boolean and continues the out-of-world controller list. VM.GetGlobalValue explicitly throws for an out-of-range global index.

This is static source-backed evidence. The existing cloud computer has no dotnet/mono/csc/mcs tool or built FSO.SimAntics assembly. The repository's original-assembly runner expects separately built .NET Framework assemblies and a matching MonoGame runtime; they are not available here. No original-assembly differential execution or rendered source dialog equivalence is claimed. No alternate computer or environment was used. Rust authority/replica replay is useful but is not a substitute for that differential gate.

## Publication boundary

All work is local on the existing cloud computer. Prerequisite tree was transplanted onto existing PR75 commit 1c4d5c9db59587c364107c64856a318ae4a8e413, preserving remote ancestry. No remote publication, merge, deployment, live provisioning, credentials, public admission or gameplay grant occurs in this task.

## Implemented bounded behavior

`AcceptedCommand::ExecuteSourceExpressionCallback` is appended at tag 26. Legacy `ExecuteNamedEntryPoint` remains tag 25 with its existing fail-closed exception behavior. The new command preflights the selected named routine as expression-only before execution; explicit subroutine and non-expression opcodes in that selected routine are refused. This preflight is not a proof that no indirect scripts can run: expression writes can trigger memory signals such as reset, whose separate hosts retain fail-closed behavior. Only `VmFault::Bounds` for the global register array is suppressed. Its earlier writes remain, the callback returns false and emits a headless `SourceNamedEntryPoint` diagnostic, and later accepted controller callbacks can continue. The temporary callback stack is discarded while its callback-local cloned temp banks do not overwrite the owned thread banks. Explicit source memory scopes that intentionally address real entity temps retain their ordinary write semantics; this is not a blanket isolation guarantee for all memory writes. Other fault families, budgets, stale entity generations and infrastructure revision exhaustion still reject the entire candidate tick. This qualification does not cover all ordinary original exceptions or nested check behavior.

## Focused verification

- Behavioral RED: the regression failed with `Vm(Bounds { area: "global", index: 32767, len: 38 })` before exception retention was implemented.
- Ten focused integration tests passed, exercising both TS1 and TSO: retained mutations and later callback execution; authority/replica event/hash equality; snapshot restore and duplicate replay without effects; legacy fault rollback; unsupported/nested content refusal before work; unqualified bounds/budget/stale identity rejection; exact tags 25/26; ReturnFalse/missing/empty results; input bounds; infrastructure revision exhaustion rollback; global destination bounds and increment-before-RHS fault ordering; explicit scope13 entity-bank writes; indirect pet-reset fault refusal without partial writes; and whole-tick rollback when a later command fails.
- Existing pinned rustfmt 1.10.0-stable (Rust 1.99 commit b940084d7e, 2026-09-28) formatted the changed Rust files; whitespace checks passed.

Final source verification is recorded in verification.json: 2,036 workspace tests passed, five explicit source-oracle/resource-dependent tests remained ignored, the legacy named callback example passed, the actual native-host targets rebuilt, and the actual native-edge durability/SIGKILL suite passed. Full-workspace formatting and whitespace checks passed. The initial parallel build was killed by resource pressure; both the serial retry and final serial run passed. Strict Clippy was not available in the local reconstructed toolchain. Original-assembly differential execution remains a required qualification gap, not a passed test.
