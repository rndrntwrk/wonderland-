# Swarm F: complete-resource dependency preflight

Base: PR #45, `e32387449850b8ea2ffb93b77a48110b7b65ab65`.
Original/native/WASM interpreter evidence from that PR remains unchanged.

## Why this checkpoint precedes placed-object testing

The individual unchanged BHAVs used by #44/#45 do execute in the original VM
and Rust. That does not establish that the corresponding **whole objects** can
be imported with their real definitions, every other behavior, interaction table
and required global resources. This preflight retains the complete source IFFs
and original OBJD records instead of replacing them with the earlier small
four-attribute test definitions.

The four admitted inputs are the unchanged casino bar, beach-ball chair,
Christmas flag and cursebook resources already present in the repository. They
contain six actual object definitions, including the bar's original multipart
master and children. The preflight follows 77 present BHAVs and 113 direct call
edges. All four resources remain incomplete when the required global scopes are
not supplied. The result is a **confirmed prerequisite list**, not four objects
qualified for gameplay.

| Source | OBJD records | Named semiglobal | Distinct absent BHAV keys |
| --- | ---: | --- | ---: |
| `Casino_2-Tile_Bar_CC.iff` | 3 | `wetbarsemiglobal` | 11 |
| `Chair_fso_Bouncy_Beach_Ball.iff` | 1 | `skillobjects` | 17 |
| `fso_christmas_flag.iff` | 1 | None declared | 5 |
| `cursebook_set_permission.iff` | 1 | None declared | 8 |

Counts of absent keys are per resource and must not be added as distinct global
functions. The report includes every exact namespace/routine ID and originating
call instruction. A repeated missing key and its call-site explanations are
separate diagnostic strings, not additional dependencies.

For example, the chair actually declares **skillobjects**, not an inferred
chair-specific semiglobal selected from its filename. The checked-in
`Content/Patch/Tuning/skillobjects.otf` and `skillobjects_10xmultiplier.piff` are
not the absent base IFF and must not be substituted for it.

The original `TSOClient/tso.content/WorldGlobalProvider.cs` loads TS1 globals
from the installed `GameData/Global/Global.far`, or TSO globals from installed
`objectdata/globals/<name>.iff`, with tuning handled separately. This preflight
does not claim an authorized installation is present, fetch game assets, apply
patches, invoke Content.Init, or equate TS1 and TSO resource sets. No confirmed
location for these original base resources was found in the available handoff.

## Implementation and deliberately limited authority

`tools/replay/content_scope.rs` uses the **existing** legacy IFF, OBJD, GLOB,
OBJf and TTAB decoders, `ResourceScope` lookup, and the shipping bridge's
`import_bhav` validation. It does not implement an alternative parser, primitive,
interpreter, effective-content loader or simulation state.

The diagnostic graph conservatively includes all local BHAV declarations, all
local/supplied-semiglobal TTAB actions and checks, the first supplied global TTAB,
and declared OBJD behavior references (or the selected OBJf). This is a static
superset: it is not menu permission evaluation, active-path reachability, a copy
of every source function-table exception or a claim that every reported static
reference is used in every mode. A cycle terminates through a visited set; a
missing or invalid root is never discarded. Global and semiglobal routines keep
the original object's private code-owner scope for their local calls.

Wrong-namespace lookups do not fall back to another file. Semiglobals require an
explicit provider matching the original GLOB name. Empty supplied scopes do not
satisfy absent BHAVs. An unapplied PIFF is not admitted as an effective base
resource. Multipart children need one matching original master definition; no
invented master GUID is supplied.

A `static_complete` value refers **only to these conservative static references**.
Even the positive authored unit fixtures always report
`whole_object_runtime_qualified: false`. Dynamic/by-name dispatch, provider
results, effective patch/tuning order, source normalization of footprints/slots,
placed geometry, avatars, actions, effects, rights and release acceptance remain
unassessed. Production admission still belongs to B's effective-content loader
and the actual runtime/provider boundaries.

The fixed probe checks all four raw-file hashes before inspecting any resource.
No resource is reduced to a single BHAV, modified, written back or emitted in
its JSON output. Resource input, decoded chunks and graph sizes have explicit
bounds. Those bounds are not a process-wide memory quota or OS sandbox.

## Reproduction from a committed branch

Apply the additive patch above #45 in a dedicated worktree, inspect it and commit
it before invoking the default clean-checkout mode. Preserve local user changes;
do not reset an existing worktree or apply it over an overlapping implementation.
The repository toolchain and normal Cargo registry access/cache are required.
Default tracked mode also requires the pinned #45 ancestor in local Git history;
the read-only workflow fetches that history explicitly.

```sh
python3 -B tools/replay/run_content_preflight.py \
  --output /tmp/wonderland-content-preflight-new
```

The output directory must not exist and must remain outside the source checkout.
`CARGO_TARGET_DIR` may point to an existing trusted external Cargo cache; otherwise
the build uses the new output's `work/cargo` directory. `--offline` uses only
already installed dependencies. Builds remain `--locked`; the runner never
updates source, Cargo manifests, expected reports or the lockfile.

The runner builds the shipping content/runtime libraries for native and ordinary
WASM targets, uses Cargo's exact compiler-artifact filenames (not a glob selecting
an arbitrary rlib), compiles and executes 27 Rust tests and 10 Python evidence
tests, and runs strict native/WASM Clippy and formatting checks for these F files.
It then runs the probe twice natively and uses the **unchanged #43 WASM host** to
instantiate the same module twice and check repeated reads. The host imports no
simulation behavior. A wrong source-hash executable must exit before emitting a
report. The explicit native `--require-static-closure` invocation must exit 1
for the incomplete original cohort.

A verification pass means the pinned missing-prerequisite report was reproduced.
It does **not** mean static object admission succeeded. The execution record
separates `verification_passed` from `static_admission` and records original-VM,
placed-world and full-object qualification as false. The checked-in expected
JSON is a reviewable metadata graph, not original resource bytes. Editing both
candidate and expectation cannot silently pass: its reviewed digest is pinned
in the validator and changes require an explicit source/expectation review.

The new read-only workflow retains only `evidence/`, never `work/`. The compiled
WASM/native probes embed the original input resources, so they must not be
included in public verification packs or treated as a redistributable game.

## Local result and provenance

Fresh native and WASM builds/execution passed on Rust **1.99.0** and Node
**22.16.0** in the current session. The compiler and Cargo cache came from the
existing conversation verification archive; its container checksums were
recomputed. This is not a new signed-toolchain attestation.

The native and WASM reports are **30,820 identical bytes**, SHA-256:

`d05ecf34f43a397706ed9cbe8594f8ad5141e90893aad3bfed28d8aba5cfa3bf`.

There were **27 passing Rust tests and 10 passing Python tests** with zero
failures/skips in the final run. Separately, the unchanged B content-import and
source-runtime suites passed **11 + 5 tests**; those are existing regressions, not
new game capabilities. The four blocked source resources are the correct
readiness result and are not counted as four playable-object successes.

Local input came from a reconstructed dependency-source copy. All **110 inherited
inputs used by the new build/audit** match the SHA-256 records of #45's exact
hosted tree `b822c1fce46d893ec3b04cf8e43d158fd3b79546`. Candidate F source files
are separately hashed. The record intentionally has a null current Git commit
and `reconstructed-dependency-input-copy`; it is not called a fresh whole-repo
checkout. The optional `--input-copy-reference` parameter implements this
explicit mode; CI must use the default clean tracked-checkout mode instead.

Initial missing behavior was observed through failing scope/evidence tests.
Additional tests exposed a Unicode suffix-boundary panic and acceptance of an
unapplied patch as a base scope; both now reject correctly. An initial TTAB test
used the wrong source-default Flags2 and was corrected as a fixture error, not
called a shipping-engine defect. The strict Clippy correction changed only an
F-owned conditional. Early orchestration attempts exceeded this chat's tool
execution window; their incomplete logs are not successful verification runs.

No new original C# execution, placed-object/scene acceptance, rendered-browser
run, production provider, physical-device test, independent review or hosted CI
run is claimed for this checkpoint. The GitHub connector exposed reads but no
commit/PR writes in this session; this work is delivered as an additive patch and
verification handoff, not a published pull request.

## Next original-content execution gate

The next required input is an authorized original installation or verified
content cohort containing `global.iff`, `wetbarsemiglobal.iff`, `skillobjects.iff`
and the selected mode's corresponding patches/tuning. Do not create empty
replacements, strip original object definitions, rename patches as base IFFs or
infer interchangeability between the two game modes.

Once that input is available, resolve effective resources with B's existing
pipeline, pin their full identities, provide reviewed runtime geometry/slot
metadata, and run real original/native/WASM placement and object lifecycles.
Preserve the strict #45 interpreter/trace/negative gates and original definitions.
W00/W17 parent packages remain open; local Codex retains all client/avatar/audio
implementation work.
