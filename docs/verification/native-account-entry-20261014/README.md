# Staged native account entry

Based on PR74 `2a4af9f78e75fe7d952c7d7f0ea683d5c6b821e5`. This is a separate account HTTP/browser integration increment; it does not reconstruct the unavailable later runtime continuation.

## Implemented

An explicitly enabled same-origin HTTP adapter calls the existing private AccountAuthority for login, roster, session rotation and logout. The authority implementation, avatar reservation, fences and Rust lot host are unchanged. All other RPC paths, public registration, character creation and lot admission remain unavailable.

The existing Rust/WASM application has an explicit `native_account` startup mode. It loads the packaged native-account transport, displays the authenticated account's saved roster and supports refresh/logout. The provider retains tokens only in memory, clears passwords after submission, rejects public cleartext origins, suppresses duplicate login, bounds replies and ignores late/disposed results. `account_ready` is independent of original Aries `LotReady`; no player or world entry is fabricated. Existing preview and legacy connected configurations are unchanged.

Default HTTP behavior remains404. No deployed configuration is changed. Local staging requires all of:
- `NATIVE_ACCOUNT_HTTP=staged-v1`
- Exact `NATIVE_ACCOUNT_ORIGIN`, HTTPS except loopback development
- Explicit `NATIVE_ACCOUNT_REALM` and ACCOUNTS Durable Object binding
- App configuration: `{"version":1,"mode":"native_account","gateway_url":"/native-account","native_lots":false}`

Only POST JSON requests under `/native-account/{login,roster,logout,rotate}` are admitted. Exact Origin is mandatory, cross-site Fetch Metadata is denied, request URL must match configured origin, and queries/encodings are refused. Bodies have an8KiB/5second bound; responses are no-store without CORS. Login's network throttle identity uses the Cloudflare-controlled connecting-IP header, never a body or forwarded-IP fallback. This relies on authentic Cloudflare ingress; any alternative proxy must be independently qualified before use. No deployment or ingress configuration is provided here.

## Verification

Passed on the final source:
- Pinned Rust1.99 account password WASM build with wasm-bindgen0.2.129.
- TypeScript check and production Worker bundle build.
- Original account workerd/SQLite journey and abrupt-process account retry suite, followed by the new account HTTP/provider suite (`npm run e2e:accounts`).
- Real Miniflare4.20260730.0/workerd, SQLite and compiled Rust Argon2id: distinct accounts, owned roster isolation, rotation, logout, and browser transport module journeys.
- Negative-only handler probes: incomplete valid JSON never calls authority on timeout; upstream errors return sanitized503 rather than falsely revoked401.
- Rust/WASM shell compilation, all8startup configuration tests, workspace formatting check.
- Trunk0.21.14 development build, including packaged native-account.mjs.

The rendered Chromium runner is included but did not execute its page assertions: sandbox socket restrictions prevented browser startup, and the supported escalation attempt failed during environment setup. Desktop/narrow-touch rendering, UI interruption and final release-optimized build are explicitly unqualified. No browser screenshot or visual acceptance is claimed. The actual browser transport module was tested against real local services in Node; this is not equivalent to a rendered WASM UI journey.

Reproduction after building account crypto and installing native-edge dependencies:
- `npm --prefix services/native-edge run check`
- `npm --prefix services/native-edge run build`
- `npm --prefix services/native-edge run e2e:accounts`
- `cargo test -p wonderland-web-shell --lib startup --locked`
- From apps/web-shell: `trunk build --locked`
- From services/native-edge in a browser-capable environment: `node tests/account-http-browser.mjs`. It accepts optional PLAYWRIGHT_MODULE and CHROMIUM_EXECUTABLE paths.

## Remaining gates

Review public signup/recovery UX, public-auth security/rate-limit and deployment policy; implement account recovery; recover/reconcile the later controlled admission source; prove atomic actor admission, custody and detach/travel; integrate native directory/world entry and original content; then run real two-browser gameplay, restart and device qualification. Staged roster access does not complete any of those gates. Source-preservation CI deliberately expects release verification to fail and cannot establish playable-game acceptance.
