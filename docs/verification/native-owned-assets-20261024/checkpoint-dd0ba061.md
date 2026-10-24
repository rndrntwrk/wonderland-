# Native saved balance and owned wardrobe — 24 October 2026

Native players can now select their Sim and inspect its **saved balance and existing owned outfit instances**, refresh after a failed read, close without late data returning, and continue into/out of the actual native lot. This reuses durable source-owned account records. It does **not** implement purchase accounting, object inventory, outfit changing/selling or a live in-lot economy. **Buy/Build remains disabled.**

Tested source: `dd0ba061b5ca2612cd9b7a4a74f203fd2adcb4a8`, tree `ab4e74cecdba5590b444cbd84f56dedd3f16d323`. Base: draft #85 at `49b1e782c3e43dbad5e0c55858781b373df344d0`, tree `8f502e957a27b7f2a9d150d1d5e8d019325c60d6`; local equivalent `5796b869`. Later inventory/evidence/prerequisite documents do not change tested product inputs. Public delivery overlays scoped postimages on #85, excluding private local history and inputs.

## Implemented prerequisite

The existing `AccountAuthority` already stores `avatar_balances`, `avatar_outfits` and explicit ownership-initialization receipts. The new `ownedAssetsPage` reader reauthorizes the account, checks that the avatar belongs to it and requires that receipt. Its SQLite transaction reads saved budget and one numerically ordered page of owned instances. A read neither invokes CAS initialization nor funds, debits, purchases or grants ownership. The original full private `ownedAssets` method and callers remain unchanged.

`POST /native-account/assets` accepts exactly `avatar_id` and `after`. Existing strict same-origin/session/body/method handling remains. The loopback and planned Container forwarding allowlists explicitly include this path; no arbitrary RPC is exposed. The provider validates scope, canonical decimal identities, saved budget range, instance ordering/uniqueness, page size and cursor before returning data to Rust. Private request state remains behind the existing provider handle. Account action/sign-out aborts its bounded read; late scope changes reject. Closing or changing the selected UI scope fences its pending result before presentation.

A transport page holds up to 64 instances plus a has-more sentinel, rather than imposing a wardrobe-size game limit. Cursor IDs remain exact strings above JavaScript's safe integer range. Each page is an independent atomic saved-state read; a consistent multi-page snapshot across future inventory mutations is not promised. The panel clearly labels **Saved balance** and lists owned instance/asset IDs with original CAS suit categories. It does not confuse catalog eligibility with ownership or claim that every listed outfit's artwork has been rendered.

## Actual qualification

- **53 client cases** pass, including four new saved-assets cases for exact large IDs/page cursors, foreign scope/malformed records, bounded pages and aborted/late sign-out reads. Existing account, creation, chat and Timer cases remain included.
- **Six real workerd/SQLite HTTP groups** pass: original CAS gives three distinct owned instances and zero saved balance; repeat/isolation/strict input checks; 64-instance pagination and exact numeric IDs above 2^53; full workerd restart/read-redelivery; revoked session isolation; and uninitialized legacy ownership failing closed without recreating rows. The 75 extra pagination rows are explicitly test-only storage inputs, not purchases or original object models.
- **Eleven existing account HTTP groups** and **16 policy/forwarding cases** pass. Targeted WASM checking and pinned offline release packaging pass; all **153 artifacts** were rehashed. The verified native host is reused because its Rust inputs are unchanged. No dependency installation or native rebuild was needed.
- **Four fresh rendered groups** pass on actual original-account fixtures at desktop 1200×800 and mobile 390×844: saved zero balance/three CAS instances; actual successful read with a deliberately dropped reply and explicit refresh; pending read completed after Close without reopening/repopulating; mobile controls, real source-owned native entry/departure, preserved saved identities, disabled Buy/Build and sign-out cleanup.

The Browser plugin is unavailable; the established sandbox-enabled Playwright/Chromium path was used. Initial title/origin and meaningful UI were checked; no blank/error-overlay state appears in reviewed final images. There are zero page exceptions. All 14 recorded roster/assets/entry/leave responses are HTTP 200; one deliberate ERR_FAILED and one existing preload-integrity warning are retained. Desktop/mobile images were visually reviewed and mobile refresh remains reachable without horizontal overflow. The fixture URL is ephemeral loopback, not a hosted gameplay link.

The missing public route first failed semantically with HTTP 404. The preceding browser build also reproduced the missing “Your possessions” control. Initial client tests failed because the new read method was absent; those are not mislabeled semantic network tests. No actual rendered inspection failure occurred after the implementation. Failed setup-script construction and all red receipts remain private and preserved.

The type-check attempt using `allowImportingTsExtensions` still fails on the existing generated `account_crypto.js` declaration TS7016; no full type-check pass is claimed. Existing unqualified-invocation TS5097 and proc-macro future-compatibility warnings remain recorded. Review is by the sole implementer under the no-extra-agent instruction, not a new independent review.

## Reproduction and evidence

```sh
node --test apps/web-shell/scripts/native-assets.test.mjs apps/web-shell/scripts/native-owned-account.test.mjs apps/web-shell/scripts/native-creation.test.mjs apps/web-shell/scripts/native-chat.test.mjs apps/web-shell/scripts/native-timer.test.mjs
# From services/native-edge, using its existing dependencies/generated crypto:
node tests/account-assets-http.mjs
node tests/account-http.mjs
node --experimental-strip-types --test tests/preview-policy.test.mjs tests/supervisor-outbound.test.mjs
```

The real browser fixture additionally needs the retained original avatar cohort, native authoring inputs and matching pinned build. It cannot be reproduced from public source alone without those private inputs. [Sanitized evidence](checkpoint-dd0ba061-evidence.json) contains precise outcomes and hashes; private screenshots, accounts, SQLite stores, original bytes and raw traces are not public downloads and are not uploaded.

## Remaining acceptance and external dependencies

The next Buy/Build source unit is the durable construction-effect provider: bind trusted quote/consent/authority to debit/object ownership/inventory, exact operation receipts and geometry acknowledgement; then qualify ambiguity, crash/replay, fencing, rejection/rollback and identical retries. The existing construction session leaves accounting host-owned and the old controlled provider is RAM-only. A read-only budget/wardrobe surface does not close that obligation, regardless of successful synthetic persistence tests.

Actual rendered purchases additionally require the permitted original object cohort and an end-to-end native ownership/persistence journey. Object inventory, outfit changes, full city/multi-lot and the broader game remain unfinished. Timer's missing original graphics/panel gate and real-object needs cohort remain open. The [consolidated external prerequisites](../../integration/EXTERNAL-PREREQUISITES-20261024.md) separate missing content, authorization, startup-upload/hosted identity, plan/cost confirmation and Linux build capacity from this unfinished source work.

No new agents, real money, credentials/grants, installation, paid services, external provider actions, private asset/media uploads, merge or deployment occurred. Original staged work, prior stacked PRs, private inputs and existing failure evidence remain intact.
