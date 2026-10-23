# Native preview deployment boundary

`preview-entry.ts` is a dedicated, inactive-by-default Worker entry. It uses
Cloudflare's direct Durable Object Container API, already present in the pinned
Workers types; no Container SDK is required. Public traffic cannot select an
outbound target, bootstrap object, account realm, lot or supervisor RPC.

One configured preview ID maps to one Container DO and at most one instance.
The exact origin, lot, realm, bootstrap SHA and fixed start/expiry are retained in
DO storage before startup. Changing them for that ID rejects. The stop alarm is
persisted before paid start; every public/private request checks the window;
Container PID 1 separately exits at the fixed deadline using wall and monotonic
time. Request activity and WebSockets do not renew it. Provider alarm lateness or
billing granularity is not represented as a hard dollar cap.

Private outbound interception resolves only `owned-private.invalid`, with public
Internet disabled. It carries the existing validated RPC/account HTTP boundary
and a hash-pinned private bootstrap. Blob access is confined to the descriptor's
hash list and preview prefix. The Node stager verifies every byte and resolves
only listed relative paths into a fresh ephemeral cache. Coordinator state
selects pristine initialization or recovery; it does not reuse a local-save flag.
The existing source authority, commit/persistence/acknowledgement and replay
checks remain the final authority. No account registration runs at Container boot.

Use `wrangler.native-preview.example.jsonc` only after filling approved settings.
It is held/inactive and contains unusable placeholders. `max_instances` is one;
observability is disabled for the bounded preview. Existing DO namespaces/realms
must be deliberately mapped rather than silently replacing account authority.
Cloudflare API auth must be repaired through the approved access flow, not by
deploying bindings as a workaround for a denied R2 request.

## Local image preparation

The official default image-build path requires a running local Docker daemon
([Cloudflare prerequisites](https://developers.cloudflare.com/containers/get-started/)).
No Docker, Podman, Colima or Lima executable was found in the scoped Mac paths.
No installation, provider build or image push has occurred. A supported Linux
image build environment remains an external prerequisite; do not expose the Mac
runner publicly or activate a provider build to conceal that gap.

`tools/native-browser/owned-demo/prepare-image-context.mjs RECEIPT OUT` creates a
new code-only context from the committed Git tree and verified browser artifact
receipt. It excludes Git history, untracked files and original game bytes. The
runtime installs only integrity-pinned `ws` 8.21.0 from the existing lock; no
install scripts/optional native peers run. The Dockerfile compiles the unchanged
Rust host with Rust 1.99.0 release overflow checks, then runs Node 26 as a non-root
user. Supply reviewed, compatible Linux/amd64 Docker Official Image digests as
`RUST_IMAGE` and `NODE_IMAGE`; no floating default is supplied. This recipe has
not been image-built or qualified in Linux while Docker is unavailable.

Only after approval/build qualification, build locally with `docker build
--platform linux/amd64 --build-arg RUST_IMAGE=... --build-arg NODE_IMAGE=...` and
record the actual image digest. A named digest-pinned image goes into the
`durable_object` Container configuration. Public ASSETS use the verified browser
distribution; original browser/native resources remain private R2 inputs and
must never be baked into the image or added to public ASSETS.

## Activation gates

Keep the preview inactive until existing Workers Paid and R2 status are verified,
approved private bucket access works, additional native startup inputs/cohort are
approved, the Linux image is qualified, and expected **total** usage fits the
owner's one-hour/$1 approval. No subscription upgrade is authorized. Compute alone
is not the bill: Worker/DO requests, CPU/duration/storage, R2 operations/storage,
network and logs must be included. Shared allowances and actual cloud runtime
costs remain unknown. Do not claim these source checks establish hosted gameplay,
30 Hz performance or a hard $1 billing cap.
