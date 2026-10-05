// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, obtain one at https://mozilla.org/MPL/2.0/.
#![forbid(unsafe_code)]
use wonderland_creator::{
    city::{BmpMap, MapLayer},
    debug::{DebugSnapshot, IsolatedDebugProvider, UnsupportedDebugProvider},
    decode_hex, default_limits,
    editors::sprites::{SpritePackage, MAX_SPRITE_PACKAGE_BYTES},
    hex, json_string, sha256, Edit, ResourceDocument, ResourceGuard, ResourceOperation,
    ResourceTransaction, TuningDocument, Workspace, MAX_TRANSACTION_SPEC_BYTES, TOOL_INVENTORY,
};
use wonderland_legacy_formats::{
    iff::{ChunkKey, IffChunk},
    sprites::Spr2AlphaMode,
    ContainerFormat,
};
const HELP: &str = r#"creator — bounded offline resource inspection and editing
Usage: creator [--root DIRECTORY] COMMAND ARGUMENTS
Paths are relative to the root. Parent traversal, absolute paths and symlinks are refused.
  inspect INPUT                         JSON metadata, BHAV instructions, strings and constants
  list INPUT                            resource keys, sizes and payload SHA-256
  validate INPUT                        validate supported payloads and BHAV branches
  import INPUT OUTPUT                   bounded IFF import and exact unchanged export
  export INPUT OUTPUT                   same exact unchanged IFF round trip
  metadata INPUT OUTPUT                 write inspection JSON atomically
  extract INPUT KIND ID OUTPUT          export one raw resource payload
  edit INPUT OUTPUT KIND ID SHA VERSION OP ARGUMENTS
    bhav-branch INDEX TRUE FALSE         branch 253/254/255 are source sentinels
    bhav-operand INDEX HEX16             eight operand bytes; no opcode execution
    string SET INDEX VALUE               exact language-set index, quoted text
    tuning INDEX U16                     BCON constant
    slot INDEX X Y Z                     finite SLOT offset; retain source version
    palette INDEX R G B                  PALT RGB entry; channels are bytes, alpha stays opaque
    unknown PAYLOAD_FILE                 unknown resource replacement only
  VERSION is decimal, 0x-prefixed integer, or 'none' for BCON/unknown resources.
  SHA is the input file SHA-256 from inspect. Changed outputs have a new guard; no-ops retain it.
  add INPUT OUTPUT KIND ID SHA FLAGS LABEL_HEX PAYLOAD_FILE
  remove INPUT OUTPUT KIND ID SHA RESOURCE_SHA VERSION
  set-metadata INPUT OUTPUT KIND ID SHA RESOURCE_SHA VERSION NEW_KIND NEW_ID FLAGS LABEL_HEX
  transaction INPUT OUTPUT SPEC_JSON    atomic guarded operations from strict version 1 JSON
  New commands require exact 128-digit LABEL_HEX; RESOURCE_SHA is the payload SHA-256.
  Transactions bind all operations to one source SHA and explicit resource SHA/version guards.
  JSON payload_file paths require payload_sha256; resource-map rsmp edits are writer-managed.
  sprite-export INPUT SPR2_ID PACKAGE_JSON
  sprite-import INPUT OUTPUT PACKAGE_JSON
  sprite-pixel PACKAGE_JSON OUTPUT_JSON FRAME X Y INDEX ALPHA DEPTH
  sprite-palette PACKAGE_JSON OUTPUT_JSON PALETTE_ID INDEX R G B
  sprite-alpha-mode PACKAGE_JSON OUTPUT_JSON exact|source
  Sprite packages preserve source/frame/palette identity. DEPTH is a byte or 'none'.
  Pixel coordinates are top-left; alpha quantization requires explicit source mode.
  otf-inspect INPUT                     external OTF tuning JSON
  otf-edit INPUT OUTPUT SHA TABLE_ID KEY_ID I32_VALUE
  container-list FORMAT INPUT           FORMAT: far1a, far1b, far3, dbpf
  container-extract FORMAT INPUT INDEX OUTPUT
  city-inspect INPUT                    uncompressed Windows RGB24/RGBA32 BMP
  city-validate INPUT LAYER              requires 512x512 and exact source palette
  city-edit INPUT OUTPUT SHA LAYER X Y R G B
  city-export-ppm INPUT OUTPUT           exact RGB P6 export; alpha is omitted
  LAYER: terrain, elevation, forest-density, forest-type, road, vertex-color
  inventory                             source tools and precise disposition JSON
  debug-capabilities                    isolated provider status
  debug-step                            fails: no live or isolated VM provider installed
"#;
fn numeric(s: &str) -> Result<u64, String> {
    if let Some(hex) = s.strip_prefix("0x") {
        u64::from_str_radix(hex, 16)
    } else {
        s.parse()
    }
    .map_err(|_| format!("invalid integer: {s}"))
}
fn usize_arg(s: &str) -> Result<usize, String> {
    usize::try_from(numeric(s)?).map_err(|_| "integer overflow".into())
}
fn u16_arg(s: &str) -> Result<u16, String> {
    u16::try_from(numeric(s)?).map_err(|_| "integer must fit u16".into())
}
fn u8_arg(s: &str) -> Result<u8, String> {
    u8::try_from(numeric(s)?).map_err(|_| "integer must fit u8".into())
}
fn version_arg(s: &str) -> Result<Option<u32>, String> {
    if s == "none" {
        Ok(None)
    } else {
        Ok(Some(
            u32::try_from(numeric(s)?).map_err(|_| "version overflow")?,
        ))
    }
}
fn require(args: &[String], n: usize) -> Result<(), String> {
    if args.len() != n {
        Err(format!(
            "expected {n} arguments, got {}; run --help",
            args.len()
        ))
    } else {
        Ok(())
    }
}
fn key(kind: &str, id: &str) -> Result<ChunkKey, String> {
    let kind: [u8; 4] = kind
        .as_bytes()
        .try_into()
        .map_err(|_| "resource kind must be exactly four bytes")?;
    Ok(ChunkKey {
        kind,
        id: u16_arg(id)?,
    })
}
fn check_hash(expected: &str, bytes: &[u8]) -> Result<(), String> {
    if expected.len() != 64 || !expected.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("expected SHA-256 must contain 64 hexadecimal characters".into());
    }
    if expected.to_ascii_lowercase() != sha256(bytes) {
        Err("source SHA-256 conflict".into())
    } else {
        Ok(())
    }
}
fn guarded_publish(
    ws: &Workspace,
    input: &str,
    expected: &str,
    output: &str,
    bytes: &[u8],
) -> Result<(), String> {
    check_hash(expected, &ws.read(input)?)?;
    ws.write_atomic(output, bytes)?;
    Ok(())
}
fn read_sprite_package(
    ws: &Workspace,
    path: &str,
    limits: &wonderland_legacy_formats::Limits,
) -> Result<(SpritePackage, String), String> {
    let bytes = ws.read_limited(
        path,
        MAX_SPRITE_PACKAGE_BYTES
            .min(limits.max_input_bytes)
            .min(limits.max_resource_bytes),
    )?;
    let hash = sha256(&bytes);
    let package = SpritePackage::from_json(&bytes, limits)?;
    Ok((package, hash))
}
fn publish_sprite_package(
    ws: &Workspace,
    input: &str,
    expected: &str,
    output: &str,
    package: &SpritePackage,
    limits: &wonderland_legacy_formats::Limits,
) -> Result<(), String> {
    let bytes = package.to_json(limits)?;
    check_hash(expected, &ws.read_limited(input, MAX_SPRITE_PACKAGE_BYTES)?)?;
    ws.write_atomic(output, bytes.as_bytes())?;
    println!("wrote {} bytes to {output}", bytes.len());
    Ok(())
}
fn format(s: &str) -> Result<ContainerFormat, String> {
    match s {
        "far1a" => Ok(ContainerFormat::Far1a),
        "far1b" => Ok(ContainerFormat::Far1b),
        "far3" => Ok(ContainerFormat::Far3),
        "dbpf" => Ok(ContainerFormat::Dbpf),
        _ => Err("unknown container format".into()),
    }
}
fn run() -> Result<(), String> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "--help" || args[0] == "help" {
        print!("{HELP}");
        return Ok(());
    }
    let root = if args[0] == "--root" {
        if args.len() < 3 {
            return Err("--root requires a directory and command".into());
        }
        let r = args.remove(1);
        args.remove(0);
        r
    } else {
        ".".into()
    };
    let command = args.remove(0);
    let ws = Workspace::new(root)?;
    let limits = default_limits();
    match command.as_str() {
        "inventory" => {
            require(&args, 0)?;
            print!("{TOOL_INVENTORY}");
        }
        "debug-capabilities" => {
            require(&args, 0)?;
            println!("{{\"provider\":null,\"isolated_snapshots\":\"adapter-only\",\"watch\":false,\"trace\":false,\"live_step\":false,\"isolated_step\":false}}");
        }
        "debug-step" => {
            require(&args, 0)?;
            UnsupportedDebugProvider.step_isolated(&DebugSnapshot {
                tick: 0,
                bytes: vec![],
            })?;
        }
        "inspect" | "list" | "validate" => {
            require(&args, 1)?;
            let bytes = ws.read(&args[0])?;
            let doc = ResourceDocument::import(&bytes, &limits)?;
            if command == "inspect" {
                print!("{}", doc.metadata_json(&limits)?);
            } else if command == "list" {
                for c in &doc.file().chunks {
                    println!(
                        "{}\t{}\t{}\t{}",
                        json_string(&String::from_utf8_lossy(&c.key.kind)),
                        c.key.id,
                        c.data.len(),
                        sha256(&c.data)
                    );
                }
            } else {
                doc.validate(&limits)?;
                println!("valid: {} resources; supported payloads and BHAV branch targets checked; unknown payloads retained raw",doc.file().chunks.len());
            }
        }
        "import" | "export" | "metadata" => {
            require(&args, 2)?;
            let bytes = ws.read(&args[0])?;
            let doc = ResourceDocument::import(&bytes, &limits)?;
            let output = if command == "metadata" {
                doc.metadata_json(&limits)?.into_bytes()
            } else {
                doc.export(&limits)?
            };
            guarded_publish(&ws, &args[0], &sha256(&bytes), &args[1], &output)?;
            println!("wrote {} bytes to {}", output.len(), args[1]);
        }
        "extract" => {
            require(&args, 4)?;
            let bytes = ws.read(&args[0])?;
            let doc = ResourceDocument::import(&bytes, &limits)?;
            let chunk = doc.chunk(key(&args[1], &args[2])?)?;
            guarded_publish(&ws, &args[0], &sha256(&bytes), &args[3], &chunk.data)?;
            println!("extracted {} bytes", chunk.data.len());
        }
        "sprite-export" => {
            require(&args, 3)?;
            let bytes = ws.read(&args[0])?;
            let expected = sha256(&bytes);
            let doc = ResourceDocument::import(&bytes, &limits)?;
            drop(bytes);
            let package = doc.export_sprite(u16_arg(&args[1])?, &limits)?;
            let output = package.to_json(&limits)?;
            guarded_publish(&ws, &args[0], &expected, &args[2], output.as_bytes())?;
            println!("wrote {} bytes to {}", output.len(), args[2]);
        }
        "sprite-import" => {
            require(&args, 3)?;
            let bytes = ws.read(&args[0])?;
            let mut doc = ResourceDocument::import(&bytes, &limits)?;
            drop(bytes);
            let (package, package_hash) = read_sprite_package(&ws, &args[2], &limits)?;
            let report = doc.import_sprite(&package, &limits)?;
            check_hash(
                &package_hash,
                &ws.read_limited(&args[2], MAX_SPRITE_PACKAGE_BYTES)?,
            )?;
            let output = doc.export(&limits)?;
            guarded_publish(&ws, &args[0], package.source_sha256(), &args[1], &output)?;
            println!(
                "{}",
                serde_json::to_string(&report).map_err(|error| error.to_string())?
            );
        }
        "sprite-pixel" => {
            require(&args, 8)?;
            let (mut package, expected) = read_sprite_package(&ws, &args[0], &limits)?;
            let depth = if args[7] == "none" {
                None
            } else {
                Some(u8_arg(&args[7])?)
            };
            package.set_pixel(
                usize_arg(&args[2])?,
                [usize_arg(&args[3])?, usize_arg(&args[4])?],
                u8_arg(&args[5])?,
                u8_arg(&args[6])?,
                depth,
                &limits,
            )?;
            publish_sprite_package(&ws, &args[0], &expected, &args[1], &package, &limits)?;
        }
        "sprite-palette" => {
            require(&args, 7)?;
            let (mut package, expected) = read_sprite_package(&ws, &args[0], &limits)?;
            package.set_palette_color(
                u16_arg(&args[2])?,
                usize_arg(&args[3])?,
                [u8_arg(&args[4])?, u8_arg(&args[5])?, u8_arg(&args[6])?],
            )?;
            publish_sprite_package(&ws, &args[0], &expected, &args[1], &package, &limits)?;
        }
        "sprite-alpha-mode" => {
            require(&args, 3)?;
            let (mut package, expected) = read_sprite_package(&ws, &args[0], &limits)?;
            let mode = match args[2].as_str() {
                "exact" => Spr2AlphaMode::Exact,
                "source" => Spr2AlphaMode::QuantizeLikeSource,
                _ => return Err("sprite alpha mode must be exact or source".into()),
            };
            package.set_alpha_mode(mode)?;
            publish_sprite_package(&ws, &args[0], &expected, &args[1], &package, &limits)?;
        }
        "edit" => {
            if args.len() < 8 {
                return Err("edit requires INPUT OUTPUT KIND ID SHA VERSION OP ARGUMENTS".into());
            }
            let bytes = ws.read(&args[0])?;
            check_hash(&args[4], &bytes)?;
            let mut doc = ResourceDocument::import(&bytes, &limits)?;
            let key = key(&args[2], &args[3])?;
            let mut guard = doc.guard(key, &limits)?;
            guard.source_hash = args[4].to_ascii_lowercase();
            guard.format_version = version_arg(&args[5])?;
            let p = &args[7..];
            let edit = match args[6].as_str() {
                "bhav-branch" => {
                    require(p, 3)?;
                    Edit::BhavBranch {
                        instruction: usize_arg(&p[0])?,
                        true_pointer: u8_arg(&p[1])?,
                        false_pointer: u8_arg(&p[2])?,
                    }
                }
                "bhav-operand" => {
                    require(p, 2)?;
                    if p[1].len() != 16 || !p[1].is_ascii() {
                        return Err("operand must contain exactly 16 hexadecimal characters".into());
                    }
                    let mut operand = [0; 8];
                    for (i, b) in operand.iter_mut().enumerate() {
                        *b = u8::from_str_radix(&p[1][i * 2..i * 2 + 2], 16)
                            .map_err(|_| "invalid operand hex")?;
                    }
                    Edit::BhavOperand {
                        instruction: usize_arg(&p[0])?,
                        operand,
                    }
                }
                "string" => {
                    require(p, 3)?;
                    Edit::StringValue {
                        set: usize_arg(&p[0])?,
                        index: usize_arg(&p[1])?,
                        value: p[2].clone(),
                    }
                }
                "tuning" => {
                    require(p, 2)?;
                    Edit::TuningConstant {
                        index: usize_arg(&p[0])?,
                        value: u16_arg(&p[1])?,
                    }
                }
                "slot" => {
                    require(p, 4)?;
                    let mut offset = [0f32; 3];
                    for i in 0..3 {
                        offset[i] = p[i + 1].parse().map_err(|_| "invalid slot offset")?;
                    }
                    Edit::SlotOffset {
                        index: usize_arg(&p[0])?,
                        offset,
                    }
                }
                "palette" => {
                    require(p, 4)?;
                    Edit::PaletteColor {
                        index: usize_arg(&p[0])?,
                        rgb: [u8_arg(&p[1])?, u8_arg(&p[2])?, u8_arg(&p[3])?],
                    }
                }
                "unknown" => {
                    require(p, 1)?;
                    Edit::UnknownBytes(ws.read_limited(&p[0], limits.max_resource_bytes)?)
                }
                _ => return Err("unknown edit operation".into()),
            };
            doc.edit(key, &guard, edit, &limits)?;
            let out = doc.export(&limits)?;
            guarded_publish(&ws, &args[0], &args[4], &args[1], &out)?;
            println!("wrote {} bytes; SHA-256 {}", out.len(), sha256(&out));
        }
        "add" | "remove" | "set-metadata" | "transaction" => {
            require(
                &args,
                match command.as_str() {
                    "add" => 8,
                    "remove" => 7,
                    "set-metadata" => 11,
                    _ => 3,
                },
            )?;
            let bytes = ws.read(&args[0])?;
            let mut doc = ResourceDocument::import(&bytes, &limits)?;
            let transaction = if command == "transaction" {
                let spec = ws.read_limited(&args[2], MAX_TRANSACTION_SPEC_BYTES)?;
                ResourceTransaction::from_json(&spec, &ws, &limits)?
            } else {
                check_hash(&args[4], &bytes)?;
                let resource_key = key(&args[2], &args[3])?;
                let operation = if command == "add" {
                    ResourceOperation::Add {
                        chunk: IffChunk {
                            key: resource_key,
                            flags: u16_arg(&args[5])?,
                            label: decode_hex(&args[6], "resource label")?,
                            data: ws.read_limited(&args[7], limits.max_resource_bytes)?,
                        },
                    }
                } else {
                    let expected = ResourceGuard {
                        resource_hash: args[5].clone(),
                        format_version: version_arg(&args[6])?,
                    };
                    if command == "remove" {
                        ResourceOperation::Remove {
                            key: resource_key,
                            expected,
                        }
                    } else {
                        ResourceOperation::SetMetadata {
                            key: resource_key,
                            expected,
                            new_key: key(&args[7], &args[8])?,
                            flags: u16_arg(&args[9])?,
                            label: decode_hex(&args[10], "resource label")?,
                        }
                    }
                };
                ResourceTransaction {
                    source_hash: args[4].clone(),
                    operations: vec![operation],
                }
            };
            doc.transact(&transaction, &limits)?;
            let out = doc.export(&limits)?;
            guarded_publish(&ws, &args[0], &transaction.source_hash, &args[1], &out)?;
            println!(
                "wrote {} bytes; {} operations; SHA-256 {}",
                out.len(),
                transaction.operations.len(),
                sha256(&out)
            );
        }
        "otf-inspect" => {
            require(&args, 1)?;
            let tuning = TuningDocument::import(&ws.read(&args[0])?, &limits)?;
            print!("{}", tuning.metadata_json(&limits)?);
        }
        "otf-edit" => {
            require(&args, 6)?;
            let bytes = ws.read(&args[0])?;
            check_hash(&args[2], &bytes)?;
            let mut tuning = TuningDocument::import(&bytes, &limits)?;
            let table = args[3]
                .parse::<i32>()
                .map_err(|_| "invalid signed OTF table ID")?;
            let key = args[4]
                .parse::<i32>()
                .map_err(|_| "invalid signed OTF key ID")?;
            let value = args[5]
                .parse::<i32>()
                .map_err(|_| "invalid signed OTF constant")?;
            tuning.edit_constant(&args[2], table, key, value, &limits)?;
            let out = tuning.export(&limits)?;
            guarded_publish(&ws, &args[0], &args[2], &args[1], &out)?;
            println!("wrote OTF constant edit; SHA-256 {}", sha256(&out));
        }
        "container-list" | "container-extract" => {
            require(&args, if command == "container-list" { 2 } else { 4 })?;
            let bytes = ws.read(&args[1])?;
            let index = wonderland_legacy_formats::index(&bytes, format(&args[0])?, &limits)
                .map_err(|e| e.to_string())?;
            if command == "container-list" {
                for (i, e) in index.entries().iter().enumerate() {
                    println!("{i}\t{e:?}");
                }
            } else {
                let out = index
                    .extract(usize_arg(&args[2])?, &limits)
                    .map_err(|e| e.to_string())?;
                guarded_publish(&ws, &args[1], &sha256(&bytes), &args[3], &out)?;
                println!("extracted {} bytes", out.len());
            }
        }
        "city-inspect" | "city-validate" | "city-export-ppm" => {
            require(&args, if command == "city-inspect" { 1 } else { 2 })?;
            let bytes = ws.read(&args[0])?;
            let map = BmpMap::decode(&bytes, limits.max_pixels)?;
            if command == "city-inspect" {
                println!("{{\"format\":\"uncompressed BMP\",\"width\":{},\"height\":{},\"source_sha256\":{},\"first_rgb_hex\":{}}}",map.width(),map.height(),json_string(&sha256(&bytes)),json_string(&hex(&map.pixel(0,0)?)));
            } else if command == "city-validate" {
                map.validate(MapLayer::parse(&args[1])?, true)?;
                println!("valid: 512x512 city data map; exact palette checked");
            } else {
                guarded_publish(&ws, &args[0], &sha256(&bytes), &args[1], &map.ppm())?;
                println!("exported exact RGB P6 PPM; alpha omitted");
            }
        }
        "city-edit" => {
            require(&args, 9)?;
            let bytes = ws.read(&args[0])?;
            check_hash(&args[2], &bytes)?;
            let layer = MapLayer::parse(&args[3])?;
            let mut map = BmpMap::decode(&bytes, limits.max_pixels)?;
            map.validate(layer, true)?;
            map.set_pixel(
                usize_arg(&args[4])?,
                usize_arg(&args[5])?,
                [u8_arg(&args[6])?, u8_arg(&args[7])?, u8_arg(&args[8])?],
                layer,
            )?;
            guarded_publish(&ws, &args[0], &args[2], &args[1], &map.bytes())?;
            println!("wrote pixel edit; SHA-256 {}", sha256(&map.bytes()));
        }
        _ => return Err(format!("unknown command {command}; run --help")),
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("creator: {error}");
        std::process::exit(2);
    }
}
