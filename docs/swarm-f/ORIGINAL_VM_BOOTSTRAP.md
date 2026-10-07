# Original VM bootstrap: Swarm F execution and handoff

Base: PR #43, `15775b0608cab56cff15654a01850966f7d3f9d9`.
Original source baseline: `4c6b3e8f5835b228723caea3c9f683c62f244f73`.

This checkpoint builds the **complete original FSO.SimAntics assembly** and its
actual ten-project dependency closure. The separate test program instantiates
original VM, VMContext, VMGameObject, VMThread, translator and scheduler classes,
and runs original BHAV bytes. No VM thread, primitive or execution method is
copied into a miniature replacement implementation.

**The content provider is an explicitly authored test fixture.** This is not a
complete installed FreeSO lot, original account server, original asset-loader
qualification, gameplay parity or a native/WASM full-state comparison. Local
Codex retains the application/avatar/terrain/audio continuation.

## Build and isolation contract

The Windows reference workflow takes a temporary source copy from the exact PR
checkout, compares it to Git's raw blobs, and restores any platform-filtered
archive bytes from those same blobs. It records commit, tree, file SHA-256 and
Git blob identity. Original files in the repository are never patched or
retargeted. A final check detects changes to any recorded input in the temporary
copy and the tracked checkout.

The project closure is evaluated as the existing classic project graph for
Release/AnyCPU: Mp3Sharp, FSO.Common, TargaImagePCL, FSO.Files, FSO.Vitaboy,
FSO.Content, FSO.HIT, FSO.Vitaboy.Engine, FSO.LotView and FSO.SimAntics. Their
original package versions are restored under the temporary directory. The
reference-only Microsoft.NETFramework.ReferenceAssemblies.net45 1.0.3 package
supplies missing .NET Framework 4.5 targeting files. The original Profile7
Microsoft references must already exist on the reference host and are copied
into that isolated search root. There is **no network archive fallback** or
replacement of the portable project with a different target.

The actual declared MonoGame.Framework.Portable 3.6.0.1625 runtime DLL is copied
alongside the witness because original projects mark it Private=false. No fake
renderer/runtime type is provided. Graphics are disabled through the original
VM.UseWorld setting, not by replacing a VM class.

Restore and compilation can access package feeds. The controlled simulation has
no authenticated user, account endpoint, database/global-link or EOD provider;
its fixture driver rejects outbound/direct commands and account-IP queries.
This is not an OS network sandbox or a security certification of the old engine.
All build products remain in the disposable reference directory. Only text
reports, logs, source/binary hashes and numeric traces are uploaded. No game
assets, font files, reference assemblies or package payloads are redistributed.

Original compiler warnings are retained. The old Newtonsoft.Json 12.0.2 restore
reports a known high-severity advisory; this is a test-only original-dependency
cohort, not permission to deploy that package set. Assemblies are hashed per
build; reproducible source and repeated execution must not be confused with
byte-reproducible DLL/MVID generation by the old project toolchain.

## Controlled original-VM scenario

The fixture supplies an empty original-format global resource to the actual
WorldGlobalProvider cache. It allocates only the Content provider container
without its installation constructor, explicitly injects its singleton/cache,
and sets TS1 mode. **VM, VMContext and all simulation entity/thread constructors
still run unchanged.** Content.Init, installed global tables, tuning and patch
resolution are not exercised. Reflection is confined to the content-provider
seam; no private VM state or execution code is replaced.

The original VM.Init produces its real global state and validator. The fixture
then supplies an 8x8 architecture, exact UTC start `630822816000000000`, RNG seed
`0x123456789abcdef0`, and one out-of-world non-avatar entity. Out-of-world status
is deliberate: placement, routing and graphics are not claimed by this test.

Resource input:

- `TSOClient/FSO.Content.TSO/Content/Objects/Casino_2-Tile_Bar_CC.iff`
- Whole-resource SHA-256:
  `20b67e06940bfe0a89b689312fe001721ff3760cce10fca104373bf1122c4865`
- Original BHAV **4110**, decoded by the real FSO.Files reader.

A declared OBJD binds that unchanged three-instruction routine as Init/Main.
It uses GUID `0xf00d0044`, four attributes and an eight-frame stack. Its initial
attributes are `[10,20,30,40]`, matching the established A/B test setup. **The
routine clears attributes 0, 1 and 3 and preserves attribute 2. It does not
assign the literal 30.** Every tick independently sets those three mutable
attributes to `[-tick,100+tick,200+tick]`, then the actual VMNetDriver.InternalTick
and original VM scheduler execute the real main thread. All four attributes,
frame return, positive identity, live entity count, clock progression and the
scheduler's post-tick RNG are asserted.

The supplied VMNetTick commands are empty. Fixture writes are explicitly
pre-tick interventions, not native client commands or authenticated multiplayer
input. There are no external-result completions in this scenario. The native
checkpoint protocol and original snapshot/command conversion remain unchanged.

## Trace, provenance and deliberate failures

Each successful process emits sixteen complete rows with these ten fields:

`tick, object_id, attribute_0, attribute_1, attribute_2, attribute_3, stack_count,
clock_ticks, rng, entities` (tab-separated).

This is a **scoped observable projection**, not a complete original VM save or
all internal state. The runner checks each row against the declared scenario
using exact integer arithmetic and requires two fresh process runs to produce
identical bytes. Runtime-identity sidecars bind the actual executing CLR,
pointer width, original VM/thread types and loaded assembly/witness/resource
hashes. PowerShell's runtime is recorded separately as `launcher_runtime`.

Fault-only builds of the separate test driver omit tick 7 or deschedule the
entity just before tick 7. Neither build patches the original library. The first
must fail the clock/tick assertion, and the second must fail the attribute
assertion while the VM clock still advances. Both must retain exactly six valid
preceding rows and produce the expected error, rather than failing for an
unrelated compiler/environment issue. A third control runs the ordinary witness
without explicit opt-in and must fail before VM initialization or metadata output.
Unexpected original dialog/chat diagnostics and abort state fail the witness.

The runner retains partial traces and diagnostics on failure and always writes
`vm-execution.json` after its attempt. Its `passed` flag becomes true only after
both ordinary executions, exact row validation, binary identity checks and all
three expected-failure controls succeed. A successful workflow must additionally
pass the final source-identity check. Verify the exact run/head named in PR #44;
an earlier green assembly build does not prove this runtime gate passed.

## Reproduction

Use an isolated Windows source checkout with Visual Studio MSBuild/Roslyn,
NuGet, original Microsoft Profile7 targeting references and Python. The new
`.github/workflows/swarm-f-vm-bootstrap.yml` contains the exact restore/build
commands. After its temporary source and original assembly build steps:

```powershell
& "$Output/source/tools/reference-csharp/run-original-vm.ps1" -Output $Output
```

Use a new temporary output for each full verification attempt. The runner refuses
to replace an existing per-run trace or runtime-identity file. Runtime execution
has a 60-second process deadline. Captured stdout/stderr are capped after process
completion; this trusted fixture runner is not an untrusted-stream memory guard.

## Execution history and limits

The initial build failed on missing .NET Framework 4.5 references, then on missing
Profile7 references under the isolated search root. These tooling dependencies
were supplied without changing original product source. Whole assembly build
run `37655417250` then passed.

The first witness compile incorrectly accessed a private Attributes list; source
inspection led to use of original public SetAttribute, which performs resizing.
The first runtime assertion then incorrectly expected the source routine to
write 30 into an initially zero attribute. Comparing its declared A/B inputs
showed that 30 is the preserved input, not a source-assigned constant. The fixture
was corrected and now checks every modified and preserved attribute. Neither
failure establishes a defect in the original engine or a Rust parity bug.

Run `37658727153` at `9a485fabe0d7e09514d0255cbafb9ebb43ebcfeb` passed original VM
startup and sixteen scheduled executions in each of two fresh processes. The
subsequent expanded trace/runtime-identity/fault-control changes require their
own fresh hosted verification, reported on PR #44; that earlier run does not
substitute for their acceptance. No local execution or independent review is
claimed by the hosted results.

Next F work: align canonical original/native state and scheduling for broader
interpreter traces, then add native/WASM differential cases with unchanged source
behaviors, authorized/effective content, command and external-result schedules.
Installed content, full original objects, source routing/avatars, TS1/TSO-wide
coverage, durable effects, private EOD state, independent authenticated players
and release/device/load qualification remain open.
