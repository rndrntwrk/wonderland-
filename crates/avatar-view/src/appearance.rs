use crate::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use wonderland_render_core::{AssetKey, EntityRef};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Skin {
    #[default]
    Light = 0,
    Medium = 1,
    Dark = 2,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Gesture {
    #[default]
    Idle = 0,
    Pointing = 1,
    Fist = 2,
    None = 3,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PartRole {
    Body,
    Head,
    RightHand,
    LeftHand,
    HeadDecoration,
    Back,
    Shoes,
    Tail,
    Accessory,
}
#[derive(Clone, Debug)]
pub struct HandSet {
    pub idle: FileKey,
    pub fist: FileKey,
    pub pointing: FileKey,
}
#[derive(Clone, Debug)]
pub struct HandPair {
    pub right: HandSet,
    pub left: HandSet,
}
#[derive(Clone, Debug)]
pub struct HandGroup {
    pub skins: [HandPair; 3],
}
#[derive(Clone, Debug)]
pub struct LiteralHand {
    pub mesh: String,
    pub texture: String,
}
#[derive(Clone, Debug)]
pub struct LiteralHandSet {
    pub idle: LiteralHand,
    pub fist: LiteralHand,
    pub pointing: LiteralHand,
}
impl LiteralHandSet {
    pub fn select(&self, gesture: Gesture, right: bool) -> Option<LiteralHand> {
        let mut hand = match gesture {
            Gesture::Idle => self.idle.clone(),
            Gesture::Fist => self.fist.clone(),
            Gesture::Pointing => self.pointing.clone(),
            Gesture::None => return None,
        };
        if right {
            hand.texture = self.idle.texture.clone();
        }
        Some(hand)
    }
}
impl HandSet {
    pub fn select(&self, gesture: Gesture) -> Option<FileKey> {
        match gesture {
            Gesture::Idle => Some(self.idle),
            Gesture::Fist => Some(self.fist),
            Gesture::Pointing => Some(self.pointing),
            Gesture::None => None,
        }
    }
}
impl TryFrom<i16> for Gesture {
    type Error = AvatarError;
    fn try_from(value: i16) -> Result<Self> {
        match value {
            0 => Ok(Self::Idle),
            1 => Ok(Self::Pointing),
            2 => Ok(Self::Fist),
            3 => Ok(Self::None),
            _ => Err(AvatarError::Invalid("gesture")),
        }
    }
}
impl TryFrom<u8> for Skin {
    type Error = AvatarError;
    fn try_from(value: u8) -> Result<Self> {
        match value {
            0 => Ok(Self::Light),
            1 => Ok(Self::Medium),
            2 => Ok(Self::Dark),
            _ => Err(AvatarError::Invalid("skin")),
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct AppearanceSelection {
    pub body: Option<FileKey>,
    pub head: Option<FileKey>,
    pub decorations: Vec<(PartRole, FileKey)>,
    pub accessories: Vec<FileKey>,
    pub skin: Skin,
    pub left: Gesture,
    pub right: Gesture,
}
#[derive(Clone, Debug)]
pub struct AppearancePart {
    pub role: PartRole,
    pub appearance: FileKey,
    pub binding: FileKey,
    pub mesh_resource: ResourceKey,
    pub texture_resource: ResourceKey,
    pub texture: AssetKey,
    pub mesh: Arc<PreparedMesh>,
}
#[derive(Clone, Debug)]
pub struct AppearanceBundle {
    pub rig_key: AssetKey,
    pub skin: Skin,
    pub parts: Vec<AppearancePart>,
}
#[derive(Clone, Debug, Default)]
pub struct AppearanceCatalog {
    pub outfits: BTreeMap<FileKey, Outfit>,
    pub appearances: BTreeMap<FileKey, Appearance>,
    pub bindings: BTreeMap<FileKey, Binding>,
    pub meshes: BTreeMap<ResourceKey, (AssetKey, Arc<SourceMesh>)>,
    pub textures: BTreeMap<ResourceKey, AssetKey>,
    pub hand_groups: BTreeMap<FileKey, HandGroup>,
}
impl AppearanceCatalog {
    fn outfit_appearance(&self, id: FileKey, skin: Skin) -> Result<FileKey> {
        let o = self
            .outfits
            .get(&id)
            .ok_or_else(|| AvatarError::MissingResource(format!("outfit {id:?}")))?;
        if o.version != 1 {
            return Err(AvatarError::Invalid("outfit version"));
        }
        Ok(match skin {
            Skin::Light => o.light_appearance,
            Skin::Medium => o.medium_appearance,
            Skin::Dark => o.dark_appearance,
        })
    }
    pub fn compose(
        &self,
        rig: &Rig,
        selection: &AppearanceSelection,
        limits: AvatarLimits,
    ) -> Result<AppearanceBundle> {
        if selection.decorations.len() + selection.accessories.len() > limits.max_parts {
            return Err(AvatarError::Limit("appearance roles"));
        }
        let mut roles = Vec::new();
        if let Some(body) = selection.body {
            roles.push((
                PartRole::Body,
                self.outfit_appearance(body, selection.skin)?,
            ));
        }
        if let Some(head) = selection.head {
            roles.push((
                PartRole::Head,
                self.outfit_appearance(head, selection.skin)?,
            ));
        }
        if let Some(body) = selection.body {
            if selection.left != Gesture::None || selection.right != Gesture::None {
                let file = self.outfits[&body].hand_group;
                let id = FileKey {
                    file_id: if file == 0 { 37 } else { file },
                    type_id: 18,
                };
                let hands = self
                    .hand_groups
                    .get(&id)
                    .ok_or_else(|| AvatarError::MissingResource(format!("hand group {id:?}")))?;
                let pair = &hands.skins[selection.skin as usize];
                if let Some(id) = pair.right.select(selection.right) {
                    roles.push((PartRole::RightHand, id));
                }
                if let Some(id) = pair.left.select(selection.left) {
                    roles.push((PartRole::LeftHand, id));
                }
            }
        }
        for &(role, id) in &selection.decorations {
            if !matches!(
                role,
                PartRole::HeadDecoration | PartRole::Back | PartRole::Shoes | PartRole::Tail
            ) {
                return Err(AvatarError::Invalid("decoration role"));
            }
            roles.push((role, self.outfit_appearance(id, selection.skin)?));
        }
        for &id in &selection.accessories {
            roles.push((PartRole::Accessory, id));
        }
        let mut seen = BTreeSet::new();
        roles.retain(|r| seen.insert(*r));
        let mut parts = Vec::new();
        let mut cache: BTreeMap<ResourceKey, Arc<PreparedMesh>> = BTreeMap::new();
        let mut bytes = 0usize;
        for (role, id) in roles {
            let appearance = self
                .appearances
                .get(&id)
                .ok_or_else(|| AvatarError::MissingResource(format!("appearance {id:?}")))?;
            if appearance.version != 1 {
                return Err(AvatarError::Invalid("appearance version"));
            }
            if appearance.bindings.len() > limits.max_parts.saturating_sub(parts.len()) {
                return Err(AvatarError::Limit("appearance bindings"));
            }
            for &binding_id in &appearance.bindings {
                let binding = self.bindings.get(&binding_id).ok_or_else(|| {
                    AvatarError::MissingResource(format!("binding {binding_id:?}"))
                })?;
                if binding.version != 1
                    || binding.mesh_selector != 8
                    || binding.texture_selector != 8
                {
                    return Err(AvatarError::Invalid(
                        "binding requires resolved mesh/texture",
                    ));
                }
                let mesh_id = binding
                    .mesh
                    .ok_or(AvatarError::Invalid("missing binding mesh"))?;
                let texture_id = binding
                    .texture
                    .ok_or(AvatarError::Invalid("missing binding texture"))?;
                let texture = *self.textures.get(&texture_id).ok_or_else(|| {
                    AvatarError::MissingResource(format!("texture {texture_id:?}"))
                })?;
                let prepared = match cache.get(&mesh_id) {
                    Some(mesh) => mesh.clone(),
                    None => {
                        let (key, source) = self.meshes.get(&mesh_id).ok_or_else(|| {
                            AvatarError::MissingResource(format!("mesh {mesh_id:?}"))
                        })?;
                        let mesh = Arc::new(PreparedMesh::prepare(rig, source, *key, limits)?);
                        bytes = bytes
                            .checked_add(mesh.resident_bytes())
                            .ok_or(AvatarError::Limit("appearance bytes"))?;
                        if bytes > limits.max_metadata_bytes {
                            return Err(AvatarError::Limit("appearance bytes"));
                        }
                        cache.insert(mesh_id, mesh.clone());
                        mesh
                    }
                };
                // .bnd Bone is provenance only: no second attachment transform.
                parts.push(AppearancePart {
                    role,
                    appearance: id,
                    binding: binding_id,
                    mesh_resource: mesh_id,
                    texture_resource: texture_id,
                    texture,
                    mesh: prepared,
                });
            }
        }
        Ok(AppearanceBundle {
            rig_key: rig.key,
            skin: selection.skin,
            parts,
        })
    }
}
impl AppearanceBundle {
    pub fn validate(&self) -> Result<()> {
        if self.parts.len() > AvatarLimits::default().max_parts {
            return Err(AvatarError::Limit("installed parts"));
        }
        for p in &self.parts {
            if p.mesh.rig_key != self.rig_key
                || p.mesh.indices.len() % 3 != 0
                || p.mesh
                    .indices
                    .iter()
                    .any(|i| *i as usize >= p.mesh.vertices.len())
                || p.mesh.vertices.iter().any(|v| {
                    !v.primary_position.is_finite()
                        || !v.secondary_position.is_finite()
                        || !v.primary_normal.is_finite()
                        || !v.secondary_normal.is_finite()
                        || !v.uv.is_finite()
                        || !v.weight.is_finite()
                })
            {
                return Err(AvatarError::Invalid("installed appearance"));
            }
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppearanceRequest {
    pub entity: EntityRef,
    pub serial: u64,
}
pub struct AppearanceState {
    entity: EntityRef,
    serial: u64,
    current: Option<AppearanceBundle>,
}
impl AppearanceState {
    pub fn new(entity: EntityRef) -> Self {
        Self {
            entity,
            serial: 0,
            current: None,
        }
    }
    pub fn request(&mut self) -> Result<AppearanceRequest> {
        if self.entity.generation == 0 {
            return Err(AvatarError::Invalid("appearance entity"));
        }
        self.serial = self
            .serial
            .checked_add(1)
            .ok_or(AvatarError::Limit("appearance request serial"))?;
        Ok(AppearanceRequest {
            entity: self.entity,
            serial: self.serial,
        })
    }
    pub fn install(&mut self, request: AppearanceRequest, bundle: AppearanceBundle) -> Result<()> {
        if request.entity != self.entity || request.serial != self.serial || self.serial == 0 {
            return Err(AvatarError::Stale);
        }
        bundle.validate()?;
        self.current = Some(bundle);
        Ok(())
    }
    pub fn current(&self) -> Option<&AppearanceBundle> {
        self.current.as_ref()
    }
    pub fn clear(&mut self) {
        self.current = None;
        self.serial = self.serial.saturating_add(1);
    }
    pub fn reset(&mut self, entity: EntityRef) {
        self.clear();
        self.entity = entity;
    }
}

impl HandGroup {
    /// B/provider normalized file order: skin, right then left, idle/fist/pointing.
    /// Numeric Gesture::Pointing=1 and Fist=2 must never index these references.
    pub fn from_source_refs(refs: &[FileKey]) -> Result<Self> {
        if refs.len() != 18 {
            return Err(AvatarError::Invalid("hand group requires 18 references"));
        }
        let pair = |skin: usize| {
            let first = skin * 6;
            HandPair {
                right: HandSet {
                    idle: refs[first],
                    fist: refs[first + 1],
                    pointing: refs[first + 2],
                },
                left: HandSet {
                    idle: refs[first + 3],
                    fist: refs[first + 4],
                    pointing: refs[first + 5],
                },
            }
        };
        Ok(Self {
            skins: [pair(0), pair(1), pair(2)],
        })
    }
}
