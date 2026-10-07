//! Original Vitaboy resources sampled at the accepted native tick.
//!
//! This does not predict simulation, consume marker events, or reconstruct a
//! pre-checkpoint visual skeleton. Each current timeline is sampled from the
//! original bind pose. Missing rig/attachments/resources stay diagnosed.
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use wonderland_avatar_content::{self as content, ImportedContent};
use wonderland_avatar_view::{CarryPose, Timeline, TimelineLayer, sample_timeline};
use wonderland_game_runtime::sim_core::avatars::outfits::OutfitReference;
use wonderland_game_runtime::{AvatarVisual, AvatarVisualFrame};
use wonderland_render_core::{Aabb, AssetKey, RenderLimits, RgbaImage, Vec2, Vec3};
use wonderland_world_view::{
    ModelContext, ModelPart, ModelTexture, ModelTextureSelector, WorldDiagnostic, WorldDocument,
    WorldError, WorldModel, WorldObject, WorldRevision, WorldSourceKind,
};

const ADULT: u32 = 0x7FD96B54;
struct Prepared {
    record: usize,
    identity: wonderland_render_core::EntityRef,
    visual_revision: u64,
    hash: AssetKey,
    parts: Vec<content::RenderablePart>,
}
pub struct NativeAvatarProjection {
    revision: WorldRevision,
    rig: Option<AssetKey>,
    avatars: Vec<Prepared>,
    identities: Vec<(usize, WorldObject)>,
    textures: BTreeSet<AssetKey>,
    diagnostics: Vec<WorldDiagnostic>,
}
fn issue(code: &str, resource: String, message: impl Into<String>) -> WorldDiagnostic {
    WorldDiagnostic {
        code: code.into(),
        resource,
        message: message.into(),
    }
}
fn resource(value: &AvatarVisual) -> String {
    format!(
        "native-avatar:{}:{}:guid:{:08x}",
        value.entity.object_id.0, value.entity.generation, value.guid
    )
}
fn packed(id: u64) -> content::FileKey {
    content::FileKey {
        file_id: (id >> 32) as u32,
        type_id: id as u32,
    }
}
fn outfit(
    value: &Option<OutfitReference>,
    bank: &ImportedContent,
) -> Result<Option<content::FileKey>, String> {
    match value {
        None | Some(OutfitReference::Id(0)) => Ok(None),
        Some(OutfitReference::Id(id)) => Ok(Some(packed(*id))),
        Some(OutfitReference::Name(name)) => {
            bank.named_outfit(name).map(Some).map_err(|e| e.to_string())
        }
        Some(OutfitReference::Legacy { .. }) => {
            Err("This native avatar requires its TS1 inline-outfit resource adapter.".into())
        }
    }
}
impl NativeAvatarProjection {
    pub fn prepare(
        frame: &AvatarVisualFrame,
        world: &WorldDocument,
        bank: &ImportedContent,
    ) -> Result<Self, WorldError> {
        world.validate()?;
        if world.provenance.kind != WorldSourceKind::LiveSession || world.revision != frame.revision
        {
            return Err(WorldError(
                "Native avatar frame does not match the accepted world.".into(),
            ));
        }
        let records: BTreeMap<_, _> = world
            .objects
            .iter()
            .enumerate()
            .filter_map(|(i, o)| o.entity.map(|id| (id, (i, o))))
            .collect();
        let mut seen = BTreeSet::new();
        let mut result = Self {
            revision: frame.revision,
            rig: bank.rig.as_ref().map(|r| r.key),
            avatars: vec![],
            identities: vec![],
            textures: BTreeSet::new(),
            diagnostics: vec![],
        };
        let limits = RenderLimits::default();
        let (mut vertices, mut indices) = (0usize, 0usize);
        for avatar in &frame.avatars {
            let id = wonderland_render_core::EntityRef {
                object_id: u32::try_from(avatar.entity.object_id.0)
                    .map_err(|_| WorldError("Invalid avatar ID.".into()))?,
                generation: avatar.entity.generation,
            };
            let &(record, object) = records
                .get(&id)
                .ok_or_else(|| WorldError("Stale native avatar generation.".into()))?;
            if !seen.insert(id)
                || object.source_guid != avatar.guid
                || object.visual_revision != avatar.visual_revision
            {
                return Err(WorldError(
                    "Native avatar identity does not match the accepted scene.".into(),
                ));
            }
            result.identities.push((record, object.clone()));
            if !object.visible {
                continue;
            }
            match prepare_parts(avatar, bank) {
                Ok(parts) => {
                    let added_v = parts.iter().map(|p| p.mesh.vertices.len()).sum::<usize>();
                    let added_i = parts.iter().map(|p| p.mesh.indices.len()).sum::<usize>();
                    if added_v > limits.max_vertices.saturating_sub(vertices)
                        || added_i > limits.max_indices.saturating_sub(indices)
                    {
                        result.diagnostics.push(issue(
                            "native_avatar_resource_limit",
                            resource(avatar),
                            "Original avatar geometry exceeds the scene budget.",
                        ));
                        continue;
                    }
                    vertices += added_v;
                    indices += added_i;
                    let mut hash = Sha256::new();
                    hash.update(b"native-current-avatar-pose-v1");
                    hash.update(bank.rig.as_ref().expect("parts require rig").key.0);
                    hash.update(serde_json::to_vec(avatar).map_err(|e| WorldError(e.to_string()))?);
                    for part in &parts {
                        hash.update(part.texture.0);
                        for vertex in &part.mesh.vertices {
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
                    }
                    result.textures.extend(parts.iter().map(|p| p.texture));
                    result.avatars.push(Prepared {
                        record,
                        identity: id,
                        visual_revision: avatar.visual_revision,
                        hash: AssetKey(hash.finalize().into()),
                        parts,
                    });
                }
                Err(message) => result.diagnostics.push(issue(
                    "native_avatar_resource_unavailable",
                    resource(avatar),
                    message,
                )),
            }
        }
        Ok(result)
    }
    pub fn texture_keys(&self) -> &BTreeSet<AssetKey> {
        &self.textures
    }

    /// Called with decoded pixels from the same imported bank. Work on a private
    /// candidate so stale/malformed input never partially replaces the displayed scene.
    pub fn apply(
        self,
        world: &mut WorldDocument,
        bank: &ImportedContent,
        decoded: &BTreeMap<AssetKey, RgbaImage>,
    ) -> Result<(), WorldError> {
        if world.revision != self.revision
            || world.provenance.kind != WorldSourceKind::LiveSession
            || bank.rig.as_ref().map(|r| r.key) != self.rig
        {
            return Err(WorldError(
                "Stale native avatar resource preparation.".into(),
            ));
        }
        for (record, original) in &self.identities {
            if world.objects.get(*record) != Some(original) {
                return Err(WorldError(
                    "Native entity changed during resource preparation.".into(),
                ));
            }
        }
        for avatar in &self.avatars {
            let object = world
                .objects
                .get(avatar.record)
                .ok_or_else(|| WorldError("Stale avatar scene record.".into()))?;
            if object.entity != Some(avatar.identity)
                || object.visual_revision != avatar.visual_revision
                || object.model.is_some()
            {
                return Err(WorldError(
                    "Native avatar scene changed during resource preparation.".into(),
                ));
            }
        }
        let mut next = world.clone();
        // This is a presentation revision, not the source entity's mutation counter.
        // Avatar timelines advance on accepted ticks without changing that counter.
        // Resource changes within one tick require an explicit viewport lifetime reset.
        for (record, _) in self.identities {
            next.objects[record].visual_revision = self.revision.tick;
        }
        next.diagnostics
            .retain(|d| !d.code.starts_with("native_avatar_"));
        next.diagnostics.extend(self.diagnostics);
        let mut pixels = next
            .models
            .iter()
            .flat_map(|m| &m.textures)
            .map(|t| t.image.pixels.len())
            .sum::<usize>();
        for avatar in self.avatars {
            let object = &next.objects[avatar.record];
            let name = format!(
                "native-avatar:{}:{}:guid:{:08x}",
                avatar.identity.object_id, avatar.identity.generation, object.source_guid
            );
            let missing = avatar
                .parts
                .iter()
                .find_map(|part| match decoded.get(&part.texture) {
                    None => Some("The original avatar UV texture is not decoded.".to_string()),
                    Some(image) => bank
                        .validate_decoded_texture(part.texture, image)
                        .err()
                        .map(|e| e.to_string()),
                });
            if let Some(message) = missing {
                next.diagnostics
                    .push(issue("native_avatar_texture_unavailable", name, message));
                continue;
            }
            let keys = avatar
                .parts
                .iter()
                .map(|p| p.texture)
                .collect::<BTreeSet<_>>();
            let added = keys
                .iter()
                .map(|key| decoded[key].pixels.len())
                .sum::<usize>();
            if added
                > RenderLimits::default()
                    .max_texture_pixels
                    .saturating_sub(pixels)
            {
                next.diagnostics.push(issue(
                    "native_avatar_resource_limit",
                    name,
                    "Original avatar textures exceed the scene budget.",
                ));
                continue;
            }
            pixels += added;
            let lookup = keys
                .into_iter()
                .enumerate()
                .map(|(i, k)| (k, i))
                .collect::<BTreeMap<_, _>>();
            let textures = lookup
                .iter()
                .map(|(&key, &index)| ModelTexture {
                    selector: ModelTextureSelector::Custom { id: index as u16 },
                    effective_asset: key,
                    uv_scale: Vec2::new(1., 1.),
                    image: decoded[&key].clone(),
                })
                .collect();
            let mut min = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
            let mut max = Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
            let mut parts = vec![];
            for part in avatar.parts {
                for v in &part.mesh.vertices {
                    min = Vec3::new(
                        min.x.min(v.position.x),
                        min.y.min(v.position.y),
                        min.z.min(v.position.z),
                    );
                    max = Vec3::new(
                        max.x.max(v.position.x),
                        max.y.max(v.position.y),
                        max.z.max(v.position.z),
                    );
                }
                parts.push(ModelPart {
                    texture: lookup[&part.texture],
                    mesh: part.mesh,
                });
            }
            let model = WorldModel {
                effective_source: avatar.hash,
                effective_content: avatar.hash,
                context: ModelContext::Vitaboy,
                format_version: 1,
                reconstruction_version: 0,
                groups: vec![parts],
                textures,
                bounds: Aabb::new(min, max)
                    .ok_or_else(|| WorldError("Invalid native avatar bounds.".into()))?,
                depth_mask: None,
            };
            next.objects[avatar.record].model = Some(next.models.len());
            next.models.push(model);
            // Preserve diagnostics for other missing instances, including a shared GUID.
            next.diagnostics.push(issue("native_avatar_current_pose",name,
                "Original resources sampled at the accepted animation frame. Historical bone retention, head seeking and container/bone attachments are not reconstructed."));
        }
        let unavailable = next
            .objects
            .iter()
            .filter(|o| o.model.is_none())
            .map(|o| format!("guid:{:08x}", o.source_guid))
            .collect::<BTreeSet<_>>();
        next.diagnostics
            .retain(|d| d.code != "runtime_model_unavailable" || unavailable.contains(&d.resource));
        next.validate()?;
        *world = next;
        Ok(())
    }
}
fn prepare_parts(
    avatar: &AvatarVisual,
    bank: &ImportedContent,
) -> Result<Vec<content::RenderablePart>, String> {
    if avatar.container.is_some() {
        return Err("Container-bound avatars require original SLOT/bone attachment data; no grounded substitute is shown.".into());
    }
    let rig = bank
        .rig
        .as_ref()
        .ok_or("The original avatar skeleton is not loaded.")?;
    if avatar.guid != ADULT || !rig.source().name.eq_ignore_ascii_case("adult") {
        return Err("This avatar needs its original OBJD-to-skeleton mapping. Only the canonical adult template is currently bound.".into());
    }
    avatar
        .outfits
        .validate()
        .map_err(|e| format!("Invalid source outfit: {e:?}"))?;
    if avatar.animations.layers.len() > 64 || avatar.animations.bound_appearances.len() > 128 {
        return Err("The native avatar animation/attachment budget was exceeded.".into());
    }
    let resolve = |state: &wonderland_game_runtime::AvatarAnimation| {
        let clip = bank.animation(&state.resource).map_err(|e| e.to_string())?;
        if !clip.matches_projection(&state.resource, state.num_frames, clip.key) {
            return Err(
                "Original animation metadata differs from the accepted runtime resource."
                    .to_string(),
            );
        }
        Ok(clip)
    };
    let mut timeline = Timeline::default();
    for state in &avatar.animations.layers {
        timeline.layers.push(TimelineLayer {
            clip: resolve(state)?,
            current_frame: state.current_frame,
            speed: state.speed,
            weight: state.weight,
            backwards: state.backwards,
            end_reached: state.end_reached,
            looping: state.looping,
        });
    }
    if let Some(state) = &avatar.animations.carry {
        timeline.carry = Some(CarryPose {
            clip: resolve(state)?,
            frame: state.current_frame,
        });
    }
    let mut pose = rig.bind_pose();
    sample_timeline(rig, &mut pose, &timeline, 0.).map_err(|e| e.to_string())?;
    let mut accessories = avatar
        .animations
        .bound_appearances
        .iter()
        .map(|n| bank.named_appearance(n).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let decoration = &avatar.outfits.decorations;
    for id in [
        decoration.back,
        decoration.head,
        decoration.tail,
        decoration.shoes,
    ] {
        if id == 0 {
            continue;
        }
        let entry = bank
            .catalog
            .outfits
            .get(&packed(id))
            .filter(|o| o.version == 1)
            .ok_or_else(|| format!("Original decoration outfit {id:016x} is unavailable."))?;
        accessories.push(entry.light_appearance);
    }
    let mut unique = BTreeSet::new();
    accessories.retain(|k| unique.insert(*k));
    let selection = content::AppearanceSelection {
        head: outfit(&avatar.outfits.head, bank)?,
        body: outfit(&avatar.outfits.body, bank)?,
        skin: content::Skin::try_from(avatar.outfits.skin_tone).map_err(|e| e.to_string())?,
        left: content::Gesture::try_from(avatar.animations.left_hand).map_err(|e| e.to_string())?,
        right: content::Gesture::try_from(avatar.animations.right_hand)
            .map_err(|e| e.to_string())?,
        accessories,
        ..Default::default()
    };
    let mut parts = bank.compose_at(&selection, &pose).map_err(|issues| {
        issues
            .iter()
            .take(4)
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" · ")
    })?;
    let scale = if avatar.scale_percent == 0 {
        1.
    } else {
        f32::from(avatar.scale_percent) / 100.
    };
    if scale <= 0. {
        return Err("Unsupported source avatar scale.".into());
    }
    let tint = if avatar.display_flags as u16 & 1 != 0 {
        [21. / 255., 168. / 255., 63. / 255., 168. / 255.]
    } else if avatar.ghost {
        [1., 1., 1., 64. / 255.]
    } else {
        [1.; 4]
    };
    for part in &mut parts {
        for vertex in &mut part.mesh.vertices {
            vertex.position = vertex.position * scale;
            vertex.color = tint;
        }
        part.mesh
            .validate(&RenderLimits::default())
            .map_err(|e| e.to_string())?;
    }
    Ok(parts)
}

/// Revalidate a renderer-admitted mesh pick against the latest native scene.
/// FrameStore remains responsible for the rendered frame/generation ticket.
pub fn native_pick_entity(
    world: &WorldDocument,
    pick: &wonderland_world_view::WorldPick,
) -> Option<wonderland_game_runtime::EntityRef> {
    use wonderland_world_view::WorldPickTarget;
    if world.provenance.kind != WorldSourceKind::LiveSession
        || world.revision.lot_id != pick.revision.lot_id
        || world.revision.epoch != pick.revision.epoch
        || world.revision.content != pick.revision.content
    {
        return None;
    }
    let WorldPickTarget::Object {
        entity: Some(entity),
        source_guid,
        ..
    } = pick.target
    else {
        return None;
    };
    world.objects.iter().find(|o| {
        o.entity == Some(entity) && o.source_guid == source_guid && o.visible && o.selectable
    })?;
    Some(wonderland_game_runtime::EntityRef {
        object_id: wonderland_game_runtime::ObjectId(i16::try_from(entity.object_id).ok()?),
        generation: entity.generation,
    })
}
