# Reviewed performance checkpoint — qualification remains open

**Owner acceptance: the measured scheduled rates of 29.987938/29.940126 Hz are accepted for the requested release. Both historical exact 30.0Hz test gates remain FAIL; no test threshold or source was changed.**

The source reduces repeated native preparation metadata, memoizes immutable content validation and canonical state hashing, reuses a verified immutable journal archive within each receipt-persistence operation, and bounds cadence recovery after serialized stalls. Full source authority, fence checks, transaction validation, replay/recovery, checkpoint readback and exact receipt acknowledgement remain in place. Checkpoint work stays on the serialized path.

Qualification source: local checkpoint `7da6f0b58d154b2e1c4aaef9f677a5c810113150`, tree `7d4495dd8fae9b3f73f486d5465ca40040c267b4`. This update is based on published PR81 commit `23205b88e3eb8c826dcd58ae1c04c6c459b35411`, stacked on PR80. It incorporates the reviewed release/hash/status changes and checkpoints `09078560`, `05b775d0`, `8dbbbef0` and `7da6f0b5`; local checkpoint hashes describe provenance and are not separate published ancestors.

The scheduler allows at most 100 ms cadence debt and six immediate recovery turns. Each turn yields and joins the FIFO queue afresh; initial/inactive/excessive waits and lifecycle, recovery, departure or failure reset the phase. An under-budget burst-limit cooldown waits only the remaining ordinary period, discarding old debt. No source ticks are fabricated or skipped.

Forty focused scheduler tests pass, with no blocking independent source-review findings. Two fresh original two-player flows pass all four Walks, authoritative movement, peer agreement and visual checks in 6.518–7.460 seconds, within the unchanged 45-second deadline.

| Original Walk-window result | Flow 13 | Flow 14 |
| --- | ---: | ---: |
| Verified completed scheduled bodies |420|418|
| Required by strict 30.0 Hz gate |421|419|
| Scheduled/verified body rate |29.987938 Hz|29.940126 Hz|
| Real committed cursor progress |30.130738 Hz|30.083380 Hz|
| Mean body cost |18.858199 ms|17.954351 ms|

**Both original UTC and monotonic 30.0 Hz gates still fail.** Each flow includes two genuine action commits; these are not counted as scheduled bodies. There are no missing commit marks or invalid selected body proofs. The source is reviewed and functionally exercised; this checkpoint does not claim completed 30 Hz qualification.

An exact analysis of the retained window endpoints shows that even a perfectly periodic 30 Hz completion train fails this finite-window gate over 77.54575% of phases for flow 13 and 44.528752% for flow 14. These are phase-circle proportions, not probabilities for the actual scheduler. Actual interior completion spacing also deviates from the nominal period; a pure boundary explanation has not been established. This investigation changed neither source nor gate and reran no gameplay.

No build was repeated: all 147 packaged browser files and the native/exporter artifacts were hash-verified with recorded input-equivalence proofs. The retained native host was compiled from `83c7840a`, browser/WASM from `69d1caaf`, and fixture exporter from `250787f9`; those compilation identities remain distinct from this qualification source.

Known scenery/terrain notices, panel occlusion and Chromium preload warnings remain documented in private run evidence; no page or console errors were observed. This checkpoint qualifies the existing two-context Walk path, not source-timed animation, object interaction or leave/save behavior. No deployment or merge is part of this checkpoint.

Retained checks also include known baseline limitations: the edge package type-check reports TS7016 for the missing generated `account_crypto.js` declaration at `src/account/crypto.ts:7`; the retained targeted Rust formatting check reports six broader source-file discrepancies. These checks are not labeled passing. They were not rerun for this publication, and no dependency or unrelated formatting change was added.

Recipient-accessible [evidence summary](performance-checkpoint-7da6f0b5-evidence.json) and [exact source manifest](performance-checkpoint-7da6f0b5-source-manifest.json) accompany this note. The summary preserves test identities, observed results, window endpoints and analysis; hashes identify retained private originals, which are not presented as accessible links. Private inputs, raw logs and recordings are excluded.
