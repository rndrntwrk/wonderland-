//! Original file/FAR3 content to the pinned B decoder and C avatar renderer.
//! No preset, fallback geometry, second axis conversion, or binding bone transform.
#![forbid(unsafe_code)]
pub mod readers;
use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use wonderland_avatar_view as c;
pub use wonderland_avatar_view::{
    AppearanceCatalog, AppearanceSelection, AvatarLimits, FileKey, Gesture, PartRole, ResourceKey,
    Rig, Skin,
};
use wonderland_legacy_formats::{self as legacy, vitaboy as b};
use wonderland_render_core::{math::Mat4, RenderLimits};
pub use wonderland_render_core::{AssetKey, Mesh, RgbaImage};

#[derive(Clone, Copy, Debug)]
pub struct NamedBytes<'a> {
    pub name: &'a str,
    pub bytes: &'a [u8],
    /// Explicit identity for standalone resources. Otherwise only native
    /// name.<packed hexadecimal ID>.extension is recognized. FAR3 supplies IDs.
    pub key: Option<ResourceKey>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollectionRole {
    Head,
    Body,
}
#[derive(Clone, Debug)]
pub struct CollectionSpec {
    pub name: String,
    pub role: CollectionRole,
}
pub struct ImportRequest<'a> {
    pub files: Vec<NamedBytes<'a>>,
    pub skeleton_name: &'a str,
    /// Collection role is caller supplied; filenames do not establish game semantics.
    pub collections: Vec<CollectionSpec>,
}
#[derive(Clone, Copy, Debug)]
pub struct ImportLimits {
    pub resources: legacy::Limits,
    pub avatar: AvatarLimits,
    pub render: RenderLimits,
    pub max_files: usize,
    pub max_choices: usize,
    pub max_total_input_bytes: usize,
    pub max_total_resource_bytes: usize,
}
impl Default for ImportLimits {
    fn default() -> Self {
        Self {
            resources: legacy::Limits::default(),
            avatar: AvatarLimits::default(),
            render: RenderLimits::default(),
            max_files: 100_000,
            max_choices: 100_000,
            max_total_input_bytes: 1024 * 1024 * 1024,
            max_total_resource_bytes: 1024 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IssueKind {
    Missing,
    Ambiguous,
    Corrupt,
    Limit,
    Unsupported,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentIssue {
    pub kind: IssueKind,
    pub resource: String,
    pub detail: String,
}
impl ContentIssue {
    fn new(kind: IssueKind, resource: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            kind,
            resource: resource.into(),
            detail: detail.into(),
        }
    }
}
impl std::fmt::Display for ContentIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}: {}", self.kind, self.resource, self.detail)
    }
}
impl std::error::Error for ContentIssue {}
#[derive(Clone, Debug)]
pub struct ChoiceReadiness {
    pub ready: bool,
    pub issues: Vec<ContentIssue>,
}
#[derive(Clone, Debug)]
pub struct CollectionChoice {
    pub key: String,
    pub collection: String,
    pub index: i32,
    pub role: CollectionRole,
    pub purchasable: FileKey,
    pub outfit: Option<FileKey>,
    pub gender: Option<u32>,
    /// Source .apr thumbnail references, separate from the UV texture.
    pub thumbnail_keys: [Option<FileKey>; 3],
    pub thumbnails: [Option<AssetKey>; 3],
    pub skins: [ChoiceReadiness; 3],
}
#[derive(Clone, Debug)]
pub struct EncodedTexture {
    pub name: String,
    pub mime: &'static str,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug)]
pub struct RenderablePart {
    pub role: PartRole,
    pub mesh: Mesh,
    pub texture: AssetKey,
}
pub struct ImportedContent {
    pub rig: Option<Rig>,
    pub catalog: AppearanceCatalog,
    pub choices: Vec<CollectionChoice>,
    pub textures: BTreeMap<AssetKey, EncodedTexture>,
    pub issues: Vec<ContentIssue>,
    limits: ImportLimits,
}
pub fn file_content_key(key: FileKey) -> String {
    format!("vitaboy:{:016x}", key.packed())
}
/// FileProvider.cs FAR3IDs convention, rather than a filename-to-avatar mapping.
/// Original packed hex IDs occur in the penultimate dot-separated component.
pub fn source_filename_key(name: &str) -> Option<ResourceKey> {
    let basename = name.rsplit(['/', '\\']).next()?;
    let parts: Vec<_> = basename.split('.').collect();
    if parts.len() < 3 {
        return None;
    }
    let value = parts[parts.len() - 2];
    if value.is_empty() || value.len() > 16 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let packed = u64::from_str_radix(value, 16).ok()?;
    Some(ResourceKey {
        group_id: 0,
        file_id: (packed >> 32) as u32,
        type_id: packed as u32,
    })
}
/// B and C normalized DTOs share exact serde field/enum/bit schemas. This
/// converter copies integer binary32 bits; B has already applied FreeSO policy.
fn dto<T: Serialize, U: DeserializeOwned>(value: T) -> Result<U, ContentIssue> {
    serde_json::to_value(value)
        .and_then(serde_json::from_value)
        .map_err(|e| ContentIssue::new(IssueKind::Corrupt, "B/C normalized DTO", e.to_string()))
}
fn hash(bytes: &[u8]) -> AssetKey {
    AssetKey(Sha256::digest(bytes).into())
}
fn error(name: &str, err: legacy::Error) -> ContentIssue {
    let kind = match err.kind {
        legacy::ErrorKind::LimitExceeded | legacy::ErrorKind::Overflow => IssueKind::Limit,
        legacy::ErrorKind::UnsupportedVersion => IssueKind::Unsupported,
        _ => IssueKind::Corrupt,
    };
    ContentIssue::new(kind, name, err.to_string())
}
fn avatar_error(name: &str, err: c::AvatarError) -> ContentIssue {
    let kind = match err {
        c::AvatarError::MissingResource(_) | c::AvatarError::MissingBone(_) => IssueKind::Missing,
        c::AvatarError::Limit(_) => IssueKind::Limit,
        _ => IssueKind::Corrupt,
    };
    ContentIssue::new(kind, name, err.to_string())
}
fn suffix(name: &str) -> String {
    name.rsplit('.').next().unwrap_or("").to_ascii_lowercase()
}
#[derive(Clone, Debug)]
pub struct ResourceDescriptor {
    pub name: String,
    pub key: Option<ResourceKey>,
    pub kind: ResourceKind,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceKind {
    Skeleton,
    Collection,
    Purchasable,
    Outfit,
    Appearance,
    Binding,
    HandGroup,
    Mesh,
    Texture,
    Other,
}
fn kind(name: &str) -> ResourceKind {
    match suffix(name).as_str() {
        "skel" => ResourceKind::Skeleton,
        "co" | "col" => ResourceKind::Collection,
        "po" | "pof" | "purchasable" => ResourceKind::Purchasable,
        "oft" => ResourceKind::Outfit,
        "apr" => ResourceKind::Appearance,
        "bnd" => ResourceKind::Binding,
        "hag" | "hnd" => ResourceKind::HandGroup,
        "mesh" => ResourceKind::Mesh,
        "png" | "jpg" | "jpeg" => ResourceKind::Texture,
        _ => ResourceKind::Other,
    }
}
/// Bounded metadata inventory; FAR3 payloads remain compressed/unextracted.
/// This lets a browser select original named collections inside archives.
pub fn inventory(
    files: &[NamedBytes<'_>],
    limits: &ImportLimits,
) -> Result<Vec<ResourceDescriptor>, ContentIssue> {
    if files.len() > limits.max_files {
        return Err(ContentIssue::new(
            IssueKind::Limit,
            "inventory",
            "source file count",
        ));
    }
    let mut total = 0usize;
    let mut result = Vec::new();
    for file in files {
        total = total
            .checked_add(file.bytes.len())
            .ok_or_else(|| ContentIssue::new(IssueKind::Limit, file.name, "input byte overflow"))?;
        if total > limits.max_total_input_bytes {
            return Err(ContentIssue::new(
                IssueKind::Limit,
                file.name,
                "total input bytes",
            ));
        }
        limits
            .resources
            .check_input(file.bytes)
            .map_err(|e| error(file.name, e))?;
        if file.bytes.starts_with(b"FAR!byAZ") {
            let index = legacy::far::index_v3(file.bytes, &limits.resources)
                .map_err(|e| error(file.name, e))?;
            if index.entries().len() > limits.max_files.saturating_sub(result.len()) {
                return Err(ContentIssue::new(
                    IssueKind::Limit,
                    file.name,
                    "inventory resource count",
                ));
            }
            for entry in index.entries() {
                let legacy::ResourceKey::Far3 { file_id, type_id } = entry.key else {
                    unreachable!("FAR3 index")
                };
                let name = std::str::from_utf8(entry.name.as_deref().unwrap_or_default())
                    .map_err(|_| {
                        ContentIssue::new(
                            IssueKind::Unsupported,
                            file.name,
                            "non UTF-8 archive resource name",
                        )
                    })?
                    .to_owned();
                result.push(ResourceDescriptor {
                    kind: kind(&name),
                    name,
                    key: Some(ResourceKey {
                        group_id: 0,
                        file_id,
                        type_id,
                    }),
                });
            }
        } else {
            if result.len() >= limits.max_files {
                return Err(ContentIssue::new(
                    IssueKind::Limit,
                    file.name,
                    "inventory resource count",
                ));
            }
            result.push(ResourceDescriptor {
                name: file.name.into(),
                key: file.key.or_else(|| source_filename_key(file.name)),
                kind: kind(file.name),
            });
        }
    }
    Ok(result)
}
fn encoded(r: &Resource) -> Result<EncodedTexture, ContentIssue> {
    let mime = if r.bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if r.bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        "image/jpeg"
    } else {
        return Err(ContentIssue::new(
            IssueKind::Unsupported,
            &r.name,
            "texture must be original PNG/JPEG bytes for browser decoder",
        ));
    };
    Ok(EncodedTexture {
        name: r.name.clone(),
        mime,
        bytes: r.bytes.clone(),
    })
}
struct Resource {
    name: String,
    bytes: Vec<u8>,
    key: Option<ResourceKey>,
}
struct ResourceIndex<'a> {
    files: BTreeMap<FileKey, Vec<&'a Resource>>,
    names: BTreeMap<&'a str, Vec<&'a Resource>>,
}
impl<'a> ResourceIndex<'a> {
    fn new(all: &'a [Resource]) -> Self {
        let mut files: BTreeMap<FileKey, Vec<&'a Resource>> = BTreeMap::new();
        let mut names: BTreeMap<&str, Vec<&'a Resource>> = BTreeMap::new();
        for r in all {
            names.entry(&r.name).or_default().push(r);
            if let Some(k) = file_key(r) {
                files.entry(k).or_default().push(r);
            }
        }
        Self { files, names }
    }
}
fn resolve<'a>(all: &ResourceIndex<'a>, key: ResourceKey) -> Result<&'a Resource, ContentIssue> {
    // Avatar.cs and FileProvider resolve both FAR and standalone content by
    // TypeID/FileID. Retain GroupID in the binding DTO without using it as a
    // second identity namespace on this TSO avatar path.
    match all
        .files
        .get(&FileKey {
            file_id: key.file_id,
            type_id: key.type_id,
        })
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        [] => Err(ContentIssue::new(
            IssueKind::Missing,
            format!("{key:?}"),
            "original resource not supplied",
        )),
        [one] => Ok(one),
        _ => Err(ContentIssue::new(
            IssueKind::Ambiguous,
            format!("{key:?}"),
            "multiple supplied resources have this identity; no precedence selected",
        )),
    }
}
fn by_name<'a>(all: &ResourceIndex<'a>, name: &str) -> Result<&'a Resource, ContentIssue> {
    match all.names.get(name).map(Vec::as_slice).unwrap_or_default() {
        [one] => Ok(one),
        [] => Err(ContentIssue::new(
            IssueKind::Missing,
            name,
            "named original file not supplied",
        )),
        _ => Err(ContentIssue::new(
            IssueKind::Ambiguous,
            name,
            "duplicate original filenames",
        )),
    }
}
fn file_key(r: &Resource) -> Option<FileKey> {
    r.key.map(|k| FileKey {
        file_id: k.file_id,
        type_id: k.type_id,
    })
}

pub fn import(
    request: ImportRequest<'_>,
    limits: &ImportLimits,
) -> Result<ImportedContent, ContentIssue> {
    if request.files.len() > limits.max_files || request.collections.len() > limits.max_files {
        return Err(ContentIssue::new(
            IssueKind::Limit,
            "content set",
            "file/collection count",
        ));
    }
    let mut input_total = 0usize;
    let mut resource_total = 0usize;
    let mut resources = Vec::new();
    for file in request.files {
        input_total = input_total
            .checked_add(file.bytes.len())
            .ok_or_else(|| ContentIssue::new(IssueKind::Limit, file.name, "input byte overflow"))?;
        if input_total > limits.max_total_input_bytes {
            return Err(ContentIssue::new(
                IssueKind::Limit,
                file.name,
                "total input bytes",
            ));
        }
        limits
            .resources
            .check_input(file.bytes)
            .map_err(|e| error(file.name, e))?;
        if file.bytes.starts_with(b"FAR!byAZ") {
            let index = legacy::far::index_v3(file.bytes, &limits.resources)
                .map_err(|e| error(file.name, e))?;
            for (i, entry) in index.entries().iter().enumerate() {
                resource_total =
                    resource_total
                        .checked_add(entry.decoded_len)
                        .ok_or_else(|| {
                            ContentIssue::new(IssueKind::Limit, file.name, "decoded byte overflow")
                        })?;
                if resource_total > limits.max_total_resource_bytes
                    || resources.len() >= limits.max_files
                {
                    return Err(ContentIssue::new(
                        IssueKind::Limit,
                        file.name,
                        "total decoded bytes/resource count",
                    ));
                }
                let legacy::ResourceKey::Far3 { type_id, file_id } = entry.key else {
                    unreachable!("FAR3 index")
                };
                let name = std::str::from_utf8(entry.name.as_deref().unwrap_or_default())
                    .map_err(|_| {
                        ContentIssue::new(
                            IssueKind::Unsupported,
                            file.name,
                            "non UTF-8 archive resource name",
                        )
                    })?
                    .to_owned();
                resources.push(Resource {
                    name,
                    bytes: index
                        .extract(i, &limits.resources)
                        .map_err(|e| error(file.name, e))?,
                    key: Some(ResourceKey {
                        group_id: 0,
                        file_id,
                        type_id,
                    }),
                });
            }
        } else {
            limits
                .resources
                .check_count(
                    file.bytes.len(),
                    limits.resources.max_resource_bytes,
                    0,
                    "resource bytes",
                )
                .map_err(|e| error(file.name, e))?;
            resource_total = resource_total
                .checked_add(file.bytes.len())
                .ok_or_else(|| {
                    ContentIssue::new(IssueKind::Limit, file.name, "resource byte overflow")
                })?;
            if resource_total > limits.max_total_resource_bytes
                || resources.len() >= limits.max_files
            {
                return Err(ContentIssue::new(
                    IssueKind::Limit,
                    file.name,
                    "total resource bytes/count",
                ));
            }
            resources.push(Resource {
                name: file.name.to_owned(),
                bytes: file.bytes.to_vec(),
                key: file.key.or_else(|| source_filename_key(file.name)),
            });
        }
    }
    let mut content = ImportedContent {
        rig: None,
        catalog: AppearanceCatalog::default(),
        choices: Vec::new(),
        textures: BTreeMap::new(),
        issues: Vec::new(),
        limits: *limits,
    };
    let index = ResourceIndex::new(&resources);
    match by_name(&index, request.skeleton_name).and_then(|r| {
        b::decode_skeleton(&r.bytes, &limits.resources)
            .map_err(|e| error(&r.name, e))
            .and_then(dto)
            .and_then(|s| {
                Rig::new(s, hash(&r.bytes), limits.avatar).map_err(|e| avatar_error(&r.name, e))
            })
    }) {
        Ok(rig) => content.rig = Some(rig),
        Err(e) => content.issues.push(e),
    }
    let mut purchasables = BTreeMap::new();
    let mut counts = BTreeMap::<FileKey, usize>::new();
    for r in &resources {
        if let Some(k) = file_key(r) {
            *counts.entry(k).or_default() += 1;
        }
    }
    for r in &resources {
        let ext = suffix(&r.name);
        if !matches!(
            ext.as_str(),
            "oft" | "apr" | "bnd" | "hag" | "hnd" | "po" | "pof" | "purchasable"
        ) {
            continue;
        }
        let Some(k) = file_key(r) else {
            content.issues.push(ContentIssue::new(
                IssueKind::Missing,
                &r.name,
                "standalone resource requires its original FileKey identity",
            ));
            continue;
        };
        if counts[&k] != 1 {
            content.issues.push(ContentIssue::new(
                IssueKind::Ambiguous,
                &r.name,
                format!("duplicate FileKey {k:?}"),
            ));
            continue;
        }
        let result: Result<(), ContentIssue> = (|| {
            match ext.as_str() {
                "oft" => {
                    content.catalog.outfits.insert(
                        k,
                        dto(b::decode_outfit(&r.bytes, &limits.resources)
                            .map_err(|e| error(&r.name, e))?)?,
                    );
                }
                "apr" => {
                    content.catalog.appearances.insert(
                        k,
                        dto(b::decode_appearance(&r.bytes, &limits.resources)
                            .map_err(|e| error(&r.name, e))?)?,
                    );
                }
                "bnd" => {
                    content.catalog.bindings.insert(
                        k,
                        dto(b::decode_binding(&r.bytes, &limits.resources)
                            .map_err(|e| error(&r.name, e))?)?,
                    );
                }
                "hag" | "hnd" => {
                    content.catalog.hand_groups.insert(
                        k,
                        readers::decode_hand_group(&r.bytes, &limits.resources)
                            .map_err(|e| error(&r.name, e))?,
                    );
                }
                _ => {
                    purchasables.insert(
                        k,
                        readers::decode_purchasable(&r.bytes, &limits.resources)
                            .map_err(|e| error(&r.name, e))?,
                    );
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            content.issues.push(e);
        }
    }
    let mesh_keys: BTreeSet<_> = content
        .catalog
        .bindings
        .values()
        .filter_map(|b| b.mesh)
        .collect();
    for key in mesh_keys {
        match resolve(&index, key).and_then(|r| {
            b::decode_mesh(&r.bytes, &limits.resources)
                .map_err(|e| error(&r.name, e))
                .and_then(dto)
                .map(|mesh| (hash(&r.bytes), Arc::new(mesh)))
        }) {
            Ok(mesh) => {
                content.catalog.meshes.insert(key, mesh);
            }
            Err(e) => content.issues.push(e),
        }
    }
    let texture_keys: BTreeSet<_> = content
        .catalog
        .bindings
        .values()
        .filter_map(|b| b.texture)
        .collect();
    for key in texture_keys {
        let result =
            resolve(&index, key).and_then(|r| encoded(r).map(|texture| (hash(&r.bytes), texture)));
        match result {
            Ok((asset, texture)) => {
                content.catalog.textures.insert(key, asset);
                content.textures.insert(asset, texture);
            }
            Err(e) => content.issues.push(e),
        }
    }
    let thumbnail_keys: BTreeSet<_> = content
        .catalog
        .appearances
        .values()
        .map(|a| a.thumbnail)
        .filter(|k| k.packed() != 0)
        .collect();
    let mut thumbnail_assets = BTreeMap::new();
    for key in thumbnail_keys {
        let resource = ResourceKey {
            group_id: 0,
            file_id: key.file_id,
            type_id: key.type_id,
        };
        match resolve(&index, resource)
            .and_then(|r| encoded(r).map(|texture| (hash(&r.bytes), texture)))
        {
            Ok((asset, texture)) => {
                content.textures.insert(asset, texture);
                thumbnail_assets.insert(key, asset);
            }
            Err(e) => content.issues.push(ContentIssue {
                resource: format!("thumbnail {}", e.resource),
                ..e
            }),
        }
    }
    let mut selected_collections = BTreeSet::new();
    for spec in request.collections {
        if !selected_collections.insert(spec.name.clone()) {
            return Err(ContentIssue::new(
                IssueKind::Ambiguous,
                &spec.name,
                "collection selected more than once",
            ));
        }
        let items = match by_name(&index, &spec.name).and_then(|r| {
            readers::decode_collection(&r.bytes, &limits.resources).map_err(|e| error(&r.name, e))
        }) {
            Ok(items) => items,
            Err(e) => {
                content.issues.push(e);
                continue;
            }
        };
        if items.len() > limits.max_choices.saturating_sub(content.choices.len()) {
            return Err(ContentIssue::new(
                IssueKind::Limit,
                &spec.name,
                "aggregate original collection choice count",
            ));
        }
        for item in items {
            let p = purchasables.get(&item.purchasable);
            let outfit = p.map(|p| p.outfit);
            let thumbnail_keys = std::array::from_fn(|i| {
                outfit
                    .and_then(|k| content.catalog.outfits.get(&k))
                    .and_then(|o| {
                        content
                            .catalog
                            .appearances
                            .get(&[o.light_appearance, o.medium_appearance, o.dark_appearance][i])
                    })
                    .map(|a| a.thumbnail)
                    .filter(|k| k.packed() != 0)
            });
            let thumbnails =
                thumbnail_keys.map(|key| key.and_then(|k| thumbnail_assets.get(&k).copied()));
            let skins = std::array::from_fn(|i| {
                let mut issues = Vec::new();
                if let Some(outfit) = outfit {
                    let mut selection = AppearanceSelection {
                        skin: Skin::try_from(i as u8).expect("three skins"),
                        ..Default::default()
                    };
                    match spec.role {
                        CollectionRole::Head => {
                            selection.head = Some(outfit);
                            selection.left = Gesture::None;
                            selection.right = Gesture::None;
                        }
                        CollectionRole::Body => selection.body = Some(outfit),
                    }
                    issues.extend(content.selection_issues(&selection));
                    if issues.is_empty() {
                        if let Err(e) = content.compose(&selection) {
                            issues.extend(e);
                        }
                    }
                } else {
                    issues.push(ContentIssue::new(
                        IssueKind::Missing,
                        file_content_key(item.purchasable),
                        "collection purchasable resource missing, ambiguous, or corrupt",
                    ));
                }
                ChoiceReadiness {
                    ready: issues.is_empty(),
                    issues,
                }
            });
            content.choices.push(CollectionChoice {
                key: file_content_key(item.purchasable),
                collection: spec.name.clone(),
                index: item.index,
                role: spec.role,
                purchasable: item.purchasable,
                outfit,
                gender: p.map(|p| p.gender),
                thumbnail_keys,
                thumbnails,
                skins,
            });
        }
    }
    if content.choices.is_empty() {
        content.issues.push(ContentIssue::new(
            IssueKind::Missing,
            "collections",
            "no original collection choices resolved",
        ));
    }
    Ok(content)
}

impl ImportedContent {
    pub fn selection_issues(&self, selection: &AppearanceSelection) -> Vec<ContentIssue> {
        let mut issues = Vec::new();
        if self.rig.is_none() {
            issues.push(ContentIssue::new(
                IssueKind::Missing,
                "skeleton",
                "valid original skeleton required",
            ));
        }
        let mut appearances = selection.accessories.clone();
        let outfits = selection
            .head
            .into_iter()
            .chain(selection.body)
            .chain(selection.decorations.iter().map(|(_, k)| *k));
        for key in outfits {
            match self.catalog.outfits.get(&key) {
                Some(o) => appearances.push(match selection.skin {
                    Skin::Light => o.light_appearance,
                    Skin::Medium => o.medium_appearance,
                    Skin::Dark => o.dark_appearance,
                }),
                None => issues.push(ContentIssue::new(
                    IssueKind::Missing,
                    format!("outfit {key:?}"),
                    "outfit required by selection",
                )),
            }
        }
        if let Some(o) = selection.body.and_then(|k| self.catalog.outfits.get(&k)) {
            if selection.left != Gesture::None || selection.right != Gesture::None {
                let key = FileKey {
                    file_id: if o.hand_group == 0 { 37 } else { o.hand_group },
                    type_id: 18,
                };
                match self.catalog.hand_groups.get(&key) {
                    Some(g) => {
                        let pair = &g.skins[selection.skin as usize];
                        appearances.extend(pair.left.select(selection.left));
                        appearances.extend(pair.right.select(selection.right));
                    }
                    None => issues.push(ContentIssue::new(
                        IssueKind::Missing,
                        format!("hand group {key:?}"),
                        "body gestures require original hand group",
                    )),
                }
            }
        }
        for key in appearances {
            let Some(a) = self.catalog.appearances.get(&key) else {
                issues.push(ContentIssue::new(
                    IssueKind::Missing,
                    format!("appearance {key:?}"),
                    "selected skin/gesture appearance",
                ));
                continue;
            };
            if a.bindings.is_empty() {
                issues.push(ContentIssue::new(
                    IssueKind::Corrupt,
                    format!("appearance {key:?}"),
                    "empty appearance has no rendered parts",
                ));
            }
            for binding in &a.bindings {
                let Some(b) = self.catalog.bindings.get(binding) else {
                    issues.push(ContentIssue::new(
                        IssueKind::Missing,
                        format!("binding {binding:?}"),
                        "appearance binding",
                    ));
                    continue;
                };
                if b.mesh.is_none_or(|k| !self.catalog.meshes.contains_key(&k)) {
                    issues.push(ContentIssue::new(
                        IssueKind::Missing,
                        format!("mesh {:?}", b.mesh),
                        "binding mesh",
                    ));
                }
                if b.texture
                    .is_none_or(|k| !self.catalog.textures.contains_key(&k))
                {
                    issues.push(ContentIssue::new(
                        IssueKind::Missing,
                        format!("texture {:?}", b.texture),
                        "binding texture",
                    ));
                }
            }
        }
        issues
    }
    /// Actual C geometry, already normalized once by B. Texture bytes are
    /// supplied separately, keyed by each part's texture AssetKey.
    pub fn compose(
        &self,
        selection: &AppearanceSelection,
    ) -> Result<Vec<RenderablePart>, Vec<ContentIssue>> {
        let issues = self.selection_issues(selection);
        if !issues.is_empty() {
            return Err(issues);
        }
        let rig = self.rig.as_ref().expect("readiness checked");
        let bundle = self
            .catalog
            .compose(rig, selection, self.limits.avatar)
            .map_err(|e| vec![avatar_error("appearance composition", e)])?;
        if bundle.parts.is_empty() {
            return Err(vec![ContentIssue::new(
                IssueKind::Missing,
                "selection",
                "no head/body/parts selected",
            )]);
        }
        let pose = rig.bind_pose();
        bundle
            .parts
            .iter()
            .map(|p| {
                p.mesh
                    .skin(&pose, Mat4::IDENTITY)
                    .map(|mesh| RenderablePart {
                        role: p.role,
                        mesh,
                        texture: p.texture,
                    })
                    .map_err(|e| vec![avatar_error("mesh skinning", e)])
            })
            .collect()
    }
    /// Browser/native decoders must return unpremultiplied RGBA, original pixel
    /// order and original UV orientation; no flip/axis transform occurs here.
    pub fn validate_decoded_texture(
        &self,
        key: AssetKey,
        image: &RgbaImage,
    ) -> Result<(), ContentIssue> {
        if !self.textures.contains_key(&key) {
            return Err(ContentIssue::new(
                IssueKind::Missing,
                "texture asset",
                "decoder result has unknown source identity",
            ));
        }
        image
            .validate(&self.limits.render)
            .map_err(|e| ContentIssue::new(IssueKind::Corrupt, "decoded texture", e.to_string()))
    }
    pub fn ready_for(&self, selection: &AppearanceSelection) -> bool {
        self.compose(selection).is_ok()
    }
}
