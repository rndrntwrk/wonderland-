# Connected UI continuation

This continues the approved FreeSO UX/UI redesign from
[PR #18](https://github.com/rndrntwrk/wonderland-/pull/18), exact checkpoint
`a318581f33770540808aefcf132018255a3a1d94`. The branch is
`feat/ui-connected-continuation`, based on `feat/playable-world-integration`.
The original [integration handoff](connected-integration.md) retains startup,
operator prerequisites, source pins and its historical verification record.
The [capability map](player-capability-map.md) remains the full surface list.

The design still centers on the Sim, city map and lot. All returned account
Sims remain reachable through pagination, head and body choices stay independent,
and existing preview profiles, Home, outfits, possessions, budgets and versioned
saves remain supported. A page of five cards is not an account limit.

## Original avatars in source lot snapshots

The connected lot resolves the snapshot's exact numeric or named head/body
outfit references, skin, original skeleton, bound appearances and decorations.
It composes original mesh and texture resources and renders the first restored
zero-time pose defined by the original VM loading path. Ordered animation
layers are sampled without advancing time; carry is applied last at its source
integer frame. Crossed animation events are not replayed.

Avatar placement uses raw source tile contact, original graphics units and
`pi - yaw` orientation without the object-model center offset. Color rendering
and depth picking use the same posed geometry and texture samples. Vitaboy UVs
wrap after interpolation; nearest filtering remains an explicit software
approximation. Accessory references deduplicate within the original accessory
domain, retaining bound appearances followed by Back, Head, Tail and Shoes.

Unsupported rigs, missing resources, incomplete visibility data, contained
SLOT/bone placement and invalid pose state remain diagnosed. A bad avatar does
not cause an invented appearance or discard other valid avatars. Asynchronous
resource decoding is fenced by content generation, snapshot identity and
session/lot identity. Resource-only replacement cannot silently change accepted
architecture, positions or source record identities.

This is a restored snapshot pose. Continuous animation, pre-save visual history,
head seeking, full original lighting/material passes and original VM replay
remain separate integration work.

## Incoming original lot chat

The native gateway observes original chat, join, permission, ignore and channel
commands, projects visible source speech, and assigns a delivery sequence for
the browser. The existing Chat panel displays original sender names, channel
labels, private status, unread state, retained scroll, channel visibility and a
lot-scoped draft. Profile **Message** explicitly opens a private conversation.

Original direct commands have no message ID. Equal text in separate native
deliveries is retained; replayed gateway deliveries are deduplicated by their
sequence. Snapshot bubble text does not become invented history. Native tick
catch-up, actor incarnations and session/lot changes are handled separately.
Actor state, retained text and projected output have resource bounds. Losing
source authority clears the usable chat projection.

Outgoing lot chat remains an original actor-bound command. A socket write does
not establish delivery; the panel does not insert an optimistic message with a
fabricated receipt. Persistent source read/filter preference synchronization
and remaining specialized presentation are separate work. Tick projection
targets connected TSO server streams; paused sandbox/TS1 execution is outside
this adapter contract.

## Correct source timing, needs and entry

The original server's StateSync wrapper carries the **next** ordinary tick ID.
Chat, source world and native object-dialog observers preserve that pre-tick
boundary: the first real tick with the same number is admitted once. A cached
older snapshot cannot replay a dialog grant or make a newer world look current.
Tick rollover and stale session/lot frames remain fenced.

Both needs displays share the original indices and labels:

| Source index | Need |
| --- | --- |
| 5 | Energy |
| 6 | Comfort |
| 7 | Hunger |
| 8 | Hygiene |
| 9 | Bladder |
| 13 | Room |
| 14 | Social |
| 15 | Fun |

Raw signed values are preserved. Incomplete arrays produce unknown needs, not
fabricated healthy values. The earlier profile display mislabeled six needs.
Matching successful property admission now closes the Property panel to reveal
the entered lot. Pending, failed, stale or duplicate admissions do not dismiss
an unrelated panel.

## Responsive source city

Actual source terrain fills the connected scene. Its buffer preserves the
displayed aspect ratio and is capped at 393,216 pixels and 960 pixels per axis.
Terrain, pins and pointer movement share the measured viewport; pins publish
only with their painted terrain frame.

Selected place and Whole city use the map space above the mobile directory or
beside the desktop directory. Resizing preserves geometry, directory updates
do not reset a panned camera, and a failed map change cannot leave old terrain
visible. All directory pages and unsupported-map directory access remain.
Very short narrow screens leave limited map height while showing the full
directory, so an explicit Whole city view can become small.

## Local sound controls

Sound loads the unchanged `/audio/source-audio.mjs` module, shares in-flight
loading, exposes errors/retry and fences late imports on cleanup. MP3 streaming
participates in the initiating browser gesture. Playback failures and
superseded Pause/Resume/Dispose operations have explicit ownership. Invalid
stream admission does not consume voice identity or replace an earlier
generation.

WAVE memory reservation accounts for browser resampling before native decoding;
seeks remain measured in original source frames. Mute and Music, Effects,
Voices and Ambience gains persist under `wonderland.audio-preferences.v1`.
Uploaded files stay in memory for the current session. Closing Sound preserves
playback; hiding the page suspends audio; leaving the authenticated view or
page disposes players and source files.

Local music audition remains separate from synchronized gameplay sound. The
accepted runtime audio driver still needs an active-lot call site and original
cue/content/camera providers. Original banks/stations and complete effects,
voices and ambience are not established by local MP3 playback.

## Reproducible controlled source

The loopback example contains a valid deterministic 8 × 8 source-shaped avatar
snapshot. Its packed city location is **16777472**, tile **(256, 256)**, so the
directory entry also produces a selectable map pin. Database lot ID remains
**1**, account avatar **42**, and source object ID **7** at lot tile **(4, 4)**.
Distinct needs and exact Robin head/leathers body references support browser
checks. Original resource bytes are loaded separately through Game content.

```sh
python3 services/browser-gateway/examples/fixtures/generate_synthetic_avatar_fsov38.py
cargo test -p wonderland-browser-gateway --example controlled_replay --locked
WONDERLAND_REPLAY_BROWSER_ORIGINS='http://127.0.0.1:4173' \
  cargo run -p wonderland-browser-gateway --example controlled_replay --locked
```

Select connected startup with gateway `http://127.0.0.1:18787` and public fixture
values `controlled-player` / `test-only`. The example performs native city and
lot handshakes and sends an original SimJoin followed by chat from synthetic
sender Controlled Bob. Snapshot and first ordinary tick share the source-correct
tick ID. Refresh returns the same test world with a newer transport tick.
Other submitted commands do not fabricate mutations or receipts.

The [fixture documentation](../../../services/browser-gateway/examples/fixtures/README.md)
records source field authority and a reproducible hash. Original parser golden
fixtures stay unchanged. These are controlled synthetic peers, not a deployed
Wonderland server or continuous simulation.

## Verification

Verification was run from this continuation's isolated checkout with the
pinned Rust 1.99.0 and Trunk 0.21.14 toolchains. The
[unedited screenshot gallery](connected-continuation-gallery.md) records the
connected desktop and narrow browser views.

| Check | Result |
| --- | --- |
| `cargo test --workspace --locked --offline` | 1,317 passed, zero failed; five explicit environment-dependent tests ignored by the default suite. |
| Controlled native replay example | Two fixture and native-chat tests passed. |
| `node --test crates/audio-runtime/browser/*.test.mjs` | 59 passed, zero failed or skipped, including delayed dialog opening/focus, native resampling budgets, playback races, cleanup and equality of the canonical/public modules. |
| Opt-in original-resource avatar render/pick test | Passed with the supplied local original pack and decoded pixels: four parts, 668 triangles, three textures, and a depth pick retaining the source avatar identity. This was run separately from the default suite. |
| Native and WASM Clippy with `-D warnings` | Passed. |
| `cargo fmt --all -- --check` and `git diff --check` | Passed. |
| `trunk build --release --locked --offline` | Passed; the produced WASM release was used for browser checks. |
| Original source preservation | `TSOClient/` and `Other/` remain byte-identical to baseline `4c6b3e8f5835b228723caea3c9f683c62f244f73`; `Cargo.lock` is unchanged. |

The four other opt-in gates (Mono avatar oracle, indexed original IFF corpus,
source-derived IFF v1 input, and original semantic corpus) were not run in this
continuation. The build emits the existing future-compatibility notice for
`proc-macro-error2` 2.0.1; strict project lint checks still pass.

The connected Chromium checks use a 1440 × 1000 desktop viewport and a
390 × 844 mobile/touch emulation. They sign into the controlled native peers,
select the actual city pin, enter its property, verify admission panel closure,
refresh the source snapshot, and render the supplied original avatar resources.
The lot's eight need percentages agree with the corresponding signed profile
values. Incoming native chat appears once with its original sender, and an
unsent draft survives panel close/reopen. The small direct-resource appearance
pack verifies exact snapshot resources; it does not establish the full creator
collection catalog.

The final audio browser run verified actual original MP3 playback advancing
past 0.2 seconds, Pause retaining and freezing its position, Resume advancing,
37% Music gain and Mute on the media element, Stop retiring its stream, and
keyboard focus returning to Sound settings after closing. Signing out disposed
the active player, removed the media source and cleared loaded files. After
reload and a new sign-in, Mute and 37% Music volume were restored while the
file list remained empty. No original audio files are included in this change.

The existing preview passed a separate 1364 × 936 browser journey: select
Nico, Play, select Harbor Café on the map, observe pending Visit, enter after
its acknowledgement, request Make coffee, observe Accepted, cancel and wait
for cancellation, then enter Nico's Home. One acknowledged $180 armchair
purchase changed the budget from $1,250 to $1,070. Reload retained identical
save bytes, all roster identities, the same object instance at cell (2, 2),
and $1,070 without another charge. Screen/actor navigation resets on reload;
the test explicitly reselected Nico. This used fresh isolated storage and
made zero gateway or external browser requests.

No uncaught page errors or console errors were observed in those successful
checks. Chromium reported preload-integrity and software-WebGL warnings, with
expected canceled requests during view changes. Mobile emulation does not
establish physical-device performance or compatibility.

Focused regressions cover source pose/resources, wrapped alpha/picking, needs,
source timing, chat visibility/lifecycle/bounds, responsive terrain/pin alignment
and audio loading/playback/cleanup. Independent reviews checked avatar and chat
semantics against the unchanged original source.

## Remaining integration

The unfinished work includes continuous original VM restoration/replay, full
person/object action discovery, complete catalog/inventory/construction/property
providers, remaining profile/relationship/bookmark adapters, specialized object
dialogs and active-lot sound. Other cities and missing original content need
their actual providers. Operator endpoint/account acceptance remains a separate
prerequisite; controlled browser fixtures and native tests do not establish
deployed original-server acceptance.
