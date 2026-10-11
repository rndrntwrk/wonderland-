# Swarm F baseline gate: execution and evidence contract

This additive checkpoint implements a declared-source drift gate and an observed
interface map. It is a bounded first leaf of W00.1/W00.2/W17.1, not a second game
runtime, a public ABI migration, or completion of those program packages.
Local Codex retains the application/avatar/terrain/audio work recorded in
[`../swarm-f/TAKEOVER.md`](../swarm-f/TAKEOVER.md). No application source, original
assets, existing workflow, root manifest, dependency or lockfile is changed.

## Source and interface mapping

[`../compat/baseline.json`](../compat/baseline.json) records all declared solution
project dispositions and the literal source census. See its
[scope and exclusions](../compat/dispositions.md).

[`../contracts/boundary-map.json`](../contracts/boundary-map.json) records twelve
existing boundaries using 24 whole-file source SHA-256 pins, current consumers,
test-source locations, primary owner and source-specific qualifications:

| Realm | Existing boundaries |
| --- | --- |
| Presentation | Preview UI IDs and eligibility; not native authorization. |
| Native | Signed/generation-aware identity; accepted ticks/clock/RNG; native snapshots; effects; atomic replay; bounded wire; player admission/receipts. |
| Native presentation | Read-only world/avatar projections; not persistent authority. |
| Legacy | Original FSOv/VMNet codecs and original account/Aries compatibility gateway. |
| Content | Manifest/pack identity and the effective runtime content binding. |

All entries are `observed-not-frozen`. The map preserves the native completed-N /
next-N+1 checkpoint convention, separately identifies the legacy snapshot lane,
and records operation identity versus authority epochs. The named
`crates/contracts` preview types are not relabelled native wire types. Existing
`services/browser-gateway/src/native.rs` is original-service TCP integration,
not proof of the new native account/database provider.

A source-file change asks for mapping review even when it is only an internal
refactor. This intentionally conservative pin is **not semantic ABI analysis**.
Evidence labels mean only fixture/test/workflow source present, not passed tests.
The checker rejects promotions to full acceptance or original equivalence based
solely on source existence. Free-text notes and reviewed manifest changes still
require human review; this is not a malicious-maintainer security boundary.

## Run against the actual tracked checkout

Python 3.11+ and Git are the only dependencies. Verification never downloads
submodules, imports project build tasks, installs packages, executes manifest
commands, or updates accepted records. Use the repository's committed LF bytes;
a filtered/autocrlf checkout that changes input bytes deliberately fails the
exact-source check rather than being normalized behind the evidence.

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -B -m unittest discover \
  -s tests/parity -p test_baseline.py -v
PYTHONDONTWRITEBYTECODE=1 python3 -B tools/replay/baseline.py \
  --require-git --output /tmp/wonderland-f-new-evidence
```

The output directory must not already exist. The source tree must be clean,
including untracked non-ignored files. `--require-git` also verifies that the
executed checker and every inspected source/consumer/evidence input are regular
tracked blobs whose actual bytes match HEAD. Thus ignored impostor files and
`skip-worktree` modifications cannot masquerade as the recorded revision.
Declared submodule paths are checked against the actual gitlinks without
initializing their payloads.

Without `--require-git`, a detached input copy can be inspected, but the report
says `unattested-input-copy` and has a null execution commit/tree. Do not promote
that result to a current-repository acceptance result. The initial local inventory
used such an explicitly labelled reconstructed input copy; hosted CI must check
the actual published tree separately.

`--capture-source` prints a candidate compact source record to standard output.
It does not run acceptance, edit a manifest, add owner dispositions, approve a
source change or update contract pins. Review any deliberate refresh alongside
the original source/contract change and its lane-specific tests.

## Failure and evidence semantics

Exit 0 means the recorded declared source and observed mapping matched. Exit 1
means source/mapping drift, with the first changed project/field or exact contract
path. Exit 2 means malformed/unsupported input, unsafe/missing path, byte/depth
limit, invalid Git provenance or an output error. All failures return a structured
error/result rather than auto-approving a new baseline.

A successful evidence directory contains:

- `source-census.json`: ordered declarations, source hashes and explicit gaps.
- `report.json`: passed status, baseline-input commit, actual tested commit/tree
  when identified, manifest hashes, complete census digest, counts, inspected
  Git blob/gitlink verification and drift diagnostics.

Both original-equivalence and release-qualification fields remain `not-tested`.
Neither a gate pass nor a checked-in map establishes complete active capability
coverage, a frozen multiplayer ABI, lawful asset distribution or game parity.

The new read-only `Swarm F baseline` workflow executes the checker/tests on the
PR checkout, never generates application call sites, and retains the reports,
complete test log and Python/Git versions. It does not change branch protection
or make itself a mandatory repository merge rule. Existing Rust/WASM/browser
workflows and other swarm branches remain untouched.

## Regression coverage and execution record

The 52 standard-library tests cover literal/conditional/linked/duplicate source
entries, SDK and submodule exclusions, imports, missing/changed source, exact
required boundary membership/realms, evidence non-promotion, strict JSON/GUID/XML,
path escape/symlink/size rejection, deterministic output, read-only manifests,
exclusive artifact creation, real CLI failure statuses, real Git identities,
ignored inputs, skip-worktree drift and changed submodule pointers.

The current local suite ran **52 tests, zero failures**, on Python 3.13. The local
input audit found 47 declared projects, 40 present project files, seven
submodule-backed declarations, three SDK implicit-source sets and 1,977 present
literal Compile targets. Its execution scope is unattested-input-copy, not a
fresh full Git checkout. Hosted results belong to the subsequently published
commit and must be checked separately in the PR/artifact.

Behavioral RED evidence was observed for changed-file acceptance, wrong realm,
insufficient project-level drift localization, required-slot reassignment,
overflowing JSON exponents and malformed GUID admission before their fixes.
An earlier absent-module import failure is a setup failure, not a behavioral
regression witness. Existing source-reference, native/WASM and browser suites
were **not rerun** by this F-only implementation; none of their historical test
counts are added to the new 52 tests.

## Remaining F work

Reconcile SDK/imported/transitive source closure, active primitive/EOD/screen
registries and effective content rights with A-E. Review/freeze coordinated
native contracts only after mapping their actual producers and consumers.
Build on the [existing extracted reference probes](../compat/oracle.md) toward
an actual reproducible original-runtime harness and localized three-way replay.
Then qualify independent authenticated players, real durable ownership/providers,
private-state isolation, device/load/failure budgets and release operations.
