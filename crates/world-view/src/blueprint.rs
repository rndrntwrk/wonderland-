//! XmlHouseData/VMWorldActivator adapter. No object GUID is replaced with a prop.
use crate::*;
use quick_xml::{
    Reader,
    events::{BytesStart, Event},
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use wonderland_render_core::{AssetKey, Vec3};

const XML_BYTE_BUDGET: usize = 16 * 1024 * 1024;

struct Records {
    size: Option<u16>,
    category: Option<i32>,
    floors: Vec<BTreeMap<String, String>>,
    walls: Vec<BTreeMap<String, String>>,
    pools: Vec<BTreeMap<String, String>>,
    objects: Vec<BTreeMap<String, String>>,
    sounds: Vec<BTreeMap<String, String>>,
}
fn invalid(message: &str) -> WorldError {
    WorldError(format!("source blueprint: {message}"))
}
fn number<T: std::str::FromStr>(
    attributes: &BTreeMap<String, String>,
    key: &str,
) -> Result<T, WorldError> {
    attributes
        .get(key)
        .ok_or_else(|| invalid(&format!("missing {key}")))?
        .parse()
        .map_err(|_| invalid(&format!("invalid {key}")))
}
fn attributes(
    start: &BytesStart<'_>,
    reader: &Reader<&[u8]>,
) -> Result<BTreeMap<String, String>, WorldError> {
    let mut values = BTreeMap::new();
    for attribute in start.attributes() {
        let attribute = attribute.map_err(|_| invalid("malformed or duplicate attribute"))?;
        let name = std::str::from_utf8(attribute.key.as_ref())
            .map_err(|_| invalid("attribute name encoding"))?;
        let value = attribute
            .decode_and_unescape_value(reader.decoder())
            .map_err(|_| invalid("attribute encoding"))?;
        if value.len() > 4096 || values.insert(name.into(), value.into_owned()).is_some() {
            return Err(invalid("oversized or duplicate attribute"));
        }
    }
    Ok(values)
}
fn start_element(
    name: &str,
    parent: Option<&str>,
    values: BTreeMap<String, String>,
    records: &mut Records,
) -> Result<(), WorldError> {
    let valid_parent = match name {
        "house" => parent.is_none(),
        "size" | "category" | "world" | "objects" | "sounds" => parent == Some("house"),
        "floors" | "walls" | "pools" => parent == Some("world"),
        "floor" => parent == Some("floors"),
        "wall" => parent == Some("walls"),
        "pool" => parent == Some("pools"),
        "object" => parent == Some("objects"),
        "sound" => parent == Some("sounds"),
        _ => false,
    };
    if !valid_parent {
        return Err(invalid("unknown or incorrectly nested element"));
    }
    let destination = match name {
        "floor" => Some(&mut records.floors),
        "wall" => Some(&mut records.walls),
        "pool" => Some(&mut records.pools),
        "object" => Some(&mut records.objects),
        "sound" => Some(&mut records.sounds),
        _ => None,
    };
    if let Some(destination) = destination {
        if destination.len() >= 262_144 {
            return Err(invalid("record allocation budget"));
        }
        destination.push(values);
    }
    Ok(())
}

pub(crate) fn parse(xml: &str, origin: &str, revision: &str) -> Result<WorldDocument, WorldError> {
    if xml.len() > XML_BYTE_BUDGET {
        return Err(invalid("XML allocation budget"));
    }
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut records = Records {
        size: None,
        category: None,
        floors: vec![],
        walls: vec![],
        pools: vec![],
        objects: vec![],
        sounds: vec![],
    };
    let mut stack: Vec<String> = vec![];
    let mut root_seen = false;
    let mut world_seen = false;
    loop {
        let event = reader.read_event().map_err(|_| invalid("malformed XML"))?;
        let empty = matches!(&event, Event::Empty(_));
        match event {
            Event::Start(start) | Event::Empty(start) => {
                let name = std::str::from_utf8(start.name().as_ref())
                    .map_err(|_| invalid("element encoding"))?
                    .to_owned();
                if stack.len() > 8 {
                    return Err(invalid("XML nesting budget"));
                }
                if name == "house" {
                    if root_seen {
                        return Err(invalid("multiple root elements"));
                    }
                    root_seen = true;
                }
                if name == "world" {
                    if world_seen {
                        return Err(invalid("duplicate world"));
                    }
                    world_seen = true;
                }
                start_element(
                    &name,
                    stack.last().map(String::as_str),
                    attributes(&start, &reader)?,
                    &mut records,
                )?;
                if !empty {
                    stack.push(name);
                }
            }
            Event::End(end) => {
                if stack.pop().as_deref().map(str::as_bytes) != Some(end.name().as_ref()) {
                    return Err(invalid("unbalanced XML"));
                }
            }
            Event::Text(value) => {
                let value = value.decode().map_err(|_| invalid("text encoding"))?;
                if value.is_empty() {
                    continue;
                }
                match stack.last().map(String::as_str) {
                    Some("size") => {
                        if records.size.is_some() {
                            return Err(invalid("duplicate size"));
                        }
                        records.size = Some(value.parse().map_err(|_| invalid("size"))?);
                    }
                    Some("category") => {
                        if records.category.is_some() {
                            return Err(invalid("duplicate category"));
                        }
                        records.category = Some(value.parse().map_err(|_| invalid("category"))?);
                    }
                    _ => return Err(invalid("unexpected element text")),
                }
            }
            Event::Decl(_) | Event::Comment(_) => {}
            Event::Eof => break,
            _ => {
                return Err(invalid(
                    "DTD, entities, CDATA and processing instructions are not blueprint data",
                ));
            }
        }
    }
    if !stack.is_empty() || !root_seen || !world_seen {
        return Err(invalid("incomplete house/world document"));
    }
    let width = records.size.ok_or_else(|| invalid("missing size"))?;
    let area = crate::document::checked_area(width, width)?;
    // VMArchitecture constructor starts with five stories. This is source input,
    // not a browser limit; normalized snapshots carry their own level count.
    let levels = 5;
    let mut tiles = vec![WorldTile::default(); area * usize::from(levels)];
    let index = |attributes: &BTreeMap<String, String>, level: u8| -> Result<usize, WorldError> {
        let x: u16 = number(attributes, "x")?;
        let y: u16 = number(attributes, "y")?;
        if x >= width || y >= width || level == 0 || level > levels {
            return Err(invalid("tile coordinates or level"));
        }
        Ok(
            (usize::from(level - 1) * usize::from(width) + usize::from(y)) * usize::from(width)
                + usize::from(x),
        )
    };
    let xml_level = |attributes: &BTreeMap<String, String>| -> Result<u8, WorldError> {
        number::<u8>(attributes, "level")?
            .checked_add(1)
            .ok_or_else(|| invalid("architecture level overflow"))
    };
    let mut floor_positions = BTreeSet::new();
    let mut wall_positions = BTreeSet::new();
    let mut pool_positions = BTreeSet::new();
    let mut material_ids = BTreeSet::new();
    for floor in &records.floors {
        let i = index(floor, xml_level(floor)?)?;
        if !floor_positions.insert(i) {
            return Err(invalid("duplicate floor tile"));
        }
        let value = number(floor, "value")?;
        tiles[i].floor = value;
        if value != 0 {
            material_ids.insert(value);
        }
    }
    for pool in &records.pools {
        let i = index(pool, 1)?;
        let _: i32 = number(pool, "value")?;
        if !pool_positions.insert(i) {
            return Err(invalid("duplicate pool tile"));
        }
        tiles[i].floor = 65_535;
    }
    for wall in &records.walls {
        let i = index(wall, xml_level(wall)?)?;
        if !wall_positions.insert(i) {
            return Err(invalid("duplicate wall tile"));
        }
        let segments: u8 = number(wall, "segments")?;
        if segments & !63 != 0 || segments & 48 == 48 {
            return Err(invalid("wall segments"));
        }
        let patterns = [
            number(wall, "tlp")?,
            number(wall, "trp")?,
            number(wall, "blp")?,
            number(wall, "brp")?,
        ];
        let styles = [number(wall, "tls")?, number(wall, "trs")?];
        let tile = &mut tiles[i];
        tile.diagonal = if segments & 32 != 0 {
            Some(WorldDiagonal::Vertical)
        } else if segments & 16 != 0 {
            Some(WorldDiagonal::Horizontal)
        } else {
            None
        };
        tile.half_floors = tile.diagonal.map(|diagonal| match diagonal {
            WorldDiagonal::Vertical => [patterns[0], styles[0]],
            WorldDiagonal::Horizontal => [styles[0], patterns[0]],
        });
        tile.wall = WorldWall {
            patterns,
            styles,
            object_styles: [0; 2],
            west: segments & 1 != 0,
            north: segments & 2 != 0,
            east: segments & 4 != 0,
            south: segments & 8 != 0,
        };
    }
    let mut diagnostics = vec![];
    let mut objects = vec![];
    for (record, object) in records.objects.iter().enumerate() {
        let source = BlueprintObject {
            record: record as u32,
            x: number(object, "x")?,
            y: number(object, "y")?,
            level: number(object, "level")?,
            direction: number(object, "dir")?,
            group: number(object, "group")?,
        };
        let guid = object
            .get("guid")
            .ok_or_else(|| invalid("missing object GUID"))?;
        let source_guid =
            u32::from_str_radix(guid.trim_start_matches("0x").trim_start_matches("0X"), 16)
                .map_err(|_| invalid("object GUID"))?;
        let level = u8::try_from(source.level).map_err(|_| invalid("object level"))?;
        if level > levels
            || (level > 0
                && (source.x < 0
                    || source.y < 0
                    || source.x >= i32::from(width)
                    || source.y >= i32::from(width)))
        {
            return Err(invalid("object coordinates"));
        }
        let yaw = match source.direction {
            0 => 0.,
            2 => std::f32::consts::FRAC_PI_2,
            4 => std::f32::consts::PI,
            _ => 3. * std::f32::consts::FRAC_PI_2,
        };
        objects.push(WorldObject {
            source_guid,
            blueprint: Some(source),
            snapshot: None,
            entity: None,
            visual_revision: 0,
            position_tiles: Vec3::new(
                source.x as f32,
                source.y as f32,
                f32::from(level.saturating_sub(1)) * 2.95,
            ),
            yaw_radians: yaw,
            dynamic_flags: [0; 2],
            room: 0,
            level,
            visible: level > 0,
            selectable: level > 0,
            model: None,
        });
        if level > 0 {
            diagnostics.push(WorldDiagnostic { code: "missing_object_model".into(), resource: format!("object:{source_guid:08X}:record:{record}"), message: format!("Object {source_guid:08X} at ({}, {}), floor {level}: original model and texture resources are unavailable.", source.x, source.y) });
        }
    }
    for material in material_ids {
        diagnostics.push(WorldDiagnostic { code: "missing_floor_texture".into(), resource: format!("floor:{material}"), message: format!("Original floor pattern {material} texture is unavailable; its source geometry uses a neutral material.") });
    }
    if !records.walls.is_empty() {
        diagnostics.push(WorldDiagnostic { code: "unresolved_room_support".into(), resource: "architecture:rooms".into(), message: "Blueprint walls are retained. Room, support and roof decisions require the source architecture projection.".into() });
    }
    // A wall-free empty lot is known outside; a furnished house's room state
    // cannot be guessed from having floors.
    if records.walls.is_empty() {
        for tile in &mut tiles {
            tile.indoors = Some(false);
        }
    }
    let source_counts = SourceCounts {
        floors: records.floors.len(),
        walls: records.walls.len(),
        pools: records.pools.len(),
        objects: records.objects.len(),
    };
    let sounds = records
        .sounds
        .iter()
        .map(|sound| {
            Ok(BlueprintSound {
                id: number(sound, "id")?,
                on: number(sound, "on")?,
            })
        })
        .collect::<Result<Vec<_>, WorldError>>()?;
    let effective_source = AssetKey(Sha256::digest(xml.as_bytes()).into());
    let document = WorldDocument {
        schema_version: WORLD_SCHEMA_VERSION,
        provenance: WorldProvenance {
            kind: WorldSourceKind::OriginalXml,
            origin: origin.into(),
            source_revision: revision.into(),
            effective_source,
        },
        revision: WorldRevision {
            lot_id: None,
            epoch: 0,
            tick: 0,
            architecture_revision: 0,
            content: effective_source,
        },
        lot: WorldLot {
            width,
            height: width,
            levels,
            terrain: source_terrain(width, width, &vec![0; area], 0)?,
            tiles,
            roof: None,
            cutaway: None,
        },
        objects,
        models: vec![],
        materials: vec![],
        source_counts: Some(source_counts),
        category: records.category,
        sounds,
        diagnostics,
    };
    document.validate()?;
    Ok(document)
}
