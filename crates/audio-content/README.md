# Original audio content adapter

Source baseline: FreeSO `4c6b3e8f5835b228723caea3c9f683c62f244f73`.
Portable engine imported from
`f6f78be1fef247f2db47e19f56d94054f0c9e88c`, `crates/audio-runtime/src/**`.
The integrated engine has equivalent Rust 1.99 lint/style corrections, including
bit-identical UTK literal spellings, documented in
[audio-runtime's source maintenance notes](../audio-runtime/README.md#imported-source-maintenance).
The original `TSOClient` source is unchanged.

`bind_group` parses original HIT/EVT/HSM resource bytes through the existing
bounded legacy readers. It preserves the complete HIT file and absolute PCs.
The original `Audio.cs` group order is represented by `TsoGroup` and enforced by
`EventBank::new`. `bind_track` takes the actual DBPF instance ID, retains source
TRK metadata and backup IDs; `bind_hitlist` takes an explicit source encoding and
retains encounter order including duplicates. `SourceFwav` binds provider-owned
IFF FWAV event names with separate object scope and global fallback.

`prepare_sample` joins the original PCM WAVE/XA/UTK header parser to the imported
portable decoder. It hashes original bytes into content identity and reports
float32 browser allocation, rather than PCM16 source allocation. MP3 is decoded
or streamed by the browser, never by this Rust decoder.

`AcceptedAudioSession` admits exact causal `CueId` records through the imported
ledger. Neither browser frame arrival nor completed device playback is an
accepted simulation operation. Owner reconciliation includes generation.
`AudioCadence` supplies fixed 60 Hz audio ticks independently of the 30 Hz VM.
It discards a suspended browser tab's presentation backlog; it cannot retire a
cue identity. The caller must advance cue retirement only after reconciliation.

`position::source_gain_pan` ports `VMEntity.cs:339–406`: original screen-space
squared pan, three-dimensional side vector and 2.25 exponent, distance/zoom
attenuation, level attenuation and final volume clamp. Camera/entity inputs must
come from the active source scene. The original 3D code applies zoom even when
the operand's NoZoom flag is set; this quirk is preserved.

## Browser boundary

`SourceAudioControls` mounts a compact sound settings dialog. All four groups and
mute affect active node gains and streamed music. User activation creates/resumes
real Web Audio contexts. User-selected MP3 and long PCM WAVE Music use bounded
HTML media streaming with owned blob URLs; short WAVE uses `BrowserAudio`'s
bounded decoder. Music streaming does not allocate a full float32 AudioBuffer.

Local file audition and accepted runtime playback use distinct adapters so local
preview playback cannot consume/replay the runtime's generation/serial identity.
Original samples can be registered on `acceptedAudioHost` by their SHA-256 keys.
`start_audio_driver` holds an accepted session, ticks at 60 Hz and returns finished
voice identities only to `complete_voice`; retain its `AudioDriver` for the active
incarnation. Drop clears the timer and stops owned voices.

## External resources and limits

No soundtrack, HIT bank or fabricated effect is bundled. The current installed
avatar resources contain no original audio bank. Full effects/voices/ambience
require operator-supplied original HIT/EVT/HSM, DBPF TRK/HLS/sample identities,
FWAV bindings and station/FSC manifests. The API accepts those provider records;
the file audition UI accepts MP3/WAVE only. Audio device success never establishes
live city/lot session success. Pure tests use explicitly identified byte fixtures;
the browser tests control only presentation/platform interfaces. No probe sine or
square wave is imported as game content.
