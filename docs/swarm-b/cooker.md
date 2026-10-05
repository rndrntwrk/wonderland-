# Asset cooker: explicit source import and portable release packs

The local `wonderland-asset-cooker` tool imports content through the real bounded legacy readers and content resolver. It does not require a complete installation, execute BHAV, use HTTP, discover providers, convert meshes, or construct gameplay scenes. All source behavior is tied to FreeSO commit `4c6b3e8f5835b228723caea3c9f683c62f244f73`; format implementations carry their source references in `crates/legacy-formats/src/` and `crates/content-ir/src/`.

Build and run from the repository root:

```sh
CARGO_INCREMENTAL=0 cargo run --manifest-path tools/asset-cooker/Cargo.toml -- demo /tmp/authored-content-demo
cargo run --manifest-path tools/asset-cooker/Cargo.toml -- inspect fixtures/packs/authored.iff iff fixture
cargo run --manifest-path tools/asset-cooker/Cargo.toml -- cook fixtures/packs/demo.json /tmp/cooked-content --public
cargo run --manifest-path tools/asset-cooker/Cargo.toml -- verify /tmp/cooked-content
cargo run --manifest-path tools/asset-cooker/Cargo.toml -- plan /tmp/cooked-content/manifest.json fixture/chunk-42484156-1000 --simulation
```

`demo` creates entirely authored CC0-1.0 sources, runs the same import/cook/verify pipeline and emits a dependency-first readiness plan. It is a content pipeline fixture, not playable simulation. Repeating it under different new directories yields identical manifest and pack hashes.

## Commands

| Command | Behavior |
| --- | --- |
| `cook <spec.json> <new-dir> [--public]` | Resolve and validate everything before creating a release. Refuse an existing destination. Write digest-named `.wlp` files, `import-report.json`, and canonical `manifest.json` last. Public mode requires explicit allowed redistribution and a nonempty license for every resource. |
| `verify <dir> [--manifest-sha256 <digest>]` | Read bounded manifest, optionally require a trusted release digest, validate manifest/pack versions, every pack and member hash, resource ownership and exact bytes, then each explicit resource codec. Semantic, tuning, standalone Vitaboy and audio codecs are decoded; IFF visual members receive envelope integrity validation here. Source-palette sprite decoding happens during import; verify does not reconstruct palettes across release pack members. Print only counts and manifest digest. |
| `plan <manifest.json> <id>... [--simulation] [--variant <name>]` | Validate the full manifest and graph; compute exact dependency closure, deterministic dependency-first resources, unique required packs and download bytes. Simulation mode selects declared critical resources. |
| `inspect <file> <format> [source-id]` | Run the actual importer and print its mapping of exact canonical logical IDs, original file/key/chunk identities and classification. Default source ID is `inspect`; pass the intended source ID to obtain override IDs for that spec. |
| `demo <new-dir>` | Write authored IFF and spec, public cook a nested release, verify it and plan the declared semantic closure. No execution occurs. |

A manifest supplied without an expected release hash proves internal consistency. Use `verify --manifest-sha256` with a digest selected independently by the trusted release to bind verification to that release.

## Strict CookSpec version 1

`fixtures/packs/demo.json` is a complete example. All schema objects reject unknown fields. `schema_version` must be `1`; `source_baseline` must equal the pinned full source commit; `tuning_version` is a 64-character lowercase SHA-256 digest. `sources` is an ordered array, and `overrides` is an array of unique exact resource IDs. Duplicate sources or overrides, unknown overrides, missing dependencies, cycles, malformed recognized semantic data and unsupported versions fail.

Optional `semiglobal` and `global` objects carry safe relative IFF `path` and exact `source_name`. Optional `tuning` carries `otf` and `otf_rewrite` relative XML file paths, ordered `upgrades` (`table`, `index`, signed `value`) and ordered `dynamic_private`/`dynamic_semiglobal` (`table`, `index`, IEEE float `value_bits`). These files are actually parsed and all inputs feed the content resolver. Optional `resolver_variant` (empty for standard or `tsbo` for the explicit TSBO TTAB codec) and `locale_selection` (`requested`, `default_language`) feed the effective resolver identity.

Each source requires `id`, safe relative `path`, explicit `format`, `pack_group` and `provenance` (`origin`, nullable `license`, `redistribution`). `source_name` is optional; its default is the original basename with casing intact. Ordered `patches` contain relative `path` and `is_user`. Patches are accepted only on an explicitly named IFF source. Exact PIFF source-name matching and user-patch suppression follow the content resolver. Suppressed patches remain separately reported and never enter applied patch hashes.

Overrides require `id`, `dependencies` and `simulation_critical`; optional `locale` and `variants` record consumer selection metadata. Imports default to noncritical. Marking a critical BHAV implicitly asserts the caller's accepted dependency completeness contract: the cooker validates the explicitly referenced graph, but does not claim to discover dynamic routine references, global/semiglobal scopes or all provider dependencies. Every dependency of a critical resource must also be declared critical. Opaque resources and PIFF descriptors cannot become critical. Locale metadata retains source multilingual bytes; variant metadata does not imply a conversion or renderer implementation.

## Formats and identities

Accepted explicit formats are `iff`, `far1a`, `far1b`, `far3`, `dbpf`, `vitaboy_animation`, `vitaboy_skeleton`, `vitaboy_mesh`, `vitaboy_binding`, `vitaboy_appearance`, `vitaboy_outfit`, `pcm_wave`, `xa_metadata`, `utk_metadata`, `hit_track_metadata`, `hit_events_metadata`, `hit_hsm_metadata`, `hit_bytecode_metadata`, `hitlist_versioned`, `hitlist_counted`, `hitlist_pascal_ranges` and `opaque`.

Each resolved IFF also emits `<source-id>/resolved-tuning` using the bounded portable `ResolvedTuning` binary codec. Actual BCON, selected OTF rewrite, upgrades and dynamic values are retained with source-specific precedence; tuned bytes can be consumed without filesystem/provider lookup. Scope chunks use `<source-id>/semiglobal/` and `<source-id>/global/` prefixes, and retain their actual original file hashes. They and the tuning resource remain noncritical until explicitly declared in overrides.

IFF resources use `<source-id>/chunk-<FourCC-bytes-as-hex>-<id-as-four-hex-digits>`. Unknown raw chunks survive with exact kind, ID, label, flags and data. Each cooked member is a one-chunk IFF with the original 64-byte header except the source whole-file resource-map pointer is neutralized for this declared derivative. Indexed IFF sources currently fail explicitly in the resolver; rebuilding resource maps is unsupported.

FAR1 IDs preserve every filename byte as hex: `<source-id>/far1-<hex-name>`. FAR3 IDs retain both 32-bit identity components: `<source-id>/far3-<type>-<file>`. DBPF IDs retain all three: `<source-id>/dbpf-<type>-<group>-<instance>`. Archive entries with IFF signatures are resolved and split into chunks by appending the chunk key. Other entries remain opaque. Entry names never become filesystem paths. There is no nested archive expansion or extension-based decoder guessing. Standard DBPF indexing is exposed; the separate source-compatible nonstandard DBPF layouts are not implicitly enabled.

Recognized semantic classification requires successful real semantic decoding. PALT and DGRP are decoded; SPR/SPR2 validation uses palettes present in the resolved source. Missing required palettes fail explicitly without invented colors. Cooked sprite members preserve raw source bytes; validation does not produce renderable image conversions. Known static SPR/SPR2 palette dependencies are derived from the decoded source. DGRP links an existing unambiguous source SPR/SPR2; missing or ambiguous targets fail explicitly. Overrides add dependencies and cannot remove these required edges. Variant declarations and dynamic BHAV references remain explicit. Standalone Vitaboy formats are validated using their actual binary readers. Animation is semantic metadata; skeleton, mesh, binding, appearance and outfit are visual data, without full mesh conversion. `pcm_wave` validates PCM wave data. XA/UTK/HIT inputs validate encoded metadata and remain encoded: this is not audio decoding or a playback engine. All audio is noncritical.

## Provenance and bounds

Manifest provenance stores the SHA-256 of the actual original source file bytes, ordered hashes of actual applied patch files, and the actual effective tuning identity. The manifest tuning version records the explicit release tuning epoch. For archive chunks, `source_hash` identifies the original archive, while the report includes the extracted IFF byte hash and effective resolver identity. Each source's license/redistribution claim is supplied by the operator; public mode validates presence and permission rather than adjudicating its legal truth.

Input JSON, file lengths, source/patch/override counts, resource bytes, aggregate source/decoded bytes, pack bytes, manifest bytes and planning downloads are bounded. Unsafe paths, symlink components and nonregular files are refused. Bounded reads inspect file metadata before reading, then cap reads to catch growth. Output filenames derive only from verified SHA-256 digests. Existing destinations are never removed or overwritten; failures during newly owned output writes are cleaned up, and the manifest is written last. This is a local operator CLI, not a hostile concurrent upload service: path checks are not a kernel-enforced race-proof filesystem sandbox. Interrupted writes can leave an incomplete directory without a completed manifest; operators should remove such directories explicitly before retrying.

Tests in `tools/asset-cooker/tests/import.rs` and `cli.rs` exercise real source imports, ordered PIFF precedence, labels/flags/opaque bytes, explicit critical closure, malformed data, reproducibility, release-digest mismatch, pack corruption, subset plans, unknown overrides, permissions and path/symlink refusal. The authored corpus is tiny and cannot establish broad installation/provider completeness.
