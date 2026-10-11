# Swarm B native / WebAssembly execution check

This is a test-only execution check of the actual browser-safe import and content libraries, plus the actual isolated interaction query module. It uses authored fixture bytes and the existing synthetic interaction provider fixture. It requires no original game assets. The FreeSO source baseline is `4c6b3e8f5835b228723caea3c9f683c62f244f73`.

Run from the repository root:

```sh
python3 tools/swarm-b-check/parity/check.py
```

`--target-dir` is optional; the default is `../parity-target`. The driver uses the same locked standalone Cargo crate and source program for native and `wasm32-wasip1` builds. It sets `CARGO_INCREMENTAL=0`; the test crate also disables debug information and incremental compilation. Rust 1.90, the `wasm32-wasip1` target, Python 3, and Node with `node:wasi` are required. It prefers `$CARGO`, then `~/.cargo/bin/cargo`, then Cargo from `PATH`. The validated environment has Node v24.19.0 and Rust 1.90.0.

The Node runner grants an empty guest environment, argument list, and preopen map. It provides only WASI Preview 1 imports, and does not supply filesystem directories or network sockets. The host runner reads the compiled module; the guest program reads no files, requests no network operations, and prints its fixture result to stdout. Rust source forbids unsafe code; no custom unsafe WebAssembly exports are involved.

## Independent expected values and comparison

Each program first asserts literal expected values and invariants, then emits its complete result as JSON. A successful native/WASI comparison therefore supplements the fixture assertions; matching two erroneous outputs is not the only oracle. The driver normalizes both JSON results with sorted keys and fixed separators, compares every field and byte, and reports their common SHA-256 digest.

| Executed source API | Authored fixture assertion / evidence |
|---|---|
| `decompress_refpack`, `decompress_qfs` | Literal and short-backreference stream must decode exactly to `abcdabcd!?`; malformed preceding-output backreference is rejected. QFS uses its literal 9-byte header. |
| `decode_bhav`, `encode_bhav` | Version `0x8002`, locals/opcode `0x1234`, return branches `254`/`255`, operand bytes `1..8`, reserved fields, and `de ad` tail survive a byte-exact round trip. This parses a routine; it does not execute it. |
| `decode_bcon`, `encode_bcon` | Literal constants `[7,65535]`, flags `0xa5`, and opaque tail round-trip exactly. |
| `resolve_content`, real PIFF encoding/decoding/application | Exact user order `user-a,user-b` suppresses the non-user patch; resulting constant is `30`. Reversed user order produces `20` and a different effective identity. Source bytes remain unchanged, unknown bytes survive, original flags survive, and missing semiglobal BHAV does not fall through. |
| Tuning resolution and portable tuning pack | OTF, upgrade, dynamic-private, dynamic-semiglobal and global BCON inputs give selected values `[400,-1,500,-42,7,0]` and expected origins. The portable pack's complete decoded state equals the original; every encoded operand `0..65535` produces an equal lookup; their ordered JSON stream has a compared digest. Re-encoding is byte-exact. |
| `decode_animation` | Source mixed endian layout produces duration bits `0x447a0000`, distance bits `0x40200000`, translation bits `[0xbf800000,0x80000000,0x40400000]`, quaternion bits `[0,0x80000000,0x80000000,0xbf800000]`, one frame and FPS `1`. Negative zero is compared as integer bits. |
| Immutable pack / manifest | Input reversal produces identical sorted pack bytes; verified member bytes equal authored inputs. Sealed canonical metadata round-trips against its trusted digest. Selecting `animation` gives dependency-first resources `[tuning,animation]`; `unselected` remains outside the resource closure. Simulation phase selects `[tuning]`. Verified cache membership removes download requirements. Changed pack bytes and wrong manifest digest are rejected. |
| Actual `query_offers` module with existing authored provider | Offer ID `300` remains untruncated, label is `Fixture 300`, parameter is `0`, advertisements are exactly `[(2,3),(7,15)]`. Repeated calls agree, and the complete owned live snapshot remains unchanged despite detached RNG/temp/provider-byte mutations. The check provider is a synthetic fixture, not a BHAV interpreter. |

The output includes the decoded semantics, effective source/resource/patch identities and provenance, binary tuning and immutable pack bytes, canonical manifest bytes and load plans, animation bits, and the isolated query result and live fixture state. It also hashes all 65,536 tuning lookups.

The driver writes `parity-native.json` and `parity-wasi.json` in the selected target directory. On mismatch it prints the first differing field and exits nonzero. To compare saved results directly:

```sh
python3 tools/swarm-b-check/parity/check.py --compare expected.json actual.json
```

Every normal run also creates `parity-deliberately-changed.json`, changes the emitted BHAV opcode from `4660` to `0`, and invokes that same comparator in a separate process. It requires exit code `1` and first differing field `$.content.bhav.instructions[0].opcode`. This proves the comparator detects a deliberate semantic difference instead of merely testing itself for equality.

## Recorded execution

The native and Node WASI runs on 2026-10-05 passed all fixture assertions and produced identical canonical JSON: **17,150 bytes**, SHA-256 `42fe83da0cace7c343fd3736713893014187bc593cecb8694b8948c25ce15782`. The deliberately changed result was rejected with exit code `1` at `$.content.bhav.instructions[0].opcode`. Node emitted its usual experimental WASI warning.

## Scope

`wasm32-wasip1` under Node is an executable WebAssembly check, not a browser integration test. Separate `wasm32-unknown-unknown` library checks establish build compatibility for the browser target. This fixture does not establish a full VM, actual action execution, routing, animation playback, renderer, replay, networking, full original corpus, or declared FreeSO baseline parity. The interaction module is included through `wonderland-interactions-check`, a standalone test crate pointing to its actual source; shipping Swarm F integration is separate.
