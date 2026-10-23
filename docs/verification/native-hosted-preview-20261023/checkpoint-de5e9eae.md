# Native Container source checkpoint — 23 October 2026

Local source `de5e9eae2564ae744b901837811648c92d52f537`, tree `c938a87aa3806c353dd43b7e1cb80959b355331b`. This follow-up publishes 16 exact source postimages on the existing PR #81 head `78709a09cf3a9b32775612286a414e26e690949b`. It incorporates local `6f7b89348fa7796221051c519fa03546d761cb9d`, `5b7d98f9fcdc3fb4509dac1309d00d529c43d4c8` and `de5e9eae2564ae744b901837811648c92d52f537`; earlier hosted/loader checkpoint `d77509c6` and its predecessors remain in the prior public handoff.

The inactive Worker entry now routes authenticated native HTTP/WebSockets into one Container DO, and a private outbound binding supplies the existing account/coordinator RPC and hash-pinned bootstrap. The coordinator selects pristine startup or durable recovery. Bootstrap stages only allowlisted, size- and hash-verified private inputs into a new cache. It does not register accounts or weaken source authority, transaction checks, fencing, persistence, acknowledgement or replay rules.

A fixed window of at most one hour is retained across DO/process restarts. The durable stop alarm is persisted before paid start; requests never renew it; Node PID 1 independently exits against wall and monotonic deadlines. Provider alarm timing and billing granularity prevent representing this as a hard dollar cap. The committed Wrangler template is inactive with unusable placeholders.

## Actual validation

29 focused cases passed: preview policy/lifecycle 5, outbound transport 9, RPC 6, bootstrap/expiry 4, Container adapter and startup classification 5. The adapter tests execute the actual account/lot modules and crypto WASM with a fake Container API: they are offline tests, not Linux image execution.

A real native/Miniflare fixture at `6f7b8934` passed two-account login and source-owned admission, authenticated binary sockets, used-ticket replay rejection, foreign-Origin/public-RPC rejection, complete process shutdown, and cold recovery from persisted SQLite/R2 into a new empty native cache. Both original sessions reconnect with the same actors and registry. Its tested Node bootstrap/supervisor and coordinator bytes match this checkpoint; the later Container HTTP forwarding correction is covered by adapter tests.

Code-only image preparation verified all 149 existing browser artifacts and produced 784 context files from this committed tree. No original game bytes or private history are in that context. The recipe uses the unchanged Rust host, Node 26 and the existing integrity-pinned ws lock. It requires reviewed compatible Linux/amd64 Docker Official Image digests. No Docker-compatible local builder is installed; no Linux image has been built, pushed or qualified.

The earlier `d77509c6` rendered run is reused with input equivalence: 30 authenticated original resources load automatically, both original player models render and animate, genuine source Walks finish in 7.435791292 / 6.879826167 seconds, with movement and peer hash agreement under the unchanged 45-second deadline. The private GPU deformation witness and recordings remain preserved. They are not uploaded in this source commit; this is not a fresh rendered/performance run of the Container adapter.

Strict edge type-check still reports the existing TS7016 missing generated account_crypto.js declaration at src/account/crypto.ts:7. Earlier broader Rust formatting discrepancies remain. No full type-check/format pass or new independent review is claimed. Historical exact 30.0 Hz gates remain failed; the owner separately accepted scheduled 29.987938/29.940126 Hz. No threshold was changed.

## Hosted activation remains blocked

The live Worker remains the earlier static preview; this source publication does not establish hosted gameplay. R2 REST inventory previously failed with authentication error 10000 and has not been retried unchanged or bypassed. An owner-confirmed existing private bucket, existing R2 activation and narrowly scoped access are required through an explicitly approved credential flow. Do not deploy bindings to route around that denial.

The same approved 30-file browser pack totals 92,632,333 bytes; the earlier 25,411-byte declared-total discrepancy is resolved by the owner's scope clarification. No new original file is implied. The additional native lot/world/sealed release/configuration/descriptor inputs and actual hosted owner/cohort still need separate upload approval. Neither pack has been uploaded to Cloudflare.

Before activation, verify the existing paid plan/allowances, qualify a digest-pinned Linux image, and establish that expected total Worker/DO/R2/Container/network usage fits the approved USD1/one-hour preview. No subscription upgrade, credential/grant change, paid build, upload, merge or activation occurred here. The fixed deadline is implemented; it does not replace billing/access qualification.

The accompanying sanitized JSON records exact source and private-evidence hashes. Private originals, local-only URLs, account/session data, raw positions, recordings, credentials and game content are excluded. Original staged work and all failed attempts remain intact.
