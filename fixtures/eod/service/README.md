# Original service component oracle

Run from the repository root:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 fixtures/eod/service/verify.py --report fixtures/eod/service/report.json
cargo test --manifest-path crates/eod-runtime/Cargo.toml --test service_handlers
```

`verify.py` compiles **14 unchanged original C# source files** from pin
`4c6b3e8f5835b228723caea3c9f683c62f244f73` with the explicitly authored
`SourceAdapters.cs` boundary and executes `SourceOracle.cs` twice. Both runs must
match the checked-in `expected.txt` byte for byte. Eight changed-output negative
controls must be rejected. The runner checks SHA-256 and original Git blob IDs
for every input in `sources.json`; eight further files are inspection anchors.
It never rewrites the expected trace.

The compiled source set includes the original PropertySelect, Bulletin,
FNewspaper, DrawACard, CooldownEvent, Trunk, and SecureTrade handlers, their
selected original data serializers, handler/lobby/base classes, and event model.
The native implementation is exercised separately through the actual
`NativeHost`, private format 4, and checkpoint-prepared provider journal.
Selected native tests directly consume `expected.txt` literals for property
bytes, newspaper serialization, Draw persistence, cooldown tuples, and trade
private offers. See [service implementation and policy evidence](../../../docs/swarm-b/eod-services.md).

| Original trace family | What is executed and compared |
| --- | --- |
| Property | Signed register packing, name bytes, one event per remaining UTF-8 byte |
| Bulletin | Mode transitions, latest queued animation callback, one posted-state refresh |
| Newspaper | News and payout serialization, including original floating-point bits |
| DrawACard | Editor list, title/description, frequency 255 on add, exact saved bytes, save flag, empty deck, strict UTF-16 encoding failure |
| Cooldown | All eight modes in community/non-community cases, category/account selection, original local 16-byte records |
| Trunk | Male/female catalog collection selection, equip scope, Halloween skeleton fallback |
| SecureTrade | Private offer serialization, five item slots, 150-tick acceptance, source zero-money no-op, source duplicate-property bug |

The rack base/customer/owner/dresser handlers, `VMGLOutfit`, outfit scope enum,
IoBuffer extension methods, and model serializer are **inspection-only** inputs.
The rack big-endian packet and 16-bit enum widths are independently asserted in
native host tests; this fixture does not claim to execute Mina or original rack
provider callbacks.

`SourceAdapters.cs` supplies test VM identities/registers, selected content,
provider callback values and command capture. It supplies no production funds,
inventory, moderation, persistence, account directory, or application UI. The
fixture establishes the listed original component behavior under those supplied
boundaries. It does not establish full FreeSO application conformance,
production-provider durability, UI-runtime qualification, or equality between
System.Random and native SplitMix64 streams.

The trace deliberately retains source bugs where the native implementation has
an explicit correction: `m0` produces no source update; duplicate property offers
are accepted twice in the source case; truncating through a UTF-16 surrogate
causes the original strict writer to throw `EncoderFallbackException`. Native
regressions and named policies document the differing behavior instead of
editing the original trace to match it.
