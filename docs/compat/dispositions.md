# Swarm F declared-source baseline and dispositions

This is the first **W00.1 intake leaf**, not completion of W00.1, W00 or the
72-package program. It records the C# projects explicitly declared by
`TSOClient/FreeSO.sln`, their literal Compile entries and their source bytes.
It does not run MSBuild or decide that every declared file is active in every
configuration. Conditional declarations, imports and SDK defaults remain visible.

The original source baseline is `4c6b3e8f5835b228723caea3c9f683c62f244f73`.
The inspected integration input is PR #40 at
`a453894c83969f8bdfa1f584e1c763c8231e9fc3`, inherited from PR #39. The actual
execution commit/tree are recorded separately by the gate; these identities must
not be interchanged.

## Observed coverage

| Declaration set | Count | Interpretation |
| --- | ---: | --- |
| C# projects declared in the solution | 47 | Each has a primary intake owner and intended destination. |
| Present non-submodule project files | 40 | Their XML and literal references are inspected without evaluating conditions. |
| Submodule-backed project declarations | 7 | Payloads deliberately not inspected, even in a hydrated checkout. |
| SDK projects with implicit source sets | 3 | Default compile discovery/imported targets remain unevaluated. |
| Explicit Compile declarations | 1,977 | All 1,977 literal targets were present in the inspected source copy. |
| Unique present explicit Compile paths | 1,977 | File identity, not 1,977 implemented or qualified capabilities. |

The SDK projects are FSO.Server.Api.Core, FSO.Server.Core and
FSO.SimAntics.JIT.Roslyn. An empty explicit Compile list in those project files
must not become an empty, completely covered source cohort.

The seven uninspected project declarations belong to FSOMina.NET and the
FSOMonoGame/Lidgren group. Git HEAD pins their two actual gitlinks. The additional
`Other/libs/assimp-net` declaration in `.gitmodules` has **no tracked entry** in the
inspected revision. That unresolved declaration is recorded as `null`, not a
fabricated commit, successfully fetched dependency, or proven dispensable library.

## Machine record and ownership

[`baseline.json`](baseline.json) contains the compact declaration hashes,
per-project Compile-cohort hashes, counts and all 47 project dispositions. A
fresh gate produces `source-census.json` with each literal source target, hash,
condition, Link metadata, ProjectReference and Import declaration. No original
source payload is copied into the evidence.

Owners are **primary intake routing**, not a change to CODEOWNERS or permission
to edit another swarm's files. The routing follows the approved program:
A simulation; B formats/content/object tools; C views/avatar/audio;
D browser interface/platform replacement; E online services/persistence;
F shared compatibility, contracts and release. Cross-cutting project closure must
be reviewed with the affected lanes. Every record remains `not-assessed` for
acceptance: a file census cannot establish behavioral implementation or parity.

## Boundaries and next coverage work

Source outside this solution, SDK defaults, imported build targets, transitive
ProjectReference closure, condition evaluation, active primitive/EOD/screen
registries, effective content/patch ordering and redistribution rights remain
separate inventory leaves. Existing A/B source probes and content inventories
must be reconciled rather than replaced.

The checker fails on a changed/deleted declared source or project, missing
project disposition, altered solution declaration, or changed gitlink. It does
not silently regenerate the accepted manifest. A reviewed source change may
legitimately update these pins; document its ownership and acceptance impact.
The manifest is an observed baseline, not an immutable protocol freeze.

See [the execution and evidence contract](../verification/swarm-f-baseline.md)
and [the original-runtime reference intake](oracle.md).
