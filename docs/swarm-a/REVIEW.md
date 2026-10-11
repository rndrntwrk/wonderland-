# Independent implementation review

The simulation work was split between module implementers and separate review
agents. Reviewers read the pinned C# contracts, inspected the integrated Rust
code, wrote regressions for concrete findings, and reran the affected scopes
after corrections. The final runtime, VM/snapshot, world, and avatar reviews
reported no open Critical or Important finding within their documented scope.
These are implementation reviews, not a full legacy-content certification or
an external security audit.

The source baseline is
`4c6b3e8f5835b228723caea3c9f683c62f244f73`. The detailed compatibility ledgers
and the still-open acceptance gates are indexed in [COVERAGE.md](COVERAGE.md).
[VERIFICATION.md](VERIFICATION.md) records final package-wide results. Counts
from focused reviews overlap with those package tests and must not be added
together as distinct coverage.

## Runtime and memory corrections

| Finding | Corrected behavior and permanent evidence |
|---|---|
| Attribute writes could grow valid storage that snapshots then rejected. | Dynamic growth remains bounded and survives completed-tick validation/restore. `runtime_review_regressions::source_attribute_growth_survives_tick_validation_and_snapshot_restore`. |
| FindBest interpreted a stored EngineQuery result as current object use. | Compute use from Occupied flags and real avatar callee frames; a previous safe-delete result does not make an object occupied. Both FindBest cases in `runtime_review_regressions`. |
| Creation Main inputs were reused on every restart. | Consume creation parameter/stack inputs once, including missing/empty Main, and restart with default zero arguments/null stack. Source lifecycle vectors and runtime review regression. |
| Deleting a multitile base broke surviving group projections. | Select the remaining live base consistently and update each surviving group/world projection. Runtime review deletion regression. |
| Burn omitted water rooms from the source pool exclusion. | Treat water and pool rooms consistently with the source query. Runtime review Burn regression. |
| Synchronous checks stopped at their first Sleep yield. | Continue bounded temporary execution at the same lot time, preserving scheduling/RNG effects according to the check context. Runtime review finite-Sleep regression. |
| A temporary check hid the real avatar's executing stack. | Keep current-check and real-entity thread views separate, including multiple removed owners and exact generations. Runtime review engine-query and overlay regressions. |
| Successive conditions in one primitive reused stale registers. | Use the current request's latest Temp/XL banks and propagate shared changes through the whole primitive. Runtime review two-candidate regression. |
| Child initialization lost a reset aimed at its removed parent. | Track actual detached threads through staged creation; carry reset and register requests back to the executing owner before its next instruction. Runtime review created-child reset regression. |
| Named synchronous calls lost interrupt/countdown state. | Preserve the selected source thread's control state. Apply nested control before the parent's result so a later return/schedule retains precedence. Runtime review named-child tests and `vm_behavior` control-hook regression. |

The additional `runtime_query_alias` suite checks four outside-dispatch cases:
initial real-bank cloning, nested checks that do not overwrite a preview's
current banks, explicit scope-13 reads/writes across timer yields, and named
destination selection where the current temporary thread and real entity
thread have the same entity reference.

## VM and snapshot corrections

The VM review corrected zero-argument bounds panics, failed Create cleanup that
did not cover the whole source group, and Burn's detached-list collection
behavior. `vm_behavior`, `primitives_entities`, and `primitives_fire` retain
the permanent regressions.

Saved requests now bind to the actual originating BHAV opcode, operand bytes,
request family/kind, frame position, and captured parameters/locals. Forging a
matching VM request and effect payload together cannot bypass that source
binding. Independently mutable Temp/XL values remain valid during a wait.
`snapshot_contract` and `primitives_external` cover these distinctions.

Short and XL deferred write batches are jointly checked before either bank
changes. Invalid indices or oversized batches cannot partially apply. Snapshot
validation additionally checks final TS1 family/inventory projections and the
absence of those projections in TSO mode.

## World and route corrections

Source review fixed the complete HeadSeek person-data vector; SnapToDirection's
single exact point, mask-derived heading and lack of scoring RNG; short wrapping
after each multitile average addition; Temp0 preservation on route failure;
and sitting-posture clearance after a successful Stand callback.

Later failures now expose the enabled failure callback before terminal
completion, carry the optional blocker, and publish PersonData 62 before the
callback event. The callback remains bounded after search-budget exhaustion,
survives a snapshot, and cannot repeat after acknowledgement/timeout.
`runtime_routes_source` and `world_routing` retain these regressions.

The source reviewer also ran seven isolated probes. Six supplied valid
pre-correction assertion failures. The initial Temp0 probe assigned its marker
before `StartBehavior(replace=true)` replaced the bank; its setup was corrected
to assign the marker inside BHAV before routing. The actual Temp0 defect was
independently established from the unwanted register write and is covered by
the corrected probe and permanent test. Temporary probes are not counted as
additional permanent package tests.

## Avatar and foundation corrections

Avatar review fixed source motive ordering in floating-point advertisement
scores, animation-reset head seek, aggregate/synthetic event-queue overflow,
relationship/social capacity checks, barrier-overflow atomicity, and lifecycle
phase/timer consistency. Source-specific regression fixtures cover each case.
Later resource integration tests also check ASCII case aliases, significant
whitespace, legacy female-job overrides, and their hand-group provenance.

Foundation review fixed ordinary 30 Hz UTC rounding to match the observed Mono
DateTime policy, rejected out-of-domain UTC states, and prevented successful
tick completion while scheduled entities remain undispatched. The single-wake
scheduler is retained as an explicitly documented replacement, including its
possible dispatch and RNG consequences; review does not call it source parity.

## Review evidence boundaries

The final focused runtime/memory/query/snapshot review passed 59 tests. The VM
reviewer's final selected replay passed 108 tests, including 26 snapshot and
four query cases. The independent world review passed 68 permanent tests.
The avatar review included native source-vector execution and optimized
numerical comparisons; later resource helpers were separately exercised.

The final complete native and optimized suites supersede these subsets for the
branch's aggregate pass count. Native/WASM agreement, full original-engine
comparison, actual imported object cohorts, authenticated services, and physical
browser/performance qualification remain distinct forms of evidence. See the
final verification record and coverage ledger for exactly which gates ran.
