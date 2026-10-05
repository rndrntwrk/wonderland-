// SPDX-License-Identifier: MPL-2.0
use wonderland_asset_cooker::interchange::obj::ObjModel;
use wonderland_creator::{
    city::{CityImage, MapLayer, NeighborhoodDocument},
    editors::{
        assets::{AssetDocument, AssetKind},
        upgrades::UpgradeDocument,
    },
    json_support::{self, JsonEdit, MAX_EDITOR_JSON_BYTES},
    sha256, Workspace,
};
use wonderland_creator::{
    editors::{gltf::GltfPackage, meshes::MeshOverrideDocument},
    patch_view::{PatchInput, PatchView},
};
use wonderland_legacy_formats::Limits;
use wonderland_legacy_formats::{reconstruction, vitaboy};
fn require(args: &[String], count: usize) -> Result<(), String> {
    if args.len() != count {
        Err(format!("expected {count} arguments; run --help"))
    } else {
        Ok(())
    }
}
fn integer<T: std::str::FromStr>(s: &str) -> Result<T, String> {
    s.parse().map_err(|_| format!("invalid integer {s}"))
}
fn guard(expected: &str, bytes: &[u8]) -> Result<(), String> {
    if expected.to_ascii_lowercase() != sha256(bytes) {
        Err("source SHA-256 conflict".into())
    } else {
        Ok(())
    }
}
fn publish(
    ws: &Workspace,
    input: &str,
    output: &str,
    expected: &str,
    bytes: &[u8],
) -> Result<(), String> {
    guard(expected, &ws.read(input)?)?;
    ws.write_atomic(output, bytes)?;
    println!("wrote {} bytes; SHA-256 {}", bytes.len(), sha256(bytes));
    Ok(())
}
fn edits(ws: &Workspace, path: &str, limits: &Limits) -> Result<Vec<JsonEdit>, String> {
    let bytes = ws.read_limited(path, MAX_EDITOR_JSON_BYTES)?;
    let tree = json_support::parse(&bytes, limits)?;
    let entries = tree
        .as_array()
        .ok_or("edit specification must be an array")?;
    if entries.iter().any(|v| !v.is_object()) {
        return Err("each edit must be an object".into());
    }
    serde_json::from_value(tree).map_err(|e| e.to_string())
}
fn image_output(map: &CityImage, format: &str, limits: &Limits) -> Result<Vec<u8>, String> {
    match format {
        "png" => map.encode_png(limits),
        "bmp" => map.encode_bmp(limits),
        _ => Err("output image format must be png or bmp".into()),
    }
}
fn gltf_output(package: &GltfPackage, format: &str, limits: &Limits) -> Result<Vec<u8>, String> {
    match format {
        "gltf" => package.to_gltf(limits),
        "glb" => package.to_glb(limits),
        _ => Err("interchange format must be gltf or glb".into()),
    }
}
fn gltf_input(bytes: &[u8], limits: &Limits) -> Result<GltfPackage, String> {
    if bytes.starts_with(b"glTF") {
        GltfPackage::from_glb(bytes, limits)
    } else {
        GltfPackage::from_gltf(bytes, limits)
    }
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PatchSpec {
    path: String,
    #[serde(default)]
    is_user: bool,
}
pub fn handle(
    command: &str,
    args: &[String],
    ws: &Workspace,
    limits: &Limits,
) -> Option<Result<(), String>> {
    if !matches!(
        command,
        "upgrades-inspect"
            | "upgrades-edit"
            | "neighborhood-inspect"
            | "neighborhood-edit"
            | "neighborhood-nearest"
            | "city-image-inspect"
            | "city-convert"
            | "city-paint"
            | "city-road"
            | "asset-inspect"
            | "asset-export"
            | "asset-edit"
            | "mesh-from-obj"
            | "mesh-obj-export"
            | "mesh-mtl-export"
            | "mesh-obj-import"
            | "mesh-gltf-export"
            | "mesh-gltf-import"
            | "animation-gltf-export"
            | "animation-gltf-import"
            | "patch-inspect"
            | "patch-export"
    ) {
        return None;
    }
    Some((|| {
        match command {
            "upgrades-inspect" => {
                require(args, 1)?;
                let doc = UpgradeDocument::import(
                    &ws.read_limited(&args[0], MAX_EDITOR_JSON_BYTES)?,
                    limits,
                )?;
                println!("{}", doc.metadata_json(limits)?);
            }
            "upgrades-edit" | "neighborhood-edit" => {
                require(args, 4)?;
                let bytes = ws.read_limited(&args[0], MAX_EDITOR_JSON_BYTES)?;
                guard(&args[2], &bytes)?;
                let edits = edits(ws, &args[3], limits)?;
                let output = if command == "upgrades-edit" {
                    let mut doc = UpgradeDocument::import(&bytes, limits)?;
                    doc.apply(&args[2], &edits, limits)?;
                    doc.export(limits)?
                } else {
                    let mut doc = NeighborhoodDocument::import(&bytes, limits)?;
                    doc.apply(&args[2], &edits, limits)?;
                    doc.export(limits)?
                };
                publish(ws, &args[0], &args[1], &args[2], &output)?;
            }
            "neighborhood-inspect" | "neighborhood-nearest" => {
                require(
                    args,
                    if command == "neighborhood-inspect" {
                        1
                    } else {
                        3
                    },
                )?;
                let bytes = ws.read_limited(&args[0], MAX_EDITOR_JSON_BYTES)?;
                let doc = NeighborhoodDocument::import(&bytes, limits)?;
                if command == "neighborhood-inspect" {
                    println!(
                        "{}",
                        String::from_utf8(doc.export(limits)?).map_err(|e| e.to_string())?
                    );
                } else {
                    println!(
                        "{}",
                        serde_json::to_string(
                            &doc.nearest(integer(&args[1])?, integer(&args[2])?)?
                        )
                        .map_err(|e| e.to_string())?
                    );
                }
            }
            "city-image-inspect" | "city-convert" => {
                require(
                    args,
                    if command == "city-image-inspect" {
                        1
                    } else {
                        3
                    },
                )?;
                let bytes = ws.read(&args[0])?;
                let map = CityImage::decode(&bytes, limits)?;
                if command == "city-image-inspect" {
                    println!(
                        "{}",
                        serde_json::json!({"schema":"wonderland.creator.city-image.v1","source_sha256":sha256(&bytes),"width":map.width(),"height":map.height(),"first_rgba":map.pixel(0,0)?})
                    );
                } else {
                    publish(
                        ws,
                        &args[0],
                        &args[1],
                        &sha256(&bytes),
                        &image_output(&map, &args[2], limits)?,
                    )?;
                }
            }
            "city-paint" => {
                require(args, 11)?;
                let bytes = ws.read(&args[0])?;
                guard(&args[2], &bytes)?;
                let mut map = CityImage::decode(&bytes, limits)?;
                let layer = MapLayer::parse(&args[3])?;
                map.validate(layer, true)?;
                map.brush(
                    integer(&args[4])?,
                    integer(&args[5])?,
                    integer(&args[6])?,
                    [integer(&args[7])?, integer(&args[8])?, integer(&args[9])?],
                    layer,
                    limits,
                )?;
                publish(
                    ws,
                    &args[0],
                    &args[1],
                    &args[2],
                    &image_output(&map, &args[10], limits)?,
                )?;
            }
            "city-road" => {
                require(args, 9)?;
                let bytes = ws.read(&args[0])?;
                guard(&args[2], &bytes)?;
                let mut map = CityImage::decode(&bytes, limits)?;
                map.validate(MapLayer::Road, true)?;
                let erase = match args[7].as_str() {
                    "draw" => false,
                    "erase" => true,
                    _ => return Err("road operation must be draw or erase".into()),
                };
                map.road_stroke(
                    integer(&args[3])?,
                    integer(&args[4])?,
                    integer(&args[5])?,
                    integer(&args[6])?,
                    erase,
                    limits,
                )?;
                publish(
                    ws,
                    &args[0],
                    &args[1],
                    &args[2],
                    &image_output(&map, &args[8], limits)?,
                )?;
            }
            "asset-inspect" | "asset-export" | "asset-edit" => {
                require(
                    args,
                    match command {
                        "asset-inspect" => 2,
                        "asset-export" => 3,
                        _ => 5,
                    },
                )?;
                let kind = AssetKind::parse(&args[0])?;
                let bytes = ws.read(&args[1])?;
                let mut doc = AssetDocument::import(kind, &bytes, limits)?;
                if command == "asset-inspect" {
                    println!("{}", doc.metadata_json(limits)?);
                } else {
                    let expected = if command == "asset-edit" {
                        guard(&args[3], &bytes)?;
                        doc.apply(&args[3], &edits(ws, &args[4], limits)?, limits)?;
                        args[3].clone()
                    } else {
                        sha256(&bytes)
                    };
                    publish(ws, &args[1], &args[2], &expected, &doc.export(limits)?)?;
                }
            }
            "mesh-from-obj" => {
                require(args, 3)?;
                let bytes = ws.read(&args[0])?;
                let mesh = ObjModel::decode(&bytes, limits)?.to_fsom(&args[2], limits)?;
                let output =
                    reconstruction::encode_fsom(&mesh, limits).map_err(|e| e.to_string())?;
                MeshOverrideDocument::import(&output, limits)?;
                publish(ws, &args[0], &args[1], &sha256(&bytes), &output)?;
            }
            "mesh-obj-export" | "mesh-mtl-export" | "mesh-gltf-export" => {
                require(args, if command == "mesh-gltf-export" { 3 } else { 2 })?;
                let bytes = ws.read(&args[0])?;
                let doc = MeshOverrideDocument::import(&bytes, limits)?;
                let output = match command {
                    "mesh-obj-export" => doc.export_obj(limits)?,
                    "mesh-mtl-export" => doc.export_mtl(limits)?,
                    _ => gltf_output(
                        &GltfPackage::from_fsom(doc.mesh(), &doc.source_sha256(), limits)?,
                        &args[2],
                        limits,
                    )?,
                };
                publish(ws, &args[0], &args[1], &sha256(&bytes), &output)?;
            }
            "mesh-obj-import" | "mesh-gltf-import" => {
                require(args, 4)?;
                let bytes = ws.read(&args[0])?;
                guard(&args[2], &bytes)?;
                let mut doc = MeshOverrideDocument::import(&bytes, limits)?;
                let transfer = ws.read(&args[3])?;
                let output = if command == "mesh-obj-import" {
                    doc.import_obj(&args[2], &transfer, limits)?;
                    doc.export().to_vec()
                } else {
                    let candidate = gltf_input(&transfer, limits)?.apply_fsom(
                        doc.mesh(),
                        &doc.source_sha256(),
                        limits,
                    )?;
                    if &candidate == doc.mesh() {
                        bytes
                    } else {
                        let encoded = reconstruction::encode_fsom(&candidate, limits)
                            .map_err(|e| e.to_string())?;
                        if MeshOverrideDocument::import(&encoded, limits)?.mesh() != &candidate {
                            return Err("FSOm interchange candidate reopen mismatch".into());
                        }
                        encoded
                    }
                };
                guard(&sha256(&transfer), &ws.read(&args[3])?)?;
                publish(ws, &args[0], &args[1], &args[2], &output)?;
            }
            "animation-gltf-export" | "animation-gltf-import" => {
                require(
                    args,
                    if command == "animation-gltf-export" {
                        4
                    } else {
                        6
                    },
                )?;
                let bytes = ws.read(&args[0])?;
                let skeleton_bytes = ws.read(&args[1])?;
                let animation =
                    vitaboy::decode_animation(&bytes, limits).map_err(|e| e.to_string())?;
                let skeleton =
                    vitaboy::decode_skeleton(&skeleton_bytes, limits).map_err(|e| e.to_string())?;
                let animation_sha = sha256(&bytes);
                let skeleton_sha = sha256(&skeleton_bytes);
                let output = if command == "animation-gltf-export" {
                    gltf_output(
                        &GltfPackage::from_animation(
                            &animation,
                            &skeleton,
                            &animation_sha,
                            &skeleton_sha,
                            limits,
                        )?,
                        &args[3],
                        limits,
                    )?
                } else {
                    guard(&args[3], &bytes)?;
                    guard(&args[4], &skeleton_bytes)?;
                    let transfer = ws.read(&args[5])?;
                    let candidate = gltf_input(&transfer, limits)?.apply_animation(
                        &animation,
                        &skeleton,
                        &animation_sha,
                        &skeleton_sha,
                        limits,
                    )?;
                    guard(&sha256(&transfer), &ws.read(&args[5])?)?;
                    if candidate == animation {
                        bytes
                    } else {
                        let encoded = vitaboy::encode_animation(&candidate, limits)
                            .map_err(|e| e.to_string())?;
                        if vitaboy::decode_animation(&encoded, limits).map_err(|e| e.to_string())?
                            != candidate
                        {
                            return Err("animation interchange candidate reopen mismatch".into());
                        }
                        encoded
                    }
                };
                guard(&skeleton_sha, &ws.read(&args[1])?)?;
                publish(ws, &args[0], &args[2], &animation_sha, &output)?;
            }
            "patch-inspect" | "patch-export" => {
                require(args, if command == "patch-inspect" { 3 } else { 5 })?;
                let source = ws.read(&args[0])?;
                let name = &args[if command == "patch-inspect" { 1 } else { 2 }];
                let specs_path = &args[if command == "patch-inspect" { 2 } else { 4 }];
                let spec_bytes = ws.read_limited(specs_path, MAX_EDITOR_JSON_BYTES)?;
                let specs: Vec<PatchSpec> =
                    serde_json::from_value(json_support::parse(&spec_bytes, limits)?)
                        .map_err(|e| e.to_string())?;
                let mut patches = Vec::new();
                let mut retained = source.len();
                for spec in &specs {
                    let remaining = limits
                        .max_total_decoded_bytes
                        .saturating_div(16)
                        .saturating_sub(retained);
                    let bytes =
                        ws.read_limited(&spec.path, remaining.min(limits.max_input_bytes))?;
                    retained = retained
                        .checked_add(bytes.len())
                        .ok_or("patch aggregate overflow")?;
                    patches.push(bytes);
                }
                let inputs: Vec<_> = specs
                    .iter()
                    .zip(&patches)
                    .map(|(spec, bytes)| PatchInput {
                        name: &spec.path,
                        is_user: spec.is_user,
                        bytes,
                    })
                    .collect();
                let view = PatchView::resolve(name, &source, &inputs, limits)?;
                if command == "patch-inspect" {
                    println!(
                        "{}",
                        String::from_utf8(view.metadata_json(limits)?).map_err(|e| e.to_string())?
                    );
                } else {
                    guard(&args[3], &source)?;
                    guard(&sha256(&spec_bytes), &ws.read(specs_path)?)?;
                    for (spec, bytes) in specs.iter().zip(&patches) {
                        guard(&sha256(bytes), &ws.read(&spec.path)?)?;
                    }
                    publish(ws, &args[0], &args[1], &args[3], view.effective_bytes())?;
                }
            }
            _ => unreachable!(),
        }
        Ok(())
    })())
}
