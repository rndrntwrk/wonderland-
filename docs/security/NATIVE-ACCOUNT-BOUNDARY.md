# Native account boundary, staged profile

This is a private, realm-scoped SQLite Durable Object, using a real Rust/WASM
password kernel. It is not a public signup endpoint or a deployed login service.
The public Worker still returns 404. Only a separately admitted edge service may
hold its binding. No browser-supplied account ID or identity header grants access.

## Cryptography and session policy

- RustCrypto `argon2` 0.5.3, Argon2id v19, 19 MiB, two iterations, one lane,
  16-byte CSPRNG salt and 32-byte output. Parameters are fixed in Rust, never read
  from an untrusted request or stored PHC cost string.
- The profile follows the current OWASP Argon2id minimum. It needs independent
  security review and measured deployed CPU/concurrency budgets before release.
  https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html
- Passwords require at least 15 Unicode code points and at most 1,024 UTF-8 bytes.
  They are not silently truncated or normalized. Transient typed byte buffers are
  cleared on both sides of the WASM boundary. JavaScript strings cannot be
  reliably erased; request bodies and credentials must never enter logs.
- Each session uses 256 random bits. Only SHA-256(token) is persisted. Tokens
  expire after 12 hours or 30 minutes idle. Rotation invalidates the old token
  atomically without extending its absolute lifetime. Logout-all increments the
  account revision and invalidates every session. A lost rotation response
  requires signing in again; it never restores the old token.
- Unknown-account login performs the same Argon2 work. Invalid login reports a
  generic result. A durable one-minute window allows at most five attempts per
  username and 20 per trusted network key. Success does not reset that window.
  There is a fail-closed bounded rate-table profile and at most 16 live sessions
  per account. The edge must derive network keys, not accept them from browsers.

## Ownership and content

Account and avatar IDs are monotonic canonical decimal u64 values. Avatar head,
body, skin, gender, name and description are independent fields; head/body must
exist and agree with the privately pinned release catalog. Catalog pinning is a
trusted provisioning operation, not proof of licensing or dependency closure.
The release producer must independently qualify both before public first play.
The staged catalog is bounded to 1 MiB; larger release catalogs need a paged
provider before release. Its head/body identities must be persistent outfit keys
and gender must follow creator-collection membership, not purchasable flags.
No fixture catalog is shipped or automatically selected by production code.

Avatar creation authorizes the current session inside the same transaction that
allocates identity, enforces name uniqueness and retains the operation receipt.
A same-body retry returns exactly the original result; a changed body rejects.
All roster and avatar reads bind the authenticated account. Source policy allows
three owned avatars per account across its world directory; this is separate from
the 50-item transport page. Names retain the original ASCII-letter/space rule
without trimming. Descriptions retain the original 499-character printable-ASCII
and whitespace profile, except that DEL is deliberately refused as a control byte. The 4,096 retained
creation-receipt capacity fails closed until a separately reviewed retention
scheme exists; it is not permission to delete receipts or existing avatars.

## Source preservation and remaining work

Reference: FreeSO 4c6b3e8f5835b228723caea3c9f683c62f244f73,
`FSO.Server.Api.Core/Controllers/{RegistrationController,AuthLoginController,UserOAuthController}.cs`
and `FSO.Server/Servers/City/Handlers/RegistrationHandler.cs`.
The username casing and 3–24-character ASCII rule are retained. Legacy query-string
password delivery and browser-selected permission levels are not reproduced in
this new private transport. Existing original-service client behavior is unchanged.

Public registration and account recovery, email verification/delivery, bans and
maintenance controls, breached-password screening, HTTP origin/CSRF/cookie
policy, the account UI connection, wardrobe/game-economy initialization, native
lot admission, duplicate-controller fencing and cross-lot travel remain open.
The private account record alone cannot authorize a gameplay actor or claim a
complete player identity lifecycle. No credentials, keys, provider grants,
network deployment or user mail are created by this increment.

## Verification

The actual local workerd journey compiles and loads the static Rust WASM module,
uses SQLite across restarts and tests registration/login, independently selected
appearances, cross-account refusals, creation retry races, changed requests,
rotation races, revocation and expiry, malformed/unavailable appearance refusal,
and durable throttling. A fixed Argon2 vector is independently reproduced by
libargon2 through argon2-cffi 25.1.0 and executed in workerd; it also confirms
clearing of the mutable password byte input. These are test-only users/catalogs,
not a live-account or distribution acceptance claim.

## Source-compatible avatar identity range

Avatar creation now allocates positive IDs below 16,777,216. The original
fso_objects.sql begins object IDs at that boundary, and SqlAvatars/VMAvatar plus
Rust social logic classify identities on that distinction. External IDs remain
exact decimal strings; account, epoch and appearance-resource widths are not
reduced. Exhaustion fails in the same SQLite transaction before any avatar or
operation receipt is inserted. Existing successful create receipts still replay
at capacity after restart. The test-only driver seeds the boundary; actual
successful creation and refused competing creates execute AccountAuthority.
This does not yet claim native actor or cross-lot admission.
