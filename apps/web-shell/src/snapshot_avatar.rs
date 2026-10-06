//! First, zero-time original avatar restore presentation from an FSOv refresh.
//!
//! VM.Load constructs a new SimAvatar, then restores animation state. Its first
//! FractionalAnim(0) samples those layers over the original resource skeleton,
//! with Idle hands and zero head-seek weight; carry is an integer-frame override.
//! This never reconstructs omitted pre-save visual history, executes client-join
//! scripts/markers, advances the VM, or turns a reusable source ID into authority.
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use wonderland_avatar_content::{self as content, ImportedContent};
use wonderland_avatar_view::{CarryPose, Timeline, TimelineLayer, sample_timeline};
use wonderland_render_core::{Aabb, AssetKey, RenderLimits, RgbaImage, Vec2, Vec3};
use wonderland_vm_protocol::{
    Snapshot,
    snapshot::{Appearance, Avatar, Entity, Outfit},
};
use wonderland_world_view::{
    ModelContext, ModelPart, ModelTexture, ModelTextureSelector, WorldDiagnostic, WorldDocument,
    WorldError, WorldModel, WorldSourceKind,
};

/// VMAvatar.TEMPLATE_PERSON. Custom/avatar-pet GUIDs need their original OBJD/STR
/// skeleton mapping; selecting whatever skeleton happened to be loaded is wrong.
const TEMPLATE_PERSON: u32 = 0x7FD96B54;

fn diagnostic(code: &str, resource: &str, message: impl Into<String>) -> WorldDiagnostic {
    WorldDiagnostic {
        code: code.into(),
        resource: resource.into(),
        message: message.into(),
    }
}
fn source_resource(entity: &Entity, record: usize) -> String {
    format!(
        "object:{:08X}:source-id:{}:record:{record}",
        entity.guid, entity.object_id
    )
}
fn source_key(snapshot: &Snapshot) -> AssetKey {
    AssetKey(Sha256::digest(&snapshot.source_body).into())
}

struct PreparedAvatar {
    record: usize,
    resource: String,
    source: AssetKey,
    parts: Vec<content::RenderablePart>,
}

/// CPU preparation is independent of asynchronous browser image decoding. This
/// lets the caller decode only referenced UV textures, excluding thumbnails and
/// unrelated outfits from a possibly large character content collection.
pub struct SnapshotAvatarProjection {
    source: AssetKey,
    generation: u64,
    avatars: Vec<PreparedAvatar>,
    attempted: BTreeSet<String>,
    diagnostics: Vec<WorldDiagnostic>,
    texture_keys: BTreeSet<AssetKey>,
}
impl SnapshotAvatarProjection {
    pub fn prepare(
        snapshot: &Snapshot,
        world: &WorldDocument,
        content: &ImportedContent,
    ) -> Result<Self, WorldError> {
        world.validate()?;
        if world.provenance.kind != WorldSourceKind::LegacySnapshot
            || world.provenance.effective_source != source_key(snapshot)
            || world.objects.len() != snapshot.entities.len()
            || world
                .objects
                .iter()
                .zip(&snapshot.entities)
                .enumerate()
                .any(|(record, (object, entity))| {
                    object.source_guid != entity.guid
                        || object.snapshot.is_none_or(|id| {
                            id.record != record as u32
                                || id.object_id != entity.object_id
                                || id.persistent_id != entity.persist_id
                                || id.x != entity.position.x
                                || id.y != entity.position.y
                                || id.level != entity.position.level
                                || id.avatar != matches!(entity.appearance, Appearance::Avatar(_))
                                || id.presentation_generation
                                    != world.revision.architecture_revision
                        })
                })
        {
            return Err(WorldError(
                "avatar resources require the matching accepted source snapshot presentation"
                    .into(),
            ));
        }
        let mut projection = Self {
            source: source_key(snapshot),
            generation: world.revision.architecture_revision,
            avatars: vec![],
            attempted: BTreeSet::new(),
            diagnostics: vec![],
            texture_keys: BTreeSet::new(),
        };
        let limits = RenderLimits::default();
        let (mut vertices, mut indices) = (0usize, 0usize);
        for (record, (entity, object)) in snapshot.entities.iter().zip(&world.objects).enumerate() {
            let Appearance::Avatar(avatar) = &entity.appearance else {
                continue;
            };
            if !object.visible {
                continue;
            }
            let resource = source_resource(entity, record);
            projection.attempted.insert(resource.clone());
            match prepare_avatar(entity, avatar, record, content, &resource) {
                Ok(prepared) => {
                    let added_vertices: usize = prepared
                        .parts
                        .iter()
                        .map(|part| part.mesh.vertices.len())
                        .sum();
                    let added_indices: usize = prepared
                        .parts
                        .iter()
                        .map(|part| part.mesh.indices.len())
                        .sum();
                    if added_vertices > limits.max_vertices.saturating_sub(vertices)
                        || added_indices > limits.max_indices.saturating_sub(indices)
                    {
                        projection.diagnostics.push(diagnostic(
                            "avatar_resource_limit",
                            &resource,
                            "The source avatar meshes exceed the bounded world geometry budget.",
                        ));
                        continue;
                    }
                    vertices += added_vertices;
                    indices += added_indices;
                    projection
                        .texture_keys
                        .extend(prepared.parts.iter().map(|part| part.texture));
                    projection.avatars.push(prepared);
                }
                Err(issue) => projection.diagnostics.push(issue),
            }
        }
        Ok(projection)
    }
    pub fn texture_keys(&self) -> &BTreeSet<AssetKey> {
        &self.texture_keys
    }

    /// Materialization preserves snapshot fences while changing the content
    /// fingerprint, so a resource replacement cannot reuse an old pixel pick.
    pub fn apply(
        self,
        world: &mut WorldDocument,
        content: &ImportedContent,
        decoded: &BTreeMap<AssetKey, RgbaImage>,
    ) -> Result<(), WorldError> {
        if world.provenance.kind != WorldSourceKind::LegacySnapshot
            || world.provenance.effective_source != self.source
            || world.revision.architecture_revision != self.generation
        {
            return Err(WorldError(
                "stale source avatar resource preparation".into(),
            ));
        }
        world.diagnostics.retain(|issue| {
            issue.code != "missing_avatar_visual" || !self.attempted.contains(&issue.resource)
        });
        world.diagnostics.extend(self.diagnostics);
        let mut texture_pixels = world
            .models
            .iter()
            .flat_map(|model| &model.textures)
            .map(|texture| texture.image.pixels.len())
            .sum::<usize>();
        for avatar in self.avatars {
            let missing = avatar
                .parts
                .iter()
                .find_map(|part| match decoded.get(&part.texture) {
                    None => Some(format!(
                        "Original UV texture {:?} has not been decoded.",
                        part.texture
                    )),
                    Some(image) => content
                        .validate_decoded_texture(part.texture, image)
                        .err()
                        .map(|issue| issue.to_string()),
                });
            if let Some(message) = missing {
                world.diagnostics.push(diagnostic(
                    "missing_avatar_texture",
                    &avatar.resource,
                    message,
                ));
                continue;
            }
            let keys: BTreeSet<_> = avatar.parts.iter().map(|part| part.texture).collect();
            let added_pixels: usize = keys.iter().map(|key| decoded[key].pixels.len()).sum();
            if added_pixels
                > RenderLimits::default()
                    .max_texture_pixels
                    .saturating_sub(texture_pixels)
            {
                world.diagnostics.push(diagnostic(
                    "avatar_resource_limit",
                    &avatar.resource,
                    "The source avatar textures exceed the bounded world texture budget.",
                ));
                continue;
            }
            texture_pixels += added_pixels;
            let texture_indices: BTreeMap<_, _> = keys
                .into_iter()
                .enumerate()
                .map(|(index, key)| (key, index))
                .collect();
            let mut textures = Vec::with_capacity(texture_indices.len());
            for (&key, &index) in &texture_indices {
                textures.push(ModelTexture {
                    selector: ModelTextureSelector::Custom { id: index as u16 },
                    effective_asset: key,
                    uv_scale: Vec2::new(1., 1.),
                    image: decoded[&key].clone(),
                });
            }
            let mut min = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
            let mut max = Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
            let mut parts = Vec::with_capacity(avatar.parts.len());
            for part in avatar.parts {
                for vertex in &part.mesh.vertices {
                    min = Vec3::new(
                        min.x.min(vertex.position.x),
                        min.y.min(vertex.position.y),
                        min.z.min(vertex.position.z),
                    );
                    max = Vec3::new(
                        max.x.max(vertex.position.x),
                        max.y.max(vertex.position.y),
                        max.z.max(vertex.position.z),
                    );
                }
                parts.push(ModelPart {
                    texture: texture_indices[&part.texture],
                    mesh: part.mesh,
                });
            }
            let model = WorldModel {
                effective_source: avatar.source,
                effective_content: avatar.source,
                context: ModelContext::Vitaboy,
                format_version: 1,
                reconstruction_version: 0,
                groups: vec![parts],
                textures,
                bounds: Aabb::new(min, max)
                    .ok_or_else(|| WorldError("invalid source avatar bounds".into()))?,
                depth_mask: None,
            };
            world.objects[avatar.record].model = Some(world.models.len());
            world.models.push(model);
            world.diagnostics.push(diagnostic("snapshot_avatar_restore_pose", &avatar.resource,
                "Showing the first zero-time pose restored from this snapshot and original resources. Earlier bone/gesture history, head seeking, ground-normal smoothing and client-join scripts are not replayed."));
        }
        if !world.models.is_empty() {
            let mut hash = Sha256::new();
            hash.update(b"snapshot-avatar-content-v1");
            hash.update(self.source.0);
            for model in &world.models {
                hash.update(model.effective_content.0);
            }
            world.revision.content = AssetKey(hash.finalize().into());
        }
        world.validate()
    }
}

pub fn restore_snapshot_avatars(
    snapshot: &Snapshot,
    world: &mut WorldDocument,
    content: &ImportedContent,
    decoded: &BTreeMap<AssetKey, RgbaImage>,
) -> Result<(), WorldError> {
    SnapshotAvatarProjection::prepare(snapshot, world, content)?.apply(world, content, decoded)
}

fn outfit_key(
    outfit: &Outfit,
    content: &ImportedContent,
) -> Result<Option<content::FileKey>, String> {
    if outfit.id == u64::from(u32::MAX) {
        return content
            .named_outfit(
                outfit
                    .name
                    .as_deref()
                    .ok_or("named source outfit has no name")?,
            )
            .map(Some)
            .map_err(|issue| issue.to_string());
    }
    if outfit.name.is_some() {
        return Err("numeric source outfit unexpectedly has a name".into());
    }
    Ok((outfit.id != 0).then_some(packed_key(outfit.id)))
}
fn packed_key(id: u64) -> content::FileKey {
    content::FileKey {
        file_id: (id >> 32) as u32,
        type_id: id as u32,
    }
}

fn prepare_avatar(
    entity: &Entity,
    avatar: &Avatar,
    record: usize,
    content: &ImportedContent,
    resource: &str,
) -> Result<PreparedAvatar, WorldDiagnostic> {
    let fail = |code, message| diagnostic(code, resource, message);
    if entity.object_data.len() <= 34 {
        return Err(fail(
            "unsupported_avatar_state",
            "Source avatar visibility is unavailable: its required Hidden field is missing.".into(),
        ));
    }
    if entity.container != 0 {
        return Err(fail(
            "unresolved_avatar_container",
            format!(
                "Avatar source container {} slot {} needs its original SLOT/bone attachment resources; its raw tile position is not substituted.",
                entity.container, entity.container_slot
            ),
        ));
    }
    let rig = content.rig.as_ref().ok_or_else(|| {
        fail(
            "missing_avatar_resource",
            "The original avatar skeleton is not loaded.".into(),
        )
    })?;
    if entity.guid != TEMPLATE_PERSON || !rig.source().name.eq_ignore_ascii_case("adult") {
        return Err(fail(
            "unresolved_avatar_rig",
            format!(
                "Avatar {:08X} needs its original OBJD body-string to skeleton binding. Only the pinned adult template and original adult skeleton are resolved.",
                entity.guid
            ),
        ));
    }
    if avatar.person_data.len() <= 74 {
        return Err(fail(
            "unsupported_avatar_state",
            "Source person data does not contain scale and display state.".into(),
        ));
    }
    let scale = match avatar.person_data[63] {
        0 => 1.,
        value => f32::from(value) / 100.,
    };
    if scale <= 0. {
        return Err(fail(
            "unsupported_avatar_scale",
            format!("Source avatar scale {scale} is outside supported positive mesh transforms."),
        ));
    }
    if avatar.animations.len() > 64 || avatar.bound_appearances.len() > 128 {
        return Err(fail(
            "avatar_resource_limit",
            "Source animation/appearance count exceeds the bounded presentation capacity.".into(),
        ));
    }
    let mut timeline = Timeline::default();
    let mut prefix = 0f32;
    for state in &avatar.animations {
        if !state.frame.is_finite() || !state.speed.is_finite() || !state.weight.is_finite() {
            return Err(fail(
                "unsupported_avatar_animation",
                "Source animation has a nonfinite frame, speed or blend weight.".into(),
            ));
        }
        prefix += state.weight;
        if !prefix.is_finite() || (!state.end_reached && prefix == 0.) {
            return Err(fail("unsupported_avatar_animation", "Source animation has a nonfinite or zero accumulated blend weight; no substitute pose is displayed.".into()));
        }
        // VMAnimationState.Load appends .anim to the serialized Anim.Name.
        let name = format!("{}.anim", state.name);
        let clip = content
            .animation(&name)
            .map_err(|issue| fail("missing_avatar_animation", issue.to_string()))?;
        timeline.layers.push(TimelineLayer {
            clip,
            current_frame: state.frame,
            speed: state.speed,
            weight: state.weight,
            backwards: state.backwards,
            end_reached: state.end_reached,
            looping: state.looped,
        });
    }
    if let Some(state) = &avatar.carry_animation {
        let name = format!("{}.anim", state.name);
        timeline.carry = Some(CarryPose {
            clip: content
                .animation(&name)
                .map_err(|issue| fail("missing_avatar_animation", issue.to_string()))?,
            frame: state.frame,
        });
    }
    let mut pose = rig.bind_pose();
    sample_timeline(rig, &mut pose, &timeline, 0.)
        .map_err(|issue| fail("unsupported_avatar_animation", issue.to_string()))?;
    let skin = content::Skin::try_from(avatar.skin_tone)
        .map_err(|issue| fail("unsupported_avatar_state", issue.to_string()))?;
    let selection = content::AppearanceSelection {
        head: outfit_key(&avatar.head, content)
            .map_err(|issue| fail("missing_avatar_resource", issue))?,
        body: outfit_key(&avatar.body, content)
            .map_err(|issue| fail("missing_avatar_resource", issue))?,
        skin,
        // SimAvatar starts Idle; serialized xevt queues are not hand gestures.
        left: content::Gesture::Idle,
        right: content::Gesture::Idle,
        ..Default::default()
    };
    let mut accessories = avatar
        .bound_appearances
        .iter()
        .map(|name| {
            content
                .named_appearance(name)
                .map_err(|issue| fail("missing_avatar_resource", issue.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    // VMAvatar.Load adds bound appearances, then Back, Head, Tail, Shoes.
    // Avatar.AddAccessory deduplicates by resolved Appearance across those
    // categories. Body/head/hands use separate AddAppearance instances.
    for index in [1, 0, 3, 2] {
        let id = avatar.decoration[index];
        if id == 0 {
            continue;
        }
        let outfit = content
            .catalog
            .outfits
            .get(&packed_key(id))
            .filter(|outfit| outfit.version == 1)
            .ok_or_else(|| {
                fail(
                    "missing_avatar_resource",
                    format!("Original decoration outfit {id:016x} is absent or unsupported."),
                )
            })?;
        accessories.push(outfit.light_appearance);
    }
    let mut seen_accessories = BTreeSet::new();
    accessories.retain(|appearance| seen_accessories.insert(*appearance));
    let mut parts = vec![];
    let compose = |selection: &content::AppearanceSelection| {
        content.compose_at(selection, &pose).map_err(|issues| {
            fail(
                "missing_avatar_resource",
                issues
                    .iter()
                    .take(4)
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" · "),
            )
        })
    };
    if selection.head.is_some() || selection.body.is_some() {
        parts.extend(compose(&selection)?);
    }
    if !accessories.is_empty() {
        // Literal source restore attaches decorations before SkinTone is loaded.
        // SimAvatar.Appearance only reloads its head. These initial decorations
        // therefore select Light, while later head/body/hands use saved skin.
        parts.extend(compose(&content::AppearanceSelection {
            accessories,
            skin: content::Skin::Light,
            left: content::Gesture::None,
            right: content::Gesture::None,
            ..Default::default()
        })?);
    }
    if parts.is_empty() {
        return Err(fail(
            "missing_avatar_resource",
            "The source avatar has no resolved appearance parts.".into(),
        ));
    }
    let display = avatar.person_data[74] as u16;
    let tint = if display & 1 != 0 {
        [21. / 255., 168. / 255., 63. / 255., 168. / 255.]
    } else if avatar.person_data[68] > 0 {
        [1., 1., 1., 64. / 255.]
    } else {
        [1.; 4]
    };
    let mut hash = Sha256::new();
    hash.update(b"source-avatar-first-restore-v1");
    hash.update(rig.key.0);
    hash.update(
        serde_json::to_vec(avatar)
            .map_err(|issue| fail("unsupported_avatar_state", issue.to_string()))?,
    );
    for part in &mut parts {
        hash.update(part.texture.0);
        for vertex in &mut part.mesh.vertices {
            vertex.position = vertex.position * scale;
            vertex.color = tint;
            for value in [
                vertex.position.x,
                vertex.position.y,
                vertex.position.z,
                vertex.normal.x,
                vertex.normal.y,
                vertex.normal.z,
                vertex.uv.x,
                vertex.uv.y,
            ]
            .into_iter()
            .chain(vertex.color)
            {
                hash.update(value.to_bits().to_le_bytes());
            }
        }
        for index in &part.mesh.indices {
            hash.update(index.to_le_bytes());
        }
        part.mesh
            .validate(&RenderLimits::default())
            .map_err(|issue| fail("unsupported_avatar_geometry", issue.to_string()))?;
    }
    Ok(PreparedAvatar {
        record,
        resource: resource.into(),
        source: AssetKey(hash.finalize().into()),
        parts,
    })
}
