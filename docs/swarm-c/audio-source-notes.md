# W10 audio source, support, and provenance

## Scope and evidence

This implementation targets FreeSO source baseline **4c6b3e8f5835b228723caea3c9f683c62f244f73**. The source audit read the full HIT registry and bodies, thread/VM/owner lifecycle, content provider, station/FSC/ambience tables, original XA/UTK decoders, and A's sound-output seam. The cached B audio metadata contract from head **2c1894** was also inspected. The package is `wonderland-audio-runtime`, with Rust 1.75, MPL-2.0, forbidden unsafe Rust, and release overflow checks.

All shipped sample vectors are **synthetic**. The baseline tree contains no tracked HIT/HSM/EVT/FSC/XA/UTK/UTM/MP3/WAV/HLS/TRK payload files; those resources come from a separate game installation. This work establishes executable behavior for the source registry and declared catalogs. It does not establish which real game events reach which bytecode/sample combinations. An authorized imported-content corpus remains a separate acceptance gate.

The source comparison is executable, not a wrapper mock: [reference_probe.py](../../tools/swarm-c/audio-cooker/reference_probe.py) compiles the unchanged original C# XA and UTK files with Mono, generates 32 synthetic compressed inputs, executes both C# and Rust decoders, and compares **complete WAVE bytes**. The retained [reference-evidence.json](../../tools/swarm-c/audio-cooker/reference-evidence.json) records input/source/output hashes. HIT instruction tests use source-derived expected values and adversarial programs; they are not represented as a C# HIT differential run.

### Primary source anchors

| Area | Baseline source |
|---|---|
| Opcode registry and bodies | [HITInterpreter.cs](https://github.com/rndrntwrk/wonderland-/blob/4c6b3e8f5835b228723caea3c9f683c62f244f73/TSOClient/tso.sound/HITInterpreter.cs) |
| Registers, notes, waits, track lookup and interruption | [HITThread.cs](https://github.com/rndrntwrk/wonderland-/blob/4c6b3e8f5835b228723caea3c9f683c62f244f73/TSOClient/tso.sound/HITThread.cs) |
| Event resolution, sharing, nightclub/music and tick order | [HITVM.cs](https://github.com/rndrntwrk/wonderland-/blob/4c6b3e8f5835b228723caea3c9f683c62f244f73/TSOClient/tso.sound/HITVM.cs) |
| Owner volume and groups | [HITSound.cs](https://github.com/rndrntwrk/wonderland-/blob/4c6b3e8f5835b228723caea3c9f683c62f244f73/TSOClient/tso.sound/HITSound.cs), [HITVolumeGroup.cs](https://github.com/rndrntwrk/wonderland-/blob/4c6b3e8f5835b228723caea3c9f683c62f244f73/TSOClient/tso.sound/Model/HITVolumeGroup.cs) |
| TSO content precedence and fixed station/mode tables | [Audio.cs](https://github.com/rndrntwrk/wonderland-/blob/4c6b3e8f5835b228723caea3c9f683c62f244f73/TSOClient/tso.content/Audio.cs) |
| Station player | [HITTVOn.cs](https://github.com/rndrntwrk/wonderland-/blob/4c6b3e8f5835b228723caea3c9f683c62f244f73/TSOClient/tso.sound/Events/HITTVOn.cs) |
| FSC parser and sequencer | [FSC.cs](https://github.com/rndrntwrk/wonderland-/blob/4c6b3e8f5835b228723caea3c9f683c62f244f73/TSOClient/tso.files/HIT/FSC.cs), [FSCPlayer.cs](https://github.com/rndrntwrk/wonderland-/blob/4c6b3e8f5835b228723caea3c9f683c62f244f73/TSOClient/tso.sound/FSCPlayer.cs) |
| Ambience categories and catalog | [AmbiencePlayer.cs](https://github.com/rndrntwrk/wonderland-/blob/4c6b3e8f5835b228723caea3c9f683c62f244f73/TSOClient/tso.sound/AmbiencePlayer.cs), [VMAmbientSound.cs](https://github.com/rndrntwrk/wonderland-/blob/4c6b3e8f5835b228723caea3c9f683c62f244f73/TSOClient/tso.simantics/Engine/VMAmbientSound.cs) |
| Original sample decoding | [XAFile.cs](https://github.com/rndrntwrk/wonderland-/blob/4c6b3e8f5835b228723caea3c9f683c62f244f73/TSOClient/tso.files/XA/XAFile.cs), [UTKFile2.cs](https://github.com/rndrntwrk/wonderland-/blob/4c6b3e8f5835b228723caea3c9f683c62f244f73/TSOClient/tso.files/UTK/UTKFile2.cs) |
| A sound-owner/operand behavior | [VMPlaySound.cs](https://github.com/rndrntwrk/wonderland-/blob/4c6b3e8f5835b228723caea3c9f683c62f244f73/TSOClient/tso.simantics/Primitives/VMPlaySound.cs), [VMStopAllSounds.cs](https://github.com/rndrntwrk/wonderland-/blob/4c6b3e8f5835b228723caea3c9f683c62f244f73/TSOClient/tso.simantics/Primitives/VMStopAllSounds.cs) |
| Avatar property dispatch | [VMAvatar.cs](https://github.com/rndrntwrk/wonderland-/blob/4c6b3e8f5835b228723caea3c9f683c62f244f73/TSOClient/tso.simantics/Entities/VMAvatar.cs) |
| DJ category/pattern mapping | [VMEODDJStationPlugin.cs](https://github.com/rndrntwrk/wonderland-/blob/4c6b3e8f5835b228723caea3c9f683c62f244f73/TSOClient/tso.simantics/NetPlay/EODs/Handlers/VMEODDJStationPlugin.cs) |

## Provider and simulation boundaries

B owns the raw HIT/HSM/EVT/TRK/HLS/audio metadata parsers. C consumes normalized `ResourceGroup`, `EventBank`, `HitCatalog`, `SampleRef`, and `SampleMetadata` values. C's FSC parser is separate because the inspected B contract does not provide FSC parsing. The cached B file uses newer integer APIs that do not compile under Rust 1.75; C does not copy that parser implementation or claim a direct B dependency.

| B information | C normalized view/use |
|---|---|
| Ordered event entries, original/lowercase names, numeric event type/track | `EventRecord` in source-ordered `ResourceGroup` |
| Ordered HSM constants with first-definition lookup | Ordered `hsm: Option<Vec<(String, i32)>>` |
| Absolute HIT entrypoints | `entrypoints` plus complete original `HitProgram.bytes` |
| Track/sound/loop/hitlist identifiers | `Track`, backup map and ordered duplicate-preserving hitlists |
| Decoded rate, channels, width, frame count and payload span | `codec::SampleMetadata`; decode validates that exact span |
| Authorized source/cooked resource identity | Shared core `AssetKey` in `SampleRef`; provider owns the verified key-to-bytes mapping |

The provider must retain the complete original HIT resource. PCs are absolute offsets including the original header; passing a sliced instruction body with original PCs is invalid. The provider must resolve authorized sample manifests, classify volume groups, and verify identity/provenance before returning bytes. The runtime validates identity representation and dimensions; it does not infer authorization or rehash an arbitrary provider's PCM.

`projection::project_request` consumes an already accepted A output. Opcode 23 reads LE FWAV ID and 8.8 rate, preserves Loop/StackObjAsSource/NoZoom/NoPan bits, looks up scoped FWAV before global FWAV, and preserves the ignored source Volume metadata. Opcode 48 uses **Flags == 1** to select the stack object. It does not reinterpret that equality as a mask. Missing owner/FWAV produces no sound projection, and invalid owner generations/names fail explicitly. The public opcode field remains `u16`, matching the recovered A-view adapter.

Avatar projection retains the actual nested source property ordinal. Non-audio properties still occupy ordinal positions. Sound properties become cues; Dress/Undress remain appearance work. Playback completion, sample duration, autoplay state, decoder success, and hardware clocks have no path back into A outcomes or A's random state.

## Exact 97-slot opcode census

There are **97 source slots, 0x00 through 0x60**:

- **36 operative** handlers, including the documented partial/quirky cases.
- **50 literal no-ops**, consuming no operands.
- **Eight consuming stubs**, advancing over the source operand bytes without claiming the named operation exists.
- **Two inert duck handlers**, whose source methods have commented-out bodies.
- **One broken WaitEqual**, which reads its operands and compares the destination to itself, so it does not wait.

A literal no-op consumes only its opcode byte. No guessed operands are skipped based on a suggestive name such as NoteOff or CallEntryPoint.

| Hex | Source registry name | Operand bytes | Source/C behavior class |
|---|---|---:|---|
| 00 | NOP | 0 | Literal no-op |
| 01 | Note | 0 | Literal no-op |
| 02 | NoteOn | 1 | Operative |
| 03 | NoteOff | 0 | Literal no-op |
| 04 | LoadB | 2 | Operative |
| 05 | LoadL | 5 | Operative |
| 06 | Set | 2 | Operative |
| 07 | Call | 4 | Operative |
| 08 | Return | 0 | Operative |
| 09 | Wait | 1 | Operative |
| 0a | CallEntryPoint | 0 | Literal no-op |
| 0b | WaitSamp | 0 | Operative |
| 0c | End | 0 | Operative |
| 0d | Jump | Dynamic | Operative |
| 0e | Test | 1 | Operative |
| 0f | NOP | 0 | Literal no-op |
| 10 | Add | 2 | Operative |
| 11 | Sub | 2 | Operative |
| 12 | Div | 2 | Operative |
| 13 | Mul | 2 | Operative |
| 14 | Cmp | 2 | Operative |
| 15 | Less | 0 | Literal no-op |
| 16 | Greater | 0 | Literal no-op |
| 17 | Not | 0 | Literal no-op |
| 18 | Rand | 3 | Operative |
| 19 | Abs | 0 | Literal no-op |
| 1a | Limit | 0 | Literal no-op |
| 1b | Error | 0 | Literal no-op |
| 1c | Assert | 0 | Literal no-op |
| 1d | AddToGroup | 0 | Literal no-op |
| 1e | RemoveFromGroup | 0 | Literal no-op |
| 1f | GetVar | 0 | Literal no-op |
| 20 | Loop | 0 | Operative |
| 21 | SetLoop | 0 | Operative |
| 22 | Callback | 0 | Literal no-op |
| 23 | SmartAdd | 0 | Literal no-op |
| 24 | SmartRemove | 0 | Literal no-op |
| 25 | SmartRemoveAll | 0 | Literal no-op |
| 26 | SmartSetCrit | 0 | Literal no-op |
| 27 | SmartChoose | 1 | Operative |
| 28 | And | 0 | Literal no-op |
| 29 | NAnd | 0 | Literal no-op |
| 2a | Or | 0 | Literal no-op |
| 2b | NOr | 0 | Literal no-op |
| 2c | XOr | 0 | Literal no-op |
| 2d | Max | 5 | Operative |
| 2e | Min | 5 | Operative |
| 2f | Inc | 0 | Literal no-op |
| 30 | Dec | 0 | Literal no-op |
| 31 | PrintReg | 0 | Literal no-op |
| 32 | PlayTrack | 1 | Consuming stub |
| 33 | KillTrack | 1 | Consuming stub |
| 34 | Push | 0 | Literal no-op |
| 35 | PushMask | 0 | Literal no-op |
| 36 | PushVars | 0 | Literal no-op |
| 37 | CallMask | 0 | Literal no-op |
| 38 | CallPush | 0 | Literal no-op |
| 39 | Pop | 0 | Literal no-op |
| 3a | Test1 | 0 | Literal no-op |
| 3b | Test2 | 0 | Literal no-op |
| 3c | Test3 | 0 | Literal no-op |
| 3d | Test4 | 0 | Literal no-op |
| 3e | IfEqual | 4 | Operative |
| 3f | IfNotEqual | 4 | Operative |
| 40 | IfGreater | 4 | Operative |
| 41 | IfLess | 4 | Operative |
| 42 | IfGreatOrEq | 4 | Operative |
| 43 | IfLessOrEq | 4 | Operative |
| 44 | SmartSetList | 1 | Operative |
| 45 | SeqGroupKill | 1 | Operative |
| 46 | SeqGroupWait | 0 | Literal no-op |
| 47 | SeqGroupReturn | 1 | Consuming stub |
| 48 | GetSrcDataField | 3 | Operative |
| 49 | SeqGroupTrackID | 2 | Consuming stub |
| 4a | SetLL | 2 | Operative |
| 4b | SetLT | 2 | Consuming stub |
| 4c | SetTL | 2 | Consuming stub |
| 4d | WaitEqual | 2 | Broken WaitEqual (never waits) |
| 4e | WaitNotEqual | 0 | Literal no-op |
| 4f | WaitGreater | 0 | Literal no-op |
| 50 | WaitLess | 0 | Literal no-op |
| 51 | WaitGreatOrEq | 0 | Literal no-op |
| 52 | WaitLessOrEq | 0 | Literal no-op |
| 53 | Duck | 0 | Inert duck method |
| 54 | Unduck | 0 | Inert duck method |
| 55 | TestX | 0 | Literal no-op |
| 56 | SetLG | 5 | Operative |
| 57 | SetGL | 5 | Operative |
| 58 | Throw | 0 | Literal no-op |
| 59 | SetSrcDataField | 3 | Consuming stub |
| 5a | StopTrack | 1 | Consuming stub |
| 5b | SetChanReg | 0 | Literal no-op |
| 5c | PlayNote | 0 | Literal no-op |
| 5d | StopNote | 0 | Literal no-op |
| 5e | KillNote | 0 | Literal no-op |
| 5f | SmartIndex | 2 | Operative |
| 60 | NoteOnLoop | 1 | Operative |

### Operative behavior and source quirks

All integer variables are signed 32-bit. LoadB sign-extends its immediate byte; LoadL reads LE i32. Add/Sub/Mul/Cmp use explicit wrapping behavior, preserving zero/sign flags. Division rejects zero and i32 minimum divided by -1 as typed terminal faults. Set and SetLL share behavior. Max/Min compare an immediate i32. SetLG stores the literal operand byte into a raw-indexed shared global; SetGL reads a raw-indexed shared global. Source data lookup reads the source operand but uses the current owner's object field.

| Variable address | Mapping |
|---|---|
| 0x00–0x0f | 16 registers; register 1 starts at 12 |
| 0x10–0x45 | 54 local integers |
| 0x46–0x63 | Read zero / ignore writes |
| 0x64–0x87 | 36 shared VM globals |
| 0x88–0x2719 | Read zero / ignore writes |
| 0x271a–0x2736 | Up to 29 current-owner object fields |
| Above mapped range | Read zero / ignore writes |
| Negative dynamic index or missing shortened object field | Typed error instead of unchecked indexing |

Call pushes a return PC. End returns through that stack, loops only under the source loop gating, or ends execution. Return kills execution without using the call stack. SetLoop and Loop preserve the original loop pointer behavior. A repeated noninterruptible request from an existing owner does not erase SetLoop performed by a running program; a newly sharing owner applies the source primitive's loop flags. Explicit track LoopDefined takes precedence except for the piano alias.

Jump preserves the source's dynamic target-read quirk: a first operand byte at most 15 denotes a register and changes how subsequent bytes are consumed; a larger first byte is part of a literal u32. TS1 target translation is explicitly represented by `PcMode::Ts1`; missing source mappings translate to zero. Every resulting PC remains bounds checked.

Wait latches its source duration and subtracts the source **16 ms** quantum per audio tick. It rewinds to its opcode while waiting. WaitSamp still halts on the tick that it notices completion, then execution continues on a later tick. NoteOnLoop creates a real looping voice and halts the instruction loop. A missing patch yields -1; it is not reported as a successful decoder result.

Rand uses its operand bytes as inclusive immediate bounds. C declares a separate deterministic audio PRNG, **splitmix64-rejection-v1**, with bounded rejection. This reproduces the source selection/range policy, not the .NET Random seeded bitstream. It never draws from A's RNG. Hitlists preserve encounter order and duplicates. The source first-random-choice cache for a track is represented by runtime selection state, without mutating B's imported metadata. Missing lists produce the source zero choice; empty/index-invalid lists are typed faults.

SeqGroupKill operates only for the source constant instance zero. Other instance values retain the source's lack of action. SmartChoose, SmartSetList and SmartIndex retain source lookup ordering, including SmartIndex's second lookup after SetTrack. Duck/Unduck, named consuming stubs and WaitEqual remain visibly classified as source-inert or broken behavior.

## Event policy, ownership and ordering

Registration order is NewMain, Relationships, TsoEp5, TsoV2, TsoV3, Turkey. The first matching lowercased event wins. Ordered HSM constants use first-definition lookup; `guid_tkd_<event>` may replace the track, while the original event track remains the fallback. Without HSM, entrypoint lookup uses the original event track. `piano_play` resolves its registration before the `playpiano` alias.

Only numeric event type 30 (station) and 36 (music mode) use special player routes. Other event-type names do not invent unimplemented event behavior. Ordinary events use a HIT routine or the source simple-track fallback. Simple-track playback creates one ordinary note and waits for it; its bytecode loop flag is not silently promoted into backend looping.

Ordinary events share a globally active thread across owners. Maximum submitted owner gain wins within a tick; the first equal-gain submission retains pan and object fields. Owner generation is part of identity. Reconciliation removes stale generations, and a sound that once had owners retires when none remain. Unowned UI sounds are allowed. Interruptible replacement retains the latest waiter; superseded waiters and voices are released. Ordinary interruptions stop vocals immediately; nightclub transitions wait for their source boundary.

At each fixed **60 Hz audio tick**, `AudioSystem`:

1. Selects the nightclub HIT suffix using effective group-scaled gain; later equal-gain entries win.
2. Advances HIT and station players in shared insertion order.
3. Retires completed/faulted players and drains queued starts in order.
4. Promotes pending music when the current music is absent/dead.
5. Advances FSC players in activation order and appends their intents after HIT/station starts.

A failing FSC/sample cannot discard another player's already produced intents. Faulted players stop and release their own voices and produce bounded diagnostics. Stops cancel starts that are still queued. All lifecycle feedback remains cosmetic.

`CueLedger::check` is nonmutating. `AudioSystem::accept` commits a cue identity only after its action succeeds. A rejected event/provider/capacity request leaves the ledger unchanged and can retry the same identity when the rejection condition clears. Eager music admission is staged before killing/fading existing music; a replacement can exchange the old pending start's queue slot. Rejected admission restores the audio RNG. Increasing retired-tick watermarks permanently suppress already reconciled history; they are not reset by a transport/reconnect epoch.

### Volume groups

| Source material/route | Group / special behavior |
|---|---|
| XAI and XAJ samples | FX; XaMusic is an encoding name, not the MUSIC group |
| UTK speech | VOX |
| Source fallback PCM/WAV HIT samples | MUSIC |
| Radio/TV events, including MP3-backed playlists | FX; legacy music-player playlists ignore pan |
| UI music modes and direct loadloop | MUSIC |
| FSC notes and ambience loops | AMBIENCE |
| FSC instance default | 0.33 before note/header/group factors |

Native and browser backends receive already computed gain/pan in ordered MixerIntents. Group muting keeps logical playback ownership while producing zero gain.

## FSC and ambience

The FSC parser retains the source header and numeric note metadata. It deliberately preserves the source parser's skipped first candidate row, retains that skipped text for diagnosis, and ignores candidate rows that do not contain 20 tab-separated columns. The source unused metadata remains available; the implementation does not claim unsupported pitch, fades, stereo placement or quantization behavior merely because a column exists.

The sequencer uses strict `time > beat`, source LoopCount-as-delay behavior, the special NONE row, and source random/final-row restart ordering. Probability zero always plays; other values compare a draw below 16. Note gain is note-volume/1024 × header-volume/1024 × instance volume × ambience master. LR pan is LR/512 - 1. Changing FSC instance volume affects newly created notes only, as in the source. C additionally guarantees explicit resource release on stop/fault.

The **39-entry ambience catalog** is exact: eight animal, nine mechanical, four weather, six people, and twelve category-four looping entries. Only category four is mutually exclusive. Unknown GUIDs/IDs return errors instead of the source unknown-GUID fallthrough to bit zero. An explicit reapplication may retry a failed materialization.

Category numbers: 0 animals, 1 mechanical, 2 weather, 3 people, 4 loops.

| Bit | GUID | Category | Source name | Source relative resource |
|---:|---|---:|---|---|
| 0 | `0x3dd887a6` | 0 | AnimalsSongBirds | `daybirds` |
| 1 | `0x3dd887aa` | 1 | MechanicalExplosions | `explosions` |
| 2 | `0x7dd887ad` | 0 | AnimalsFarm | `farmanimals` |
| 3 | `0x9dd887af` | 1 | MechanicalGunshot | `gunshots` |
| 4 | `0xddd887b3` | 1 | MechanicalPlanes | `planes` |
| 5 | `0xfdd887b5` | 2 | WeatherLightingThunder | `thunder` |
| 6 | `0x9e0bc19a` | 4 | LoopBrook | `loops/brook_lp.xa` |
| 7 | `0xfe0bc1a1` | 4 | LoopCrowd | `loops/crowd_lp.xa` |
| 8 | `0x1e0bc1a3` | 4 | LoopHeartbeat | `loops/heartbeat_lp.xa` |
| 9 | `0x5e0bc1a4` | 4 | LoopIndoor | `loops/indoor_lp.xa` |
| 10 | `0x5e0bc1a6` | 4 | LoopInsects | `loops/insect_lp.xa` |
| 11 | `0xbe0bc1a9` | 4 | LoopOcean | `loops/ocean_lp.xa` |
| 12 | `0x1e0bc1ab` | 4 | LoopOutdoor | `loops/outdoor_lp.xa` |
| 13 | `0xde0bc1ad` | 4 | LoopRain | `loops/rain_lp.xa` |
| 14 | `0x3e0bc2af` | 4 | LoopTechno | `loops/scifi_lp.xa` |
| 15 | `0x1e0bc2b2` | 4 | LoopStorm | `loops/storm_lp.xa` |
| 16 | `0x3e0bc2b4` | 4 | LoopTraffic | `loops/traffic_lp.xa` |
| 17 | `0x1e0bc2b5` | 4 | LoopWind | `loops/wind_lp.xa` |
| 18 | `0x1e128187` | 2 | WeatherBreeze | `breeze` |
| 19 | `0xfe128189` | 1 | MechanicalConstruction | `construction` |
| 20 | `0x5e12818c` | 0 | AnimalsDog | `dog` |
| 21 | `0xbe12818d` | 1 | MechanicalDriveBy | `driveby` |
| 22 | `0xde12818f` | 2 | WeatherHowlingWind | `howlingwind` |
| 23 | `0x1e128190` | 1 | MechanicalIndustrial | `indust` |
| 24 | `0x3e128192` | 0 | AnimalsInsects | `insect` |
| 25 | `0xbe128196` | 0 | AnimalsJungle | `jungle` |
| 26 | `0xde128198` | 3 | PeopleOffice | `office` |
| 27 | `0x3e12819a` | 3 | PeopleRestaurant | `restaurant` |
| 28 | `0xbe12819c` | 1 | MechanicalSciBleeps | `scibleeps` |
| 29 | `0x1e1281ac` | 1 | MechanicalSirens | `siren` |
| 30 | `0x1e1281ad` | 0 | AnimalsWolf | `wolf` |
| 31 | `0xbe19bb2d` | 0 | AnimalsSeaBirds | `seabirds` |
| 32 | `0xde19bb31` | 2 | WeatherRainDrops | `raindrops` |
| 33 | `0xbe1a033e` | 3 | PeopleMagic | `magic` |
| 34 | `0xa9b9652a` | 1 | MechanicalSmallMachines | `smallmachines` |
| 35 | `0xa9b96536` | 3 | PeopleScreams | `screams` |
| 36 | `0xa9b96539` | 0 | AnimalsNightBirds | `nightbirds` |
| 37 | `0xa9b9653c` | 3 | PeopleGym | `gym` |
| 38 | `0xa9b9653e` | 3 | PeopleGhost | `ghost` |

DJ pattern mapping swaps categories zero and one, emits object event category+10, and uses three base-four digits. This is a projection of the source object command; audio does not determine EOD outcomes.

## Stations and music

Normalized station playlists arrive from an authorized provider in source encounter order. C does not walk arbitrary directories or run caller-supplied filesystem regexes. The source policy inserts each encountered sample at a random position to make an initial shuffle, then repeats that fixed order; it does not reshuffle every cycle. Commercial discovery precedes station-track discovery. The source commercial-directory helper performs dirname plus one extra path truncation, making the supplied trailing separator significant.

| Code | Source path/pattern |
|---|---|
| KBEA | `Music/Stations/Beach/` |
| KCLA | `Music/Stations/Classica/` |
| KCOU | `Music/Stations/Country/` |
| KCDA | `Music/Stations/CountryD/` |
| KDIS | `Music/Stations/Disco/` |
| KEZE | `Music/Stations/EZ/` |
| KEZX | `Music/Stations/EZX/` |
| KLAT | `Music/Stations/Latin/` |
| KRAP | `Music/Stations/Rap/` |
| KRAV | `Music/Stations/Rave/` |
| KROC | `Music/Stations/Rock/` |
| KMAP | `Music/Modes/Map/` |
| KSEL | `Music/Modes/Select/` |
| KCRE | `Music/Modes/Create/` |
| KBUY | `Music/Modes/.*buy.*\.mp3` |
| KBUI | `Music/Modes/.*build.*\.mp3` |
| KACT | `sounddata/tvstations/tv_action/` |
| KCOM | `sounddata/tvstations/tv_comedy_cartoon/` |
| KMYS | `sounddata/tvstations/tv_mystery/` |
| KROM | `sounddata/tvstations/tv_romance/` |
| KHOR | `Music/Stations/Horror/` |
| KOLD | `Music/Stations/OldWorld/` |
| KSCI | `Music/Stations/SciFi/` |

Music modes are 11→KSEL, 12→KCRE, 13→KMAP, 9→no next playlist, 1→KBUY and 2→KBUI. Mode 5 is the explicit direct looping patch **0x4f85**, started eagerly. Named zero-track buy/build events map to modes 1/2. Unsupported modes or absent authorized playlists fail explicitly.

Current music fades for 180 audio ticks. The source gain uses max(remaining/120 - 0.5, 0) times group master, ignoring instance volume during the fade. This includes a final silent second. If PlayNext runs during a fade, source normal instance gain is used for the newly started sample on that tick. Latest pending music replaces earlier pending music, and ordinary promoted music begins on a later tick. No playback callback advances simulation.

## Decoders, native playback and browser ownership

| Capability | Implemented boundary | Evidence / qualification |
|---|---|---|
| PCM integer WAVE | 8/16/24/32-bit PCM to PCM16; standard output WAVE | Analytic integer fixtures and CLI/cooker tests |
| XA speech/music | Exact predictor/nibble/interleave/history/clamp logic, mono/stereo | 12 synthetic C# differential cases across predictors and blocks |
| UTK | Mono PCM16 reflection/excitation/pitch/synthesis with exact source tables and rounding | 20 synthetic C# differential cases, voiced/unvoiced, half excitation, zero/sinc interpolation, custom codes, history and partial final frame |
| MP3 inside portable Rust decoder | Explicit Unsupported | No false decode-success or placeholder PCM |
| MP3 import | Explicit external FFmpeg executable, forced MP3 demuxer | Real synthetic MP3 encode/decode test; backend/version recorded |
| Browser encoded WAV/MP3 | Actual AudioContext.decodeAudioData with declared decoded-byte budget | Platform codec capability; failure produces diagnostics and completion |
| Native mix | Stereo PCM16, integer-phase zero-order hold and linear pan attenuation | Exact sample/chunking/pause/loop/ownership vectors |
| Native playback | Owned external FFplay process reading bounded PCM WAVE | Buffered-file mode; no live device callback |
| Browser playback | Actual AudioContext/BufferSource/Gain/StereoPanner lifecycle | Controlled-context Node tests plus separate real browser runner gate |

XA uses the full original coefficient table and independent channel history. B's strict complete-block-capacity validation is preserved; C# itself would decode to EOF despite a mismatched declared size. UTK requires mono PCM16 normalized metadata and bounded bit/unary reads. Arithmetic order and ties-to-even PCM conversion follow the original decoder. Synthetic byte equality does not prove every original compressed payload; malformed and extreme values have explicit checked-error paths.

The native mixer bounds active voices, undrained completions, resident PCM bytes, cache entries and per-call output frames. Finished voice delivery applies backpressure instead of silently dropping lifecycle events. Active sample references prevent eviction. Reset clears voices/cache but keeps the accepted voice watermark. A newer generation replaces prior playback; replayed old commands are rejected.

The Web Audio adapter creates/resumes its context on a real gesture and reports autoplay failures. Decode completion order does not reorder source starts. Each inflight request has an identity and cancellation flag. Cancelled requests detach from the reusable key map while retaining their pending count and PCM reservation until they settle. A stale cancellation or completion cannot retire or populate a replacement voice. Asset-key arrays are snapshotted on admission, protecting the content-addressed cache from caller reuse.

Paused voices retain sample phase. A Resume issued during context suspension is queued for later gesture recovery. Interruption drops one-shots, retains loop phase, and closes/replaces a lost context only on an explicit gesture. Reset/disposal fence all owned requests/nodes/cache. Completed or failed voices are returned by bounded `takeFinished()`; they cannot be used as A acknowledgements. A pending gesture cannot resurrect a disposed adapter.

Browser PCM memory is charged before conversion and encoded decode uses an expected float32 AudioBuffer byte declaration. Actual returned dimensions must fit that declaration. Internal platform codec allocation is still a browser boundary; providers must supply trustworthy metadata. XA/UTK bytes must be cooked before entering Web Audio.

## Budgets and intentional hardening

Defaults include 4 MiB HIT programs, 4,096 instructions per audio tick, call depth 64, 128 HIT/system players, 256 notes per thread/FSC player, 512 queued starts, 65,536 catalog/hitlist entries, 32 MiB encoded decoder input, 64 MiB decoded PCM and 16 million decoded frames. Browser defaults are 128 voices, 16 unsettled decodes, 64 MiB float32 PCM and 4 MiB encoded bytes. Configured limits, dimensions, offsets and cross-platform conversions are checked; no recursive/unbounded interpreter execution or implicit dependency install is used.

The cooker snapshots an authorized input, refuses symlink escapes, requires explicit provenance, checks output frame counts and bytes, and publishes output plus sidecar with exclusive creation. Failed publication removes partial outputs. External processes receive bounded wall time, CPU/address/file limits on POSIX, bounded diagnostics, no interactive stdin, and owned process-group cancellation/reaping. MP3 demuxing is forced rather than allowing a playlist/container to reference other local files.

Intentional safety differences from unchecked legacy behavior include unknown opcode errors (0x61–0xff instead of source NOP fallback), typed invalid index/division/PC/stack errors, finite arithmetic and allocation bounds, guaranteed cleanup, source-independent interruption expiry even without VolumeSet, strict XA span equality, unknown ambience errors, and stale generation/request fencing. These are named compatibility boundaries, not claimed source instructions.

## License and remaining acceptance gates

The XA/UTK translation preserves the original SimsLib MPL-2.0 provenance, including Mats “Afr0” Vederhus and Andrew D'Addesio attribution in the UTK source. The repository's bundled MP3Sharp notices conflict: its license file states LGPL-3 while file headers reference GPL-2-or-later. This implementation does not translate or bundle that code. FFmpeg/FFplay are explicitly external executables; their own distribution obligations and exact configured build are separate from this Rust crate. Cooker reports record the selected external version.

The remaining gates are concrete:

- Supply a versioned adapter from B's actual parsed metadata and authorized sample/content manifests, preserving source resource order and identity.
- Exercise authorized real HIT/event/track/FSC/sample payloads to establish reachability and bytecode/sample compatibility beyond synthetic vectors.
- Qualify real browser gesture, decode, suspend/interruption and device behavior on supported deployment targets. Controlled Node tests and a headless browser cannot certify physical speakers.
- Qualify physical native output if needed. The FFplay dummy driver test is intentionally silent; this adapter declares buffered-file playback and no low-latency live device callback.
- Validate project-specific acoustics/resampling if bitwise native reference output and Web Audio device output must be compared. Their declared pan/resampling boundaries differ.

For commands and final verification evidence, see [the package README](../../crates/audio-runtime/README.md), [the cooker README](../../tools/swarm-c/audio-cooker/README.md), and the Task 5 implementation report. No proprietary game audio payload is included.
