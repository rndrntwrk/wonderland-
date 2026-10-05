# Swarm B verification record

The final assembled implementation passed **35 verification gates** on
2026-10-05: **219 Rust tests, 8 Python catalog tests, formatting and strict
Clippy checks, native/browser-target builds, deterministic cooking, pinned
inventories, native/WASM execution, and a focused original C# reader check**.
Three of the Rust tests are compile-fail documentation tests protecting API
boundaries. One optional semantic corpus test is intentionally ignored by the
default Rust suite; the separate pinned content census was executed and its
complete output compared byte-for-byte with the checked-in metadata.

The source baseline is
[`4c6b3e8f5835b228723caea3c9f683c62f244f73`](https://github.com/rndrntwrk/wonderland-/tree/4c6b3e8f5835b228723caea3c9f683c62f244f73).
The machine-readable result is [verification.json](verification.json).
The code, locks, authored fixtures and compatibility metadata remained stable
throughout the successful run. Their ordered verification-input SHA-256 is
`72093b406ebd1d67b160b529758cfa045aa0f2631a25c2defbe904f9fa00d189`.

## Reproduce

Run from the repository root with Rust/Cargo 1.90.0, rustfmt, Clippy, Python 3,
and the `wasm32-unknown-unknown` target installed. The optional execution check
also requires `wasm32-wasip1` and Node with `node:wasi`; the recorded run used
Node 24.19.0. The original-reader option requires Mono's `mcs` and `mono`.

```sh
rustup toolchain install 1.90.0 --component rustfmt --component clippy
rustup target add --toolchain 1.90.0 wasm32-unknown-unknown wasm32-wasip1
bash tools/swarm-b/verify.sh --with-parity --with-source-oracle
```

The runner selects Rust 1.90.0, uses the standalone package lockfiles, and
allocates a fresh temporary target directory for each package. It disables
incremental compilation and debug information to keep the verification bounded
on disk. It removes only its own temporary build directories. Logs and
`verification.json` remain in the printed output directory; use `--logs-dir`
to choose that directory and `--cargo` to select a Cargo executable.

Omitting the two optional flags still runs every native suite, formatting,
Clippy, browser library checks, the EOD native-only boundary, both inventories,
catalog tests and the cooker demonstration. No original installation assets
are downloaded or copied into authored fixtures. The source census and C# check
read the existing pinned repository content.

## Native suites

| Package or actual-module harness | Passed | Ignored | Principal coverage |
| --- | ---: | ---: | --- |
| `crates/legacy-formats` | 64 | 1 | Container framing, RefPack/QFS, semantic round trips, hostile limits, sprites, slots, Vitaboy and audio metadata |
| `crates/content-ir` | 14 | 0 | Ordered patching, scopes, provenance, tuning/localization, aggregate retention and portable tuning payloads |
| `tools/swarm-b-check/manifest` | 7 | 0 | Canonical identities, strict metadata, dependency closure, cycles, cache identity and distribution decisions |
| `tools/swarm-b-check/interactions` | 55 | 0 | Actual offer/query/permission/intent/queue module with explicitly synthetic providers |
| `crates/eod-runtime` | 37 | 0 | Native private host, Timer, scope and replay guards, recipient isolation, checkpoints/effects and one forbidden-conversion doctest |
| `tools/creator` | 19 | 0 | Offline resources, guarded edits, workspace handling, actual CLI flow, BMP edits and two private-dimension doctests |
| `tools/asset-cooker` | 23 | 0 | Immutable packs, actual imports/resolution, provenance, dependency preservation, atomic publication and real CLI commands |
| **Rust total** | **219** | **1** | |
| `tests/compat/catalog` | **8** | **0** | Pinned denominator, aliases, source identities, open-leaf expansion, stale artifacts and bounded input |

Every listed Rust package passed `cargo fmt -- --check` and
`cargo clippy --locked --all-targets --no-deps -- -D warnings` using its own
manifest. The parity fixture crate also passed its focused formatting and
strict Clippy checks before the final execution run.

The `legacy-formats`, `content-ir` and actual interaction module built for
`wasm32-unknown-unknown`. Compiling the authoritative EOD library for that target
was deliberately rejected with `authoritative private EOD state is native-only`;
the runner requires that specific rejection.

## Executed comparisons and complete workflows

### Native and WebAssembly

The same authored Rust fixture program executed natively and as
`wasm32-wasip1` under Node WASI. Each run first asserted hand-expected values.
The driver then compared every output field and all **17,150 canonical JSON
bytes**. Both results had SHA-256
`42fe83da0cace7c343fd3736713893014187bc593cecb8694b8948c25ce15782`.

This covers real decompression, semantic round trips, ordered PIFF resolution,
effective identities, source float bits and negative zero, immutable
packs/manifests/load plans, and the actual isolated UI query code. Portable
tuning is checked against the original resolved state for **all 65,536 encoded
operands**. The driver also changes a BHAV opcode from `4660` to `0` and requires
the comparator to exit `1` at `$.content.bhav.instructions[0].opcode`.

See [parity.md](parity.md) for the precise fixture assertions. Node WASI execution
does not establish browser integration, a real BHAV interpreter, gameplay,
rendering or full replay parity.

### Cooker

The runner executes `demo`, cooks the same authored inputs again with `--public`,
verifies the second release against the first manifest's trusted SHA-256, and
compares every manifest and pack byte. Both runs produce **4 resources in 2
packs**. The simulation closure selects **3 resources in 1 pack, 655 bytes**.

The manifest SHA-256 is
`5ea34ee7ebda50e70f6adb77d14c82d08161ecd8442e58858a2f5ec2b6b24b66`.
The source fixture is explicitly authored and redistributable. Its BHAV tests
the pipeline and dependency selection; it is not a playable object.

### Original FAR3 reader

`tools/swarm-b/far3-oracle.py` first checks four original C# reader files against
the pinned commit, then compiles those unchanged files with a small harness.
The **73-byte authored FAR3 fixture** decodes to exactly `abcdefg`. The Rust
container regression uses the same bytes. This confirms that the inner QFS
length counts command bytes after its nine-byte header. The unchanged C# files
emit five pre-existing compiler warnings; compilation and the comparison pass.

### Pinned inventories

The content census is regenerated from `git ls-tree`/`git cat-file` at the
pinned commit and compared in full. Its SHA-256 is
`bd2305362328df467aa0c27449dab7dd44eaef21dad5a1da76f29da0ec353c76`.
It records 745 IFF/PIFF paths, five XML tuning files and one excluded font.
The mechanically generated census uses compact JSON to keep its transfer size
bounded. Its decoded records were compared against the original formatted
output and are identical; only whitespace and the resulting byte identity changed.
The object generator validates **730 source-hash cohorts**, **1,327 OBJD rows**
and **1,602 open leaves**. The EOD generator checks **30 server registrations**
and **28 UI mappings**. Source aliases and excluded layouts remain explicit.

All enter/use/cancel/leave/save/reconnect object scenarios remain unverified.
Only Timer is runtime-enabled; the other 29 EOD implementations and original
runtime qualification remain open. See [object-coverage.md](object-coverage.md)
and [eod-coverage.md](eod-coverage.md).

## Independent review corrections

Every confirmed implementation finding was corrected and independently
rechecked before the final assembled run.

| Finding | Correction and verified behavior |
| --- | --- |
| FAR3 compressed command length included the QFS header | Match the unchanged source reader's command-body length; literal Rust and C# vectors agree. |
| Several semantic resources could exceed one retained-memory budget | Share the remaining resolver budget across resources; a 4,888-byte hostile fixture rejects under 8,192 bytes while sufficient allowance retains all 24 resources and raw payloads. |
| String vectors could grow geometrically past the charged capacity | Allocate the exact recorded count; the 33-string regression retains capacity 33 within its 3,000-byte allowance. |
| XML escaping could allocate replacements before the cumulative limit | Preflight every escaped replacement and the complete edited output before allocation; sufficient allowance preserves valid XML and edited values. |
| Tuning maps were charged after insertion | Check conservative per-entry allocation before insertion; restricted allowance rejects and the normal 120-entry case succeeds. |
| UI checks shared temporary arrays across detached checks | Restore both arrays from the incoming baseline before each out-of-tick check; preserve the source's separate in-tick accumulation. |
| EOD tickets lacked a host-scope domain | Bind tickets and inbound wire data to scope, with scope checks for commands, deliveries, disconnect, rebind and checkpoint restore. |
| A creator temporary filename could equal the requested output | Exclude the target before creating the temporary file; the deterministic collision regression keeps the final path absent during the write. |
| Public BMP dimensions could invalidate pixel indexing | Keep dimensions private and expose getters; compile-fail tests reject direct mutation. |
| Release publication could commit a manifest with a missing pack | Require exact equality of supplied and declared pack identities before creating the release directory. |

The interaction review also documents three intentional bounded deviations:
visit each original TS1 cancellation-tail item once when the source can repeat
one forever; reject ParentIdle cancellation that would delete an active
descendant; and preserve the active prefix after a refused immediate frame
start. The real scheduler must supply the documented fault/unwind or atomic
start handling.

## Qualification limits

These results establish the delivered component behavior and authored fixture
agreement. Complete W01/W06/W16 acceptance still needs W00 contract adoption,
the full licensed installation and patch manifest, a real simulation/scheduler
and routing provider, renderer/audio/browser integration, production private
transport and durable effect storage, remaining EOD handlers, and original
runtime traces. The [package status table](README.md#package-status-and-remaining-acceptance-gates)
and [cross-swarm handoff](README.md#handoff-to-the-other-swarms) identify those
dependencies explicitly.
