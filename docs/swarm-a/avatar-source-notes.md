# Swarm A avatar behavior: source coverage and integration notes

Source baseline: `4c6b3e8f5835b228723caea3c9f683c62f244f73`.

Scope: W05.1–W05.4 and the avatar provider used by the animation/motive
primitives and SL.9 continuation tests. Ownership is limited to
`crates/sim-core/src/avatars/`, `tests/avatar_*.rs` and this document. The
interaction offer/check/queue pipeline belongs to B; physical slots and routes
belong to W04; entity dispatch and snapshot envelopes belong to the runtime.

The implementation provides functioning source-derived state transitions,
headless animation, scoring, directed relationship variables and participant
coordination. It does **not** close the entire W05 content cohort: actual
licensed animation/curve/tuning resources, B's interaction providers and the
baseline social/NPC BHAV corpus are still required for full-content acceptance.
The participant coordinator is an explicit integration protocol, not a claim
that original social BHAVs have been replaced by hardcoded Rust gameplay.

## Source anchors

The implementation was checked against these actual baseline files:

- [VMAvatar] initializes motives/person slots, advances animation and motives,
  implements head-seek timers, skill policy, computed person variables, reset,
  leave/deletion timers, neighbor inheritance and suit storage.
- [VMAnimationState] flattens animation properties, stores frames and event
  counts, and contains the intentionally unconsumed `OrderBy` expression.
- [VMAnimateSim] defines modes 0–3, resource absence, reset, speed/hurry,
  event-before-completion and expected-event synthesis.
- [Animator] defines the headless integer-frame completion test;
  [SimAvatar] supplies the 15-tick head-seek blend length.
- [VMMotive], [VMMotiveChange], [VMAvatarMotiveDecay], [VMTS1MotiveDecay] and
  [VMSetMotiveChange] define numeric indices, caps, fractions and clock gates.
- [VMPersonDataVariable], [VMNetSkillLockCmd], [VMNetAvatarPersistState],
  [VMOutfitReference], [VMSuitProvider], [VMSuitScope], [Outfit] and
  [VMNetSetOutfitCmd] define person widths, persisted mappings and outfits.
- [VMFindBestAction], [TTAB] and [TS1Curve] define advertisement selection,
  score arithmetic, stable ordering, attenuation and RNG use.
- [VMChangeSuitOrAccessory] defines the default/body/accessory update branches
  exposed to primitive 6 through the avatar appearance helpers.
- [VMRelationship], [VMThread], [VMEntity], [VMNetSimLeaveCmd] and
  [VMNetSimJoinCmd] define relationship storage, cancellation/idle requests,
  reset/entry points, special avatar initialization and lifecycle boundaries.

## Public integration API

All authoritative fields derive `Clone`, `Debug`, `PartialEq`, `Serialize` and
`Deserialize`. Integer-only types also derive `Eq` where applicable. Ordered
maps/sets or explicit source ordinals determine iteration. No renderer, network,
wall clock or OS RNG is imported by `src/avatars/`.

| API | Purpose and required caller behavior |
|---|---|
| `AvatarState::new(EntityRef, PersistentId, AvatarPlatform) -> Self` | Construct source-default state. `validate()` rejects invalid positive local IDs, zero generations and malformed continuation fields before admission. |
| `AvatarState::tick(&AvatarTickContext) -> Result<AvatarTickOutput, AvatarError>` | Run the avatar portion **after** the entity's VM thread dispatch. Failure is atomic. |
| `read_motive(u8)` / `write_motive(u8, i16)` | Bounds-checked 16-slot source motive memory. Writes use source overfill rules. |
| `read_person_data(u16)` / `write_person_data(u16, i16)` | Bounds-checked 101-slot person memory with computed variables and source skill/job/outfit side effects. |
| `apply_motive_change(u8, i16, i16, bool, bool)` | Arguments are motive, raw rate, raw maximum, ClearAll, Once. Returns whether a continuous assignment passed the source Ticked gate; the VM primitive succeeds even when that Boolean is false. |
| `apply_animation(&AnimationCommand, Option<&AnimationMetadata>)` | Preferred VM adapter entry point. Includes reset's head-seek person-data change as well as timeline operations. |
| `set_default_daywear`, `set_body_outfit`, `set_accessory_name` | Primitive 6 bookkeeping after B resolves the actual suit. Body updates retain the original operand index even when a temp selected the resource. |
| `resolve_head_seek_target(Option<EntityRef>)` | Runtime resolves raw person-data slot 41 to the current live generation before each avatar tick. Missing target is `None`. |
| `persistent_state()` / `apply_persistent_state()` | Off-lot persistent avatar data with source sleep/outfit resets. Do not use these for in-lot continuation snapshots. |
| `AnimationTimeline::apply`, `tick`, `validate` | Lower-level timeline provider. `apply` never advances time. `tick` returns cues, not presentation acknowledgments. |
| `score_candidates(&AutonomyContext, &[InteractionCandidate], &AutonomyTuning)` | Immutable ranking. It cannot draw RNG or mutate avatar/queue state. |
| `autonomy::select(..., &mut SimRng)` | Source selection over B's eligible candidates. Returns a selected candidate, an already queued target or no valid target; B owns enqueueing and final live-state checks. |
| `RelationshipState::apply(RelationshipRequest)` | Directed relationship reads, sets and additions, with persistent/local/neighbor addressing selected by the adapter. |
| `SocialCoordinator::{begin, ready, finish, cancel, depart, disconnect, reconnect}` | Serializable participant continuations; requests cancellation and releases exact externally supplied physical reservation tokens. |

`AvatarTickContext` contains minute/hour, room score, category, thread presence
and pause state, hidden state, out-of-world/container conditions and the results
of checking leave-action availability and whether the leave action is already
queued. `container_out_of_world = true` includes the case of no container.

The caller must maintain `skill_mode`, `lot_category`, `has_thread`, permissions
and avatar flags from authoritative lot/avatar policy before a VM memory write
that consults skill policy. These fields are serialized; a UI query must not
change them. `budget_mirror` is a read-only behavior/projection value, not an
account or transaction authority.

`PersonWrite.written` reports actual data mutation, **not** the C# setter's
success result. Blocked skill writes, ignored slot 70 writes, display flag writes
and job-subfield writes with no job entry all return success in the source.
`InvalidJobId` for a value above five maps to source setter false. The runtime
adapter consumes queue-dirty, money-headline, ghost, display-flag and unresolved
TS1 outfit requests. It must not interpret these outputs as permission to alter
a balance or invoke a presentation callback as simulation authority.

## W05.1: identity, person data, motives, skills and outfits

### Motive widths and initialization

The source motive memory has 16 signed 16-bit slots, initialized to 100 except
SleepState, which starts at zero. The exact indices are retained:

| Index | Motive | Index | Motive |
|---:|---|---:|---|
| 0 | HappyLife | 8 | Hygiene |
| 1 | HappyWeek | 9 | Bladder |
| 2 | HappyDay | 10 | UnusedMental |
| 3 | Mood | 11 | SleepState |
| 4 | UnusedPhysical | 12 | UnusedStress |
| 5 | Energy | 13 | Room |
| 6 | Comfort | 14 | Social |
| 7 | Hunger | 15 | Fun |

`set` clamps to `[-100, max(old_value, tuned_limit)]`. An existing overfilled
need can fall gradually; setting another high value cannot create additional
overfill beyond that prior value or the current limit. Persistent avatar load
replaces raw motive slots, preserving existing source values without applying
this setter clamp. Source maximum scaling is `(old_max - 100) + tuned_limit`,
then signed-short narrowing. See [VMAvatar] and [VMMotiveChange].

Continuous changes retain a double fraction. The per-tick rate is
`(per_hour / 60.0) / 30.0`, divided by another five in TSO mode. A change can be
assigned only after its prior tick has marked it Ticked; the first assignment
between ticks wins. Clearing changes resets rate/maximum and preserves both the
fraction and Ticked state. Reaching the cap does not clear a change. If already
beyond its directional cap, the source retains even the accumulated whole
fraction. Once changes and fractional additions narrow before subsequent
short comparisons. These quirks are covered by `avatar_motives.rs` and the
exact-source Mono fixture. See [VMMotiveChange] and [VMSetMotiveChange].

### Natural decay

TSO decay requires explicit `TsoMotiveTuning`: the twelve flattened source
`simmotives` values and the seven weights for each of the eleven source lot
categories. There is no made-up default TSO decay table. Missing tuning at an
actual decay minute produces `MissingTsoTuning` without partially applying an
avatar tick. Category values above ten use category zero. The energy decrement
intentionally ignores the energy lot weight, as does the pinned source.
Hunger is recomputed inside each motive iteration, so bladder can observe
hunger after the earlier decrement in that same minute. The fixed multiplication
casts the 64-bit product to 32 bits **before** division by 1000.

TSO room score updates each tick, even within the same minute and when the cheat
flag suppresses natural decay. The minute gate, signed-short fraction wrapping,
awake/asleep choices and eight-term Mood mean match [VMAvatarMotiveDecay].

TS1 uses the two-minute gate, hidden-avatar suppression and the actual constants
from [VMTS1MotiveDecay]. A motive with an active change is skipped except Energy.
The skipped contribution is omitted from Mood's numerator while the divisor
remains eight. Comfort treats active personality values below, equal to and
above 666 separately. The source's `ToFixed1000(0.000125f)` is zero, so the
outgoing-personality contribution to social decay vanishes. SleepState -1 has
its special energy restoration path; the wake-hour test retains the source's
captured `sleeping` Boolean for the rest of the tick.

Positive TS1 Energy restoration scales by integer `180 / (24 - 16)`, or 22.
TSO negative rates are unchanged; positive rates on Services lots receive the
integer 3/2 multiplier and non-Comfort gains are inversely scaled by lot weight.
Scaling precedes signed-short narrowing. Invalid zero divisors or unsupported
motive/category tuning paths return typed errors instead of panicking.

### Person data, skills and persistence

Raw person data has exactly 101 signed-short entries. The constructor sets Neat
personality slot 7 to 1000. Tick counter slot 27 wraps as a short. Source job
subfields read from the selected job map; roommate status, TSO friend counts and
the script skill-lock mask are computed. The underlying skill-lock allocation
count remains raw slot 70, separate from the computed mask. Source skill writes
have no new arbitrary 0–1000 clamp. See [VMPersonDataVariable] and [VMAvatar].

Skill policy preserves the source precedence: forced enable, nonpersistent
NPCs and TS1 use multiplier one; a new player on a Welcome lot uses multiplier
two; ordinary policy then follows the lot's skill mode and visitor status.
Multiplier zero blocks both increases and decreases. Multiplier two affects
positive deltas only. The network skill-lock helper clamps an out-of-range
skill ID to Mechanical, sums other locks in hundreds and respects the total
budget. The source accepts negative requested lock levels; that compatibility
behavior is retained and named in its test. An upstream accepted-command policy
can reject such a request before invoking the helper. See [VMNetSkillLockCmd].

`AvatarPersistState` retains the actual 27-slot persisted person-data array and
the source's 23-entry copy mapping, including aliases and spare fields. Apply
forces skill loading, preserves custom-avatar gender, loads raw motives,
replaces persistent relationships and job information, applies worker status
and separately assigns the raw skill-lock allocation. Save resets SleepState
to zero and substitutes default daywear for naked or temporary rack outfits.
In-lot `AvatarState` serialization retains current sleep, clothes, events,
fractions, leave counters and all other continuation data. See
[VMNetAvatarPersistState].

### Outfits and legacy special cases

Outfit identity preserves unsigned 64-bit IDs, the legacy named-reference
sentinel and TS1 inline definitions/hand-group references. Default and dynamic
suits, four decoration slots, gender-specific naked/mishap/default suits, all
five three-level job suit tables and invisible/skeleton selections match
[VMSuitProvider] and [VMAvatar]. Stored outfit updates do not automatically
change the currently worn body, matching [VMNetSetOutfitCmd].

The primitive 6 default-update helper changes both default daywear and body
without changing the current-outfit index. A resolved body update stores the
original operand's index; exact string accessory keys share the animation
appearance set. Decoration toggling in [VMChangeSuitOrAccessory] is entirely
under `UseWorld`: it does not change the stored decoration outfit IDs or provide
a gameplay result. The headless adapter can emit a presentation request or
perform the source headless no-op; it must not overwrite the stored suits.

The TS1 provider consumes bounded body-string inputs, a supplied hand-group
reference and resolved job uniform strings. It implements shape/skin naming,
child versus age-zero distinctions, naked/swim/formal/sleep/skeleton and
expanded wardrobe branches. Parsing the source generic outfit reference remains
hexadecimal even when the TS1 argument is true, because [VMOutfitReference]
explicitly overwrites that argument. Resource lookup, appearance geometry,
hand-group decoding and object/global STR 304 resolution belong to B/C.

Pet gender bits (8 dog, 16 cat), source child-age tests, custom GUID identity,
neighbor family membership and preservation of original gender/person type/
visitor schedule are implemented. Worker job validation and the nonpersistent
NPC skill exception are represented. Death and gardener/maid/repairman counters
retain their source slots and persistence mapping; advancing the corresponding
BHAV behavior is still B content. No new death, service-NPC, chair or bed game
logic is invented.

## W05.2: headless animation and continuation

`AnimationMetadata` carries resource identity, frame count and flattened time
properties in motion/property/item encounter order. A time property keeps its
raw ordered-map keys/strings. Recognized cues run in source order: xevt, right
hand, left hand, sound, dress and undress. Invalid xevt text becomes zero, as in
`short.TryParse`; invalid gesture text is an explicit admission error.

The pinned `GetTimeProps` calls `TimePropertyLists.OrderBy(...)` without using
the returned sequence. This implementation deliberately preserves encounter
order, including a future record blocking earlier-time records behind it.
Forward ticks consume from the front; backward ticks consume from the back.
Equal-time records preserve that encounter direction. A tick may consume many
records. All xevts from blended layers go to the **first** animation state's
queue, matching `CurrentAnimationState` in [VMAvatar]. Values below 100,
including negative values, increment the wrapping byte event count.

Source speed is f32 `30 / 25`, with hurry multiplying by two when walk style is
one and the operand permits hurry. Frames use explicit f32 operations without
FMA. [Animator]'s silent progress truncates to an integer before testing: a
reverse frame of -0.2 still has integer frame zero. Looping wraps once by the
animation frame count, without repopulating consumed time properties. Carry
animations do not advance or emit time properties in the source headless path.

Mode 0 starts/waits; mode 1 replaces the body timeline with a looping animation;
mode 2 replaces the frozen carry animation; mode 3 clears carry and plays/waits.
Missing nonzero animation resources return `CompleteNextTick`; missing reset
resources complete immediately. ID-zero reset ends active head seeking except
for mode 3's early carry-only return. The caller supplies the reset posture's
walk resource and optional default carry resource. See [VMAnimateSim].

At end, expected-event synthesis appends the current event-count values until
the expected byte count is reached. Queued and synthesized events are delivered
before completion; only then are body animations cleared. The renderer never
signals completion or controls xevt delivery.

Snapshots retain full metadata, resource-order consumption cursors, f32 frame,
speed/weight, direction/loop/end flags, event queue/count, carry, gestures and
bound appearances. `discard_legacy_elapsed_properties` is a dedicated helper
for the older marshal's lossy frame-based reconstruction. It must not be called
when restoring a native Rust continuation, whose cursor is already serialized.

Head-seek weight and its target generation are serialized because its timeout
and completion modify person-data slots 42/43/45 even without rendering.
Semantic ticking preserves the 15-tick maximum, timeout-to-return transition
and missing-target return path. Pose/quaternion interpolation is presentation
work and is not needed to complete the behavioral state machine.

## W05.3: autonomous selection and deterministic queries

Candidates are typed outputs of B's offer/check pipeline. `OfferStatus`
explicitly communicates permission, carrying, repair, check-tree and missing
action failures. The scorer does not run or replace those checks. It enforces
source out-of-world/disabled/occupied/use-count and stray-pet restrictions over
the supplied state. The caller deduplicates interaction-group leaders and gives
each candidate a unique stable source entity/TTAB ordinal.

The implementation preserves the equal nine-part happy calculation (the
source's newer weighting mode is disabled), source adult/child curves, minimum
thresholds, minimum/delta handling, personality/skill modifiers, distance across
levels and visitor/resident attenuation. Advertisement records are normalized
by motive index, matching the fixed TTAB motive array order. This is necessary
because f32 accumulation is order-sensitive. A nine-motive fixture gives exact
score bits `0x412e60ee`; reversed adapter input produces the same result.

Active advertisements are determined by the original nonzero delta. If an
optional check-tree change dictionary exists, an absent key zeros that field,
matching the source `TryGetValue(out ...)` behavior. Delta-plus-minimum narrows
to a short. The source's repeated-first-pie-item bug is retained: every returned
variant adds the first variant again to the ranked candidates. The interaction
ID is narrowed to a byte; occupied and final use-count join checks consequently
use separate pre- and post-narrowing IDs.

Ranking is stable by score after stable source enumeration. Use-count filtering
happens before taking the best four candidates. A higher-priority already queued
action returns the first such queue entry and uses no RNG. No positive candidate
and top-candidate AutoFirst also consume no RNG. Otherwise selection draws
`next(10000)` exactly once and uses the source cumulative `<=` test. The test
with seed 123 asserts the resulting RNG state is exactly 4,127,195,237.
Repeated immutable ranking queries leave selection state and RNG untouched.
See [VMFindBestAction], [TTAB] and [TS1Curve].

## W05.4: relationships, participants and lifecycle

Directed relationship vectors preserve source signed-short values, variable
extension, FailIfTooSmall, clamped/never-clamped writes and category-scaled
addition. Addition narrows and wraps before clamping. Persistent writes become
dirty at the same point as in [VMRelationship], including the existing-target
but insufficient-variable case. TSO prefers persistent targets unless forbidden;
TS1 can select neighbor IDs; other targets use generation-aware local refs.
The two TSO friend-count person variables intentionally use the same outgoing
persistent matrix, with variable 1 >= 60 and target ID below 2^24.

`SocialCoordinator` admits participants atomically, rejects busy participants
and duplicate physical slot claims, and stores B action IDs/roles, connection
epochs, readiness barriers and completion/cancellation state. A barrier releases
all participants in deterministic entity order at one supplied simulation tick.
The coordinator neither advances real time nor plays an animation to prove
readiness. A transport disconnect retains authoritative behavior; reconnect
updates the session epoch and old-session readiness is rejected. Actual departure,
reset, death, target deletion or lost reservation can cancel the interaction.
Cancellation is idempotent and emits B cancellation/idle/EOD requests plus exact
W04 reservation tokens for release. Both participants must finish before normal
completion releases the claims. Physical grants must be validated by W04 on
admission and rechecked against world revisions by the integrating owner.

The lifecycle provider preserves leave interaction 173/routine 8373, explicit
permission bypass for that source leave action, cancellation requests, repeated
forced idle timeout, blink/display/sleep updates and the source 1800-tick forced
delete threshold. No leave action sets the timer to 1800 so the next tick deletes;
normal leave deletes only once the incremented timer exceeds 1800. Generation
and session checks prevent reused entities or obsolete sockets from controlling
an existing participant continuation. Reset's VM stack/queue clear and pet reset/
main entry-point dispatch remain the VM/B owner's responsibility; avatar reset
clears body animations, appearances and priority and requests EOD disconnect.

## Validation and intentional compatibility boundaries

The source frequently assumes valid resources or allows unbounded allocation.
This crate uses typed errors to reject malformed continuations before committing
them. A separate actual-runtime replay below checks native/WASM equivalence
for four explicit fixtures.
The following are deliberate policy boundaries, not claims about source limits:

- Local references require positive IDs and nonzero live generations; original
  local relationship IDs did not carry a generation.
- Person data must have 101 entries. Animation metadata, event queues, appearance
  sets, relationship maps and participant registries have explicit finite bounds.
  Strings and frame/speed magnitudes also have admission limits.
- Non-finite continuation floats and invalid gesture records are rejected. The
  numeric helper uses the pinned Mono conversion policy for out-of-range casts.
- Animation `apply` and `tick` validate the resulting continuation atomically,
  including blended or synthesized events that could exceed the event queue cap.
  Such excessive input is rejected; ordinary source event ordering is retained.
- Relationships and terminal social records reject map growth past 65,536 before
  mutation. Source relationship storage was unbounded. Terminal social records
  must be rotated/adapted by a future integrated registry policy if that lifetime
  capacity is reached; the core does not silently discard idempotency records.
- Barrier counter exhaustion is rejected before changing readiness. The
  lifecycle phase must agree with its serialized kill timer: Active/-1,
  Leaving/0..1800, DeleteRequested/1801; Removed must be disconnected.
- TS1 inline outfit definitions and head-seek weight are preserved in Rust
  snapshots where the old marshal did not preserve all relevant transient data.

Snapshot deserialization must still be bounded by the root codec before
allocation, and the root must validate cross-entity live generations and world
reservation ownership. Per-avatar validation does not possess the whole world.
No private EOD payload is stored by these avatar providers.

## Acceptance evidence

| Suite | Main evidence |
|---|---|
| `avatar_motives` | All source indices/defaults; overfill; Ticked assignment gate; retained fractions/caps; exact TSO and TS1 decay vectors; missing tuning and integer rate scaling. |
| `avatar_state` | Person widths; skills/lock budget; outfit selection and persistence; custom pet load; worker/neighbor state seams; UTF-16 message timer; short tick wrap; head-seek restore; primitive rate narrowing and reset side effects. |
| `avatar_events` | Multiple events, original unsorted order, reverse/hurry/loop/carry, absent resources, synthetic completion, queued-event snapshot, blend forwarding and byte wrap; atomic queue-bound failures. |
| `avatar_autonomy` | Stable ties and duplicated first variant, precise score bits and ad order, UI/RNG isolation, exact RNG draw count, queued/AutoFirst/no-target fast paths, permission/carry/repair/stray/use-count rejection and byte-narrowed joins. |
| `avatar_social` | Barrier save/reconnect, participant contention, generation-aware departure, idempotent cancellation/release, relationship wrap/clamp/friend count, source leave deadline, capacity and counter atomicity, malformed lifecycle snapshots. |
| `avatar_source_reference` | Rust exact-bit vectors plus an explicitly enabled actual-source Mono fixture described below. |

Run the six suites from the repository root with Rust 1.75.0 and Mono available:

```sh
cargo +1.75.0 test --manifest-path crates/sim-core/Cargo.toml --locked --test avatar_autonomy --test avatar_events --test avatar_motives --test avatar_social --test avatar_state --test avatar_source_reference -- --include-ignored
```

The Mono test is ignored by default because it needs `mcs` and `mono`. The
explicit command above executes it. It copies seven unchanged baseline classes
(motive enum, person enum, motive change, both decay classes, animation state
and TS1 curve) into a temporary test directory, extracts the actual avatar
animation loop/xevt portion, silent-frame method and completion synthesis
fragment, and compiles them with minimal content/storage shims. No source assets
or renderer are required. The source-contract reviewer checked that all ten
referenced source files match the pinned baseline. The fixture and extraction
recipe are preserved in `tests/avatar_source_reference.rs`.

Observed Mono/Rust vectors include:

| Vector | Exact result |
|---|---|
| TSO first decay | Fractions `400,400,170,420,187,250,27`; Mood 97 |
| TS1 first decay with Hunger restoration active | Fractions `0,500,170,420,375,250,55`; Mood 87 |
| Cast-before-divide multiplication | `FracMul(1073741824, 6) == -2147483` |
| Restoration while over cap | Value 3; retained double fraction bits `3ff0000000000000` |
| Restoration after clear/reassign | Value 5; fraction bits `3fe0000000000000` |
| Three animation frames at source speed | f32 bits `3f99999a`, `4019999a`, `40666667` |
| Unsorted real events plus synthesis | Queue `7,101,-3,2,3`; byte count 4 |
| One-motive autonomy score | f32 bits `3de39000` |

This is an extracted-method reference, not the full original engine oracle.
The full FreeSO runtime, licensed resource corpus, B offer/queue integration and
original scripted socials/NPCs remain distinct validation gates. Runtime
continuation and native/WASM replay checks are covered separately by the four
explicit fixtures below. Those fixtures do not close a whole object or avatar
content cohort.

## Independent review and remaining handoff

The source-contract reviewer found and reproduced event-queue admission overflow
on blended/synthesized events, mutation before barrier exhaustion rejection,
registry growth beyond validated capacity, advertisement-order score drift and
inconsistent phase/timer snapshots. Those defects were corrected with atomic
preflight/postvalidation and focused regressions. Review also prompted the
avatar-level animation reset wrapper and an exact one-draw RNG assertion.

The independent source reviewer verified all 42 distinct debug tests, including
the actual-source Mono reference. The full 40-test run preceded the final two
appearance/persisted-job regressions; the final affected suites then passed
23/23. The independent optimized numerical run passed 13/13 (seven autonomy,
five motive and one Rust reference-vector test); the Mono comparison had already
passed in debug. Targeted formatting checks also passed. These results establish
the stated W05 provider-boundary coverage, with the remaining integration gates
listed below.

The provider responsibilities at the handoff are concrete:

1. B supplies real animation metadata in source record order, adult/child score
   curves, exact TSO tuning, resolved outfits/hand groups, and effective TTAB
   candidates with source ordinals, original active-ad ranges and check results.
2. B owns action reconstruction/enqueueing, revalidation, all queue cancellation
   mechanics and the actual BHAV social/death/service-NPC behavior corpus.
3. W04 validates physical slot grants, performs mailbox rescue placement and
   releases grants in response to participant/lifecycle requests.
4. Runtime uses the avatar-level animation/motive adapters, executes the source
   VM-before-avatar tick order, resolves live head-seek generations, applies
   lifecycle requests and validates whole-world references during restore.
5. F integrates these simulation-local interfaces with the shared contracts and
   runs full-runtime native/WASM/reference continuation and content acceptance.

## Runtime resource integration follow-up

The runtime now delegates to `src/runtime_avatars.rs` for resource lookup and
person-outfit changes. This closes the earlier provisional direct animation-ID
lookup and fixed carry-ID adapter. `AnimationKey` is an immutable metadata
ingestion identity. Executable lookup follows `VMMemory.GetAnimation`'s owner,
STR table, current TS1 age and resource-name rules before invoking the already
reviewed timeline. The shared protocol and original resource decoder remain
B/F-owned.

[FAR3Provider.Get][FAR3Provider] and [TS1BCFProvider.Get][TS1BCFProvider] perform invariant case conversion
during resource lookup. The normalized content adapter folds ASCII case,
preserves whitespace and rejects aliases with conflicting frames/time
properties. B must normalize non-ASCII resource names to the pinned original
provider's casing; general Unicode casing parity is not claimed.

| Animation scope | Source STR selection |
|---|---|
| Object (0) | Code owner's AnimationTableID, plus one for TS1 age <18; fallback 129/130 only when the configured whole table is absent. |
| Global (1) | Global owner 0, STR 128, regardless of age. |
| PersonStock (2) | Global owner 0, STR 130, regardless of age. |
| Misc (3) | Global STR 156/157 for adult/TS1 child. |
| StackObject (65536) | Live generation-checked stack object's definition and the same custom/fallback rules as Object. The primitive has already retargeted parameter-derived Object scope to this scope. |

Reset resolves the saved `walk_animations` entry at posture 1, 2, or otherwise
3, and requests the exact source default carry resource
`a2o-rarm-carry-loop.anim`. `initialize_walk_animations` fills the 50-entry walk
and swim arrays from global 150/151 and 158/160, with nonempty object STR150
overrides. Missing normalized whole tables leave their entries unfilled;
partial fixtures do not fabricate stock resources. Oversized tables and missing
required carry metadata reject atomically instead of preserving a partial
state from the source exception path. The helper preserves reset's distinct
Mode3 and absent-resource ordering, and reads Hurryable's WalkStyle from object
data index17.

`resolve_person_outfit` and `apply_person_outfit` use live gender/age and, for
TS1 job suits, current job type/level with immutable `LegacyJobUniform` inputs.
The male-uniform-empty early return precedes the female override as in the
source. An explicitly empty female override after a nonempty male mesh remains
the selected job suit and keeps the computed suit's hand group. The additive
`LegacySuitInputs::resolve_selected_job_uniform` entry point preserves that
ordering without changing existing `resolve` callers. Original-body returns use
B's original body hand-group metadata; other
computed suits copy a stored default-daywear hand group when present. Age zero
uses child animation tables but adult TS1 suit naming, matching the two distinct
source conditions. CurrentOutfit memory signals now apply the resulting body
during the accepted tick. B still decodes body STR resources and literal hand
groups; missing body/job resources produce explicit provider faults.

The integration suite passed 15/15, covering every animation scope, configured
table absence versus missing ID, child table u16 wrap, posture reset, named
carry lookup, Mode3 early exits, generated outfit bounds, live job/gender
selection including the empty-female override, mixed-case provider lookup,
hand-group provenance, CurrentOutfit application and a real BHAV
animation continuation restored from a runtime snapshot. The autonomy host
extension `select_with_random` simply routes the existing selection algorithm
through a supplied `FnMut(u64)->u64`; its focused callback test checks exactly
one bound-10,000 call or zero calls on the source fast paths. All eight autonomy
tests passed after that extension, bringing the avatar-only suite to 43 distinct
debug tests including the previously executed source Mono reference.
The final focused run passed all 35 tests across runtime-avatar integration,
avatar state and autonomy. The empty-female case failed against the previous
adapter before the correction and passed afterward.

The standalone [actual-runtime replay harness][ReplayHarness] passed all five
native fixture tests after these integrations. Its four scenarios cover real
BHAV stack/sleep/RNG behavior, normal/hurried avatar animation and fractional
motives, fenced pending effects, and a VM-triggered upper-floor portal route.
The final matrix passed 16 native cases and 64 WASM executions: 5,840 exact
per-tick comparisons and 320 complete snapshot comparisons, with no WASM host
imports. The application source digest matched before builds and after replay.
[Recorded evidence][ReplayEvidence] contains every checkpoint's byte length and
SHA-256, final state/RNG, ordered tick-hash digest, compiler/runtime versions and
module identity. The synthetic fixtures preserve the original-engine and
real-content cohort gates above.

[VMAvatar]: ../../TSOClient/tso.simantics/Entities/VMAvatar.cs
[ReplayHarness]: ../../tools/swarm-a/replay/README.md
[ReplayEvidence]: ../../tools/swarm-a/replay/results.json
[FAR3Provider]: ../../TSOClient/tso.content/Framework/FAR3Provider.cs
[TS1BCFProvider]: ../../TSOClient/tso.content/TS1/TS1BCFProvider.cs
[VMAnimationState]: ../../TSOClient/tso.simantics/Model/VMAnimationState.cs
[VMAnimateSim]: ../../TSOClient/tso.simantics/Primitives/VMAnimateSim.cs
[Animator]: ../../TSOClient/tso.vitaboy.engine/Animator.cs
[SimAvatar]: ../../TSOClient/tso.vitaboy.engine/SimAvatar.cs
[VMMotive]: ../../TSOClient/tso.simantics/Model/VMMotive.cs
[VMMotiveChange]: ../../TSOClient/tso.simantics/Model/VMMotiveChange.cs
[VMAvatarMotiveDecay]: ../../TSOClient/tso.simantics/Entities/VMAvatarMotiveDecay.cs
[VMTS1MotiveDecay]: ../../TSOClient/tso.simantics/Entities/VMTS1MotiveDecay.cs
[VMSetMotiveChange]: ../../TSOClient/tso.simantics/Primitives/VMSetMotiveChange.cs
[VMPersonDataVariable]: ../../TSOClient/tso.simantics/Model/VMPersonDataVariable.cs
[VMNetSkillLockCmd]: ../../TSOClient/tso.simantics/NetPlay/Model/Commands/VMNetSkillLockCmd.cs
[VMNetAvatarPersistState]: ../../TSOClient/tso.simantics/NetPlay/Model/VMNetAvatarPersistState.cs
[VMOutfitReference]: ../../TSOClient/tso.simantics/Model/VMOutfitReference.cs
[VMSuitProvider]: ../../TSOClient/tso.simantics/Engine/VMSuitProvider.cs
[VMSuitScope]: ../../TSOClient/tso.simantics/Engine/Scopes/VMSuitScope.cs
[Outfit]: ../../TSOClient/tso.vitaboy.model/Outfit.cs
[VMNetSetOutfitCmd]: ../../TSOClient/tso.simantics/NetPlay/Model/Commands/VMNetSetOutfitCmd.cs
[VMFindBestAction]: ../../TSOClient/tso.simantics/Primitives/VMFindBestAction.cs
[TTAB]: ../../TSOClient/tso.files/Formats/IFF/Chunks/TTAB.cs
[TS1Curve]: ../../TSOClient/tso.common/TS1/TS1Curve.cs
[VMRelationship]: ../../TSOClient/tso.simantics/Primitives/VMRelationship.cs
[VMThread]: ../../TSOClient/tso.simantics/Engine/VMThread.cs
[VMEntity]: ../../TSOClient/tso.simantics/Entities/VMEntity.cs
[VMNetSimLeaveCmd]: ../../TSOClient/tso.simantics/NetPlay/Model/Commands/VMNetSimLeaveCmd.cs
[VMNetSimJoinCmd]: ../../TSOClient/tso.simantics/NetPlay/Model/Commands/VMNetSimJoinCmd.cs
[VMChangeSuitOrAccessory]: ../../TSOClient/tso.simantics/Primitives/VMChangeSuitOrAccessory.cs
