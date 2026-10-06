#![forbid(unsafe_code)]
pub use wonderland_render_core::derivatives::*;

use bincode::Options;
use sha2::{Digest, Sha256};
use std::fmt::Write as FmtWrite;
use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::Path;
use wonderland_render_core::*;

pub const REQUEST_MAGIC: &[u8; 8] = b"WLCFDR01";
pub const MAX_REQUEST_BYTES: u64 = 64 * 1024 * 1024;
pub type WorkerResult<T> = Result<T, Box<dyn std::error::Error>>;
pub const SOURCE_REQUEST_MAGIC: &[u8; 8] = b"WLCFSR01";
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SourceWorkerRequest {
    pub input: DerivativeInput,
    pub world: source::SourceWorld,
    pub options: source::SourceFacadeOptions,
    /// None emits day only; an unavailable night state never gets a night file.
    pub night_light_color: Option<[u8; 4]>,
}
pub fn encode_source_request(request: &SourceWorkerRequest) -> WorkerResult<Vec<u8>> {
    let size = codec().serialized_size(request)?;
    if size > MAX_REQUEST_BYTES {
        return Err("source request byte limit".into());
    }
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(size as usize + 8)?;
    bytes.extend_from_slice(SOURCE_REQUEST_MAGIC);
    bytes.extend(codec().serialize(request)?);
    Ok(bytes)
}
pub fn read_source_request(path: &Path) -> WorkerResult<SourceWorkerRequest> {
    let file = File::open(path)?;
    let length = file.metadata()?.len();
    if length > MAX_REQUEST_BYTES + 8 {
        return Err("source request byte limit".into());
    }
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(length as usize + 1)?;
    file.take(length + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 != length || !bytes.starts_with(SOURCE_REQUEST_MAGIC) {
        return Err("source request magic/length".into());
    }
    Ok(codec().deserialize(&bytes[8..])?)
}
pub fn render_source_to_directory(
    request: SourceWorkerRequest,
    directory: &Path,
) -> WorkerResult<WorkerSummary> {
    let source_bytes = encode_source_request(&request)?;
    drop(request);
    let request: SourceWorkerRequest = codec().deserialize(&source_bytes[8..])?;
    let limits = DerivativeRenderLimits {
        max_draws: 8192,
        ..DerivativeRenderLimits::default()
    };
    let mut prepared = request
        .world
        .prepare(request.input, request.options, limits)?
        .into_request();
    if request.night_light_color.is_none() {
        prepared = prepared.day_only();
    }
    let artifact = prepared.render()?;
    let container = artifact
        .to_fsof(request.night_light_color.unwrap_or([0; 4]))?
        .encode(true, fsof::FsofLimits::default())?;
    let container_hash = Sha256::digest(&container);
    fs::create_dir(directory)?;
    let result = (|| -> WorkerResult<WorkerSummary> {
        let mut image_hashes = Vec::new();
        for image in artifact.images() {
            let file = File::create(directory.join(image_filename(image.role)))?;
            image_hashes.push(write_png(BufWriter::new(file), &image.image)?);
        }
        fs::write(directory.join("source-request.wlcsrc"), source_bytes)?;
        fs::write(directory.join("facade.fsof"), container)?;
        let mut metadata = metadata_json(&prepared, &artifact, &image_hashes);
        let end = metadata.rfind('}').ok_or("metadata terminator")?;
        metadata.insert_str(end,&format!(",\n  \"fsof_sha256\": \"{}\",\n  \"source_preparation\": \"Blueprint room topology, altitude, source WorldCamera and source facade geometry\"\n",hex(&container_hash)));
        fs::write(directory.join("metadata.json"), metadata)?;
        Ok(WorkerSummary {
            key: hex(&artifact.key().0),
            digest: hex(&artifact.digest().0),
            image_count: artifact.images().len(),
            reservation_bytes: prepared.reservation_bytes(),
            work_units: prepared.work_units(),
        })
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(directory);
    }
    result
}

fn codec() -> impl Options {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_little_endian()
        .reject_trailing_bytes()
        .with_limit(MAX_REQUEST_BYTES)
}
/// Portable normalized input, distinct from a .fsof or legacy save file. Limits
/// apply both to the file read and to bincode's decoded input consumption.
pub fn read_request(path: &Path) -> WorkerResult<DerivativeInput> {
    let file = File::open(path)?;
    let expected = file.metadata()?.len();
    if expected > MAX_REQUEST_BYTES + REQUEST_MAGIC.len() as u64 {
        return Err("request byte limit".into());
    }
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(expected as usize + 1)?;
    file.take(expected + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 != expected {
        return Err("request changed while reading".into());
    }
    if bytes.len() as u64 > MAX_REQUEST_BYTES + REQUEST_MAGIC.len() as u64
        || !bytes.starts_with(REQUEST_MAGIC)
    {
        return Err("request magic/byte limit".into());
    }
    Ok(codec().deserialize(&bytes[REQUEST_MAGIC.len()..])?)
}
pub fn encode_request(input: &DerivativeInput) -> WorkerResult<Vec<u8>> {
    let size = codec().serialized_size(input)?;
    if size > MAX_REQUEST_BYTES {
        return Err("request byte limit".into());
    }
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(REQUEST_MAGIC.len() + size as usize)?;
    bytes.extend_from_slice(REQUEST_MAGIC);
    bytes.extend(codec().serialize(input)?);
    Ok(bytes)
}

pub fn image_filename(role: ImageRole) -> &'static str {
    match role {
        ImageRole::ThumbnailDay => "thumbnail-day.png",
        ImageRole::ThumbnailNight => "thumbnail-night.png",
        ImageRole::FloorDay => "floor-day.png",
        ImageRole::FloorNight => "floor-night.png",
        ImageRole::WallDay => "wall-day.png",
        ImageRole::WallNight => "wall-night.png",
    }
}
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

struct HashWriter<W> {
    writer: W,
    hash: Sha256,
}
impl<W: Write> Write for HashWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let written = self.writer.write(bytes)?;
        self.hash.update(&bytes[..written]);
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320u32 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}
fn chunk(writer: &mut impl Write, kind: &[u8; 4], payload: &[u8]) -> io::Result<()> {
    let length = u32::try_from(payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "PNG chunk too large"))?;
    writer.write_all(&length.to_be_bytes())?;
    writer.write_all(kind)?;
    writer.write_all(payload)?;
    // IDAT chunks are at most 65,540 bytes, so CRC scratch stays bounded.
    let mut bytes = Vec::with_capacity(kind.len() + payload.len());
    bytes.extend_from_slice(kind);
    bytes.extend_from_slice(payload);
    writer.write_all(&crc32(&bytes).to_be_bytes())
}
fn stored_block(writer: &mut impl Write, raw: &[u8], final_block: bool) -> io::Result<()> {
    let length = u16::try_from(raw.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "DEFLATE block too large"))?;
    let mut block = Vec::with_capacity(raw.len() + 5);
    block.push(u8::from(final_block));
    block.extend_from_slice(&length.to_le_bytes());
    block.extend_from_slice(&(!length).to_le_bytes());
    block.extend_from_slice(raw);
    chunk(writer, b"IDAT", &block)
}
/// Standards-compliant RGBA8 PNG with filter 0 and stored DEFLATE blocks. No
/// external codec/install is needed. Extra scratch stays below 256 KiB even for
/// the maximum image. Encoded bytes are deterministic and hashed as written.
pub fn write_png(writer: impl Write, image: &RgbaImage) -> WorkerResult<AssetKey> {
    image.validate(&RenderLimits::default())?;
    let mut writer = HashWriter {
        writer,
        hash: Sha256::new(),
    };
    writer.write_all(b"\x89PNG\r\n\x1a\n")?;
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&image.width.to_be_bytes());
    ihdr.extend_from_slice(&image.height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut writer, b"IHDR", &ihdr)?;
    chunk(&mut writer, b"IDAT", &[0x78, 0x01])?;
    let total = image.height as u64 * (image.width as u64 * 4 + 1);
    let mut done = 0u64;
    let mut a = 1u32;
    let mut b = 0u32;
    let mut raw = Vec::with_capacity(65_535);
    for row in image.pixels.chunks_exact(image.width as usize) {
        for byte in std::iter::once(0u8).chain(row.iter().flat_map(|pixel| pixel.iter().copied())) {
            raw.push(byte);
            a = (a + byte as u32) % 65_521;
            b = (b + a) % 65_521;
            done += 1;
            if raw.len() == 65_535 {
                stored_block(&mut writer, &raw, done == total)?;
                raw.clear();
            }
        }
    }
    if !raw.is_empty() {
        stored_block(&mut writer, &raw, true)?;
    }
    chunk(&mut writer, b"IDAT", &((b << 16) | a).to_be_bytes())?;
    chunk(&mut writer, b"IEND", &[])?;
    writer.flush()?;
    Ok(AssetKey(writer.hash.finalize().into()))
}

pub struct WorkerSummary {
    pub key: String,
    pub digest: String,
    pub image_count: usize,
    pub reservation_bytes: u64,
    pub work_units: u64,
}
/// Creates a new output directory. Existing directories are never overwritten.
/// Metadata is the last file written and marks a complete artifact set.
pub fn render_to_directory(
    input: DerivativeInput,
    directory: &Path,
) -> WorkerResult<WorkerSummary> {
    let request_bytes = encode_request(&input)?;
    // Use the same bounded wire-decoder path for a constructed fixture and an
    // external request. Vec spare capacity is real accounted memory, but must
    // not make metadata differ merely because a constructor grew a vector.
    drop(input);
    let input: DerivativeInput = codec().deserialize(&request_bytes[REQUEST_MAGIC.len()..])?;
    let prepared = PreparedDerivative::new(input, DerivativeRenderLimits::default())?;
    let artifact = prepared.render()?;
    fs::create_dir(directory)?;
    let result = (|| -> WorkerResult<WorkerSummary> {
        let mut image_hashes = Vec::new();
        for image in artifact.images() {
            let file = File::create(directory.join(image_filename(image.role)))?;
            let hash = write_png(BufWriter::new(file), &image.image)?;
            image_hashes.push(hash);
        }
        fs::write(directory.join("request.wlcdr"), request_bytes)?;
        let metadata = metadata_json(&prepared, &artifact, &image_hashes);
        fs::write(directory.join("metadata.json"), metadata)?;
        Ok(WorkerSummary {
            key: hex(&artifact.key().0),
            digest: hex(&artifact.digest().0),
            image_count: artifact.images().len(),
            reservation_bytes: prepared.reservation_bytes(),
            work_units: prepared.work_units(),
        })
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(directory);
    }
    result
}
fn metadata_json(
    prepared: &PreparedDerivative,
    artifact: &DerivativeArtifact,
    png_hashes: &[AssetKey],
) -> String {
    let mut out = String::new();
    let stamp = artifact.frame_stamp();
    writeln!(out, "{{\n  \"schema\": \"wonderland-cpu-derivative-v1\",\n  \"algorithm_version\": {DERIVATIVE_ALGORITHM_VERSION},").unwrap();
    writeln!(
        out,
        "  \"key\": \"{}\",\n  \"effective_input_sha256\": \"{}\",\n  \"artifact_sha256\": \"{}\",",
        hex(&artifact.key().0),
        hex(&artifact.source_digest().0),
        hex(&artifact.digest().0)
    )
    .unwrap();
    writeln!(out, "  \"frame\": {{\"lot_id\": \"{}\", \"epoch\": \"{}\", \"tick\": \"{}\", \"architecture_revision\": \"{}\", \"content\": \"{}\"}},", stamp.lot_id, stamp.epoch, stamp.tick, stamp.architecture_revision, hex(&stamp.content.0)).unwrap();
    writeln!(
        out,
        "  \"declared_source_provenance\": \"{}\",\n  \"lighting_passes\": [",
        hex(&prepared.input().source_provenance.0)
    )
    .unwrap();
    let phase_count = if artifact.images().iter().any(|image| {
        matches!(
            image.role,
            ImageRole::ThumbnailNight | ImageRole::FloorNight | ImageRole::WallNight
        )
    }) {
        2
    } else {
        1
    };
    for (index, light) in prepared
        .input()
        .lighting
        .iter()
        .take(phase_count)
        .enumerate()
    {
        writeln!(out, "    {{\"phase\": \"{}\", \"time_of_day\": {}, \"color_multiplier\": {:?}, \"provenance\": \"{}\"}}{}", if index == 0 { "day" } else { "night" }, light.time_of_day, light.color_multiplier, hex(&light.provenance.0), if index + 1 < phase_count { "," } else { "" }).unwrap();
    }
    writeln!(out, "  ],").unwrap();
    writeln!(out, "  \"reference\": \"CPU mesh rasterizer; explicit unlit or pre-baked materials; no legacy room-lighting or GPU parity claim\",\n  \"reservation_bytes\": {},\n  \"resident_bytes\": {},\n  \"work_units\": {},\n  \"images\": [", prepared.reservation_bytes(), artifact.resident_bytes(), prepared.work_units()).unwrap();
    for (index, (image, hash)) in artifact.images().iter().zip(png_hashes).enumerate() {
        let mut pixels = Sha256::new();
        for pixel in &image.image.pixels {
            pixels.update(pixel);
        }
        writeln!(out, "    {{\"file\": \"{}\", \"width\": {}, \"height\": {}, \"rgba_sha256\": \"{}\", \"png_sha256\": \"{}\"}}{}", image_filename(image.role), image.image.width, image.image.height, hex(&pixels.finalize()), hex(&hash.0), if index + 1 == artifact.images().len() { "" } else { "," }).unwrap();
    }
    writeln!(out, "  ],\n  \"regions\": [").unwrap();
    for (index, region) in artifact.regions().iter().enumerate() {
        let name = match region.role {
            RegionRole::Thumbnail => "thumbnail".into(),
            RegionRole::Floor(i) => format!("floor-{i}"),
            RegionRole::ObjectOverlay(i) => format!("object-overlay-{i}"),
            RegionRole::Wall(i) => format!("wall-{i}"),
        };
        write!(
            out,
            "    {{\"role\": \"{name}\", \"rect\": {:?}, \"clip_from_world_columns\": [",
            region.rect
        )
        .unwrap();
        for (column, values) in region.clip_from_world.cols.iter().enumerate() {
            if column > 0 {
                out.push_str(", ");
            }
            write!(out, "{values:?}").unwrap();
        }
        writeln!(
            out,
            "]}}{}",
            if index + 1 == artifact.regions().len() {
                ""
            } else {
                ","
            }
        )
        .unwrap();
    }
    out.push_str("  ]\n}\n");
    out
}

fn quad(points: [Vec3; 4], material: u32, layer: DrawLayer) -> DerivativeDraw {
    DerivativeDraw {
        owner: None,
        source_asset: AssetKey([11; 32]),
        model: Mat4::IDENTITY,
        material,
        layer,
        mesh: Mesh {
            vertices: points
                .into_iter()
                .zip([
                    Vec2::new(0., 0.),
                    Vec2::new(1., 0.),
                    Vec2::new(1., 1.),
                    Vec2::new(0., 1.),
                ])
                .map(|(position, uv)| Vertex {
                    position,
                    normal: Vec3::new(0., 1., 0.),
                    uv,
                    color: [1.; 4],
                })
                .collect(),
            indices: vec![0, 1, 2, 0, 2, 3],
        },
    }
}
/// Self-contained synthetic scene: two floors, exterior walls, a roof, one
/// generation-bound object, and distinct day/night textures. It contains no
/// game/provider assets and supplies no licensed-content acceptance evidence.
pub fn synthetic_fixture() -> DerivativeInput {
    let mut materials = vec![
        DerivativeMaterial::solid([1.; 4], [0.30, 0.38, 0.60, 1.]),
        DerivativeMaterial::solid([0.86, 0.73, 0.48, 1.], [0.28, 0.32, 0.52, 1.]),
        DerivativeMaterial::solid([0.73, 0.23, 0.16, 1.], [0.25, 0.20, 0.35, 1.]),
        DerivativeMaterial::solid([0.2, 0.5, 0.9, 1.], [0.2, 0.8, 1., 1.]),
    ];
    materials[0].day.texture = Some(RgbaImage {
        width: 2,
        height: 2,
        pixels: vec![
            [77, 117, 55, 255],
            [103, 141, 70, 255],
            [103, 141, 70, 255],
            [77, 117, 55, 255],
        ],
    });
    materials[0].night.texture = materials[0].day.texture.clone();
    let mut draws = vec![quad(
        [
            Vec3::new(19.5, 0., 19.5),
            Vec3::new(211.5, 0., 19.5),
            Vec3::new(211.5, 0., 211.5),
            Vec3::new(19.5, 0., 211.5),
        ],
        0,
        DrawLayer::Floor(0),
    )];
    let start = 28 * 16;
    let end = 49 * 16;
    let points = [[start, start], [end, start], [end, end], [start, end]];
    let mut walls = Vec::new();
    for floor in 0..2 {
        let low = floor as f32 * 2.95 * 3.;
        let high = low + 2.95 * 3.;
        if floor > 0 {
            draws.push(quad(
                [
                    Vec3::new(84., low, 84.),
                    Vec3::new(147., low, 84.),
                    Vec3::new(147., low, 147.),
                    Vec3::new(84., low, 147.),
                ],
                1,
                DrawLayer::Floor(floor),
            ));
        }
        for side in 0..4 {
            let a = points[side];
            let b = points[(side + 1) % 4];
            walls.push(FacadeWall {
                points: [a, b],
                floor,
                terrain_height: 0.,
                outside: OutsideSide::Right,
                room_provenance: AssetKey([12; 32]),
            });
            draws.push(quad(
                [
                    Vec3::new(a[0] as f32 * 3. / 16., low, a[1] as f32 * 3. / 16.),
                    Vec3::new(b[0] as f32 * 3. / 16., low, b[1] as f32 * 3. / 16.),
                    Vec3::new(b[0] as f32 * 3. / 16., high, b[1] as f32 * 3. / 16.),
                    Vec3::new(a[0] as f32 * 3. / 16., high, a[1] as f32 * 3. / 16.),
                ],
                1,
                DrawLayer::Wall,
            ));
        }
    }
    draws.push(quad(
        [
            Vec3::new(84., 17.7, 84.),
            Vec3::new(147., 17.7, 84.),
            Vec3::new(147., 17.7, 147.),
            Vec3::new(84., 17.7, 147.),
        ],
        2,
        DrawLayer::Roof(1),
    ));
    let object = EntityRef {
        object_id: 1,
        generation: 1,
    };
    let asset = AssetKey([13; 32]);
    let mut crate_top = quad(
        [
            Vec3::new(-4., 1., -4.),
            Vec3::new(4., 1., -4.),
            Vec3::new(4., 1., 4.),
            Vec3::new(-4., 1., 4.),
        ],
        3,
        DrawLayer::Object(1),
    );
    crate_top.owner = Some(object);
    crate_top.source_asset = asset;
    draws.push(crate_top);
    let camera = Mat4::orthographic_rh(-145., 145., -145., 145., 0., 600.).unwrap()
        * Mat4::look_at_rh(
            Vec3::new(240., 180., 270.),
            Vec3::new(115.5, 0., 115.5),
            Vec3::new(0., 1., 0.),
        )
        .unwrap();
    DerivativeInput {
        frame: RenderFrame {
            stamp: FrameStamp {
                lot_id: 7001,
                epoch: 1,
                tick: 60,
                architecture_revision: 2,
                content: AssetKey([21; 32]),
            },
            entities: vec![EntityProjection {
                reference: object,
                visual_revision: 1,
                transform: Transform {
                    translation: Vec3::new(115.5, 0., 115.5),
                    ..Transform::IDENTITY
                },
                previous_transform: None,
                asset,
                level: 1,
                visible: true,
                selectable: false,
            }],
            selected: None,
        },
        source_provenance: AssetKey([22; 32]),
        lighting: [LightingPass::DAY, LightingPass::NIGHT],
        materials,
        draws,
        output: DerivativeOutput::Facade(FacadeRequest {
            lot_width: 77,
            lot_height: 77,
            floor_tiles: 64,
            floor_resolution_per_tile: 2,
            stories: 2,
            floors_used: 2,
            roof_on_floor: true,
            walls,
            thumbnail: Some(ThumbnailRequest {
                width: 256,
                height: 256,
                clip_from_world: camera,
                clear: [0; 4],
            }),
        }),
    }
}

/// The existing synthetic two-story lot, fed through actual source-world
/// preparation: per-tile ground, exterior room topology, roofs, cameras and FSOf.
pub fn source_fixture() -> SourceWorkerRequest {
    let mut input = synthetic_fixture();
    input.draws.retain(|draw| draw.layer != DrawLayer::Floor(0));
    let width = 77usize;
    let area = width * width;
    let mut tiles = vec![source::SourceTile::default(); area * 2];
    for level in 0..2 {
        for y in 28..49 {
            for x in 28..49 {
                tiles[level * area + y * width + x].floor_pattern = 1;
            }
        }
        for y in 28..49 {
            tiles[level * area + y * width + 28].walls[0] = true;
            tiles[level * area + y * width + 49].walls[0] = true;
        }
        for x in 28..49 {
            tiles[level * area + 28 * width + x].walls[1] = true;
            tiles[level * area + 49 * width + x].walls[1] = true;
        }
    }
    let mut draw_tiles: Vec<_> = input
        .draws
        .iter()
        .map(|draw| match draw.layer {
            DrawLayer::Floor(level) => Some([28, 28, level as u16 + 1]),
            _ => None,
        })
        .collect();
    let mut ground = Vec::new();
    let mut ground_tiles = Vec::new();
    for y in 6..70 {
        for x in 6..70 {
            let px = x as f32 * 3.;
            let py = y as f32 * 3.;
            ground.push(quad(
                [
                    Vec3::new(px, 0., py),
                    Vec3::new(px + 3., 0., py),
                    Vec3::new(px + 3., 0., py + 3.),
                    Vec3::new(px, 0., py + 3.),
                ],
                0,
                DrawLayer::Floor(0),
            ));
            ground_tiles.push(Some([x, y, 1]));
        }
    }
    ground.extend(input.draws);
    ground_tiles.append(&mut draw_tiles);
    input.draws = ground;
    SourceWorkerRequest {
        input,
        world: source::SourceWorld {
            width: 77,
            height: 77,
            stories: 2,
            altitude: vec![0; area],
            base_alt: 0,
            tiles,
            rooms: None,
            fine_area: None,
        },
        options: source::SourceFacadeOptions {
            draw_tiles: ground_tiles,
            ..Default::default()
        },
        night_light_color: Some([60, 70, 110, 255]),
    }
}
