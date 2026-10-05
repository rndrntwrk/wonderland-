// SPDX-License-Identifier: MPL-2.0
//! Portable authoring sessions for non-IFF Creator workbenches.
use serde::Serialize;
use serde_json::{json, Value};
pub use wonderland_creator::city::{CityImage, MapLayer};
pub use wonderland_creator::editors::assets::AssetKind;
use wonderland_creator::{
    city::NeighborhoodDocument,
    editors::{
        assets::AssetDocument, gltf::GltfPackage, meshes::MeshOverrideDocument,
        upgrades::UpgradeDocument,
    },
    json_support::{self, JsonEdit},
    patch_view::{PatchInput, PatchView},
    sha256, ResourceDocument,
};
use wonderland_legacy_formats::{iff, reconstruction, vitaboy, Limits};

pub const MAX_WORKBENCH_FILE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_WORKBENCH_HISTORY_BYTES: usize = 8 * 1024 * 1024;
const MAX_HISTORY_ENTRIES: usize = 32;
const MAX_ATTACHMENTS: usize = 32;

pub fn workbench_limits() -> Limits {
    Limits {
        max_input_bytes: MAX_WORKBENCH_FILE_BYTES,
        max_resource_bytes: MAX_WORKBENCH_FILE_BYTES,
        max_total_decoded_bytes: 32 * 1024 * 1024,
        max_entries: 4096,
        max_string_bytes: 64 * 1024,
        max_pixels: 1024 * 1024,
        ..wonderland_creator::default_limits()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkbenchMode {
    Upgrades,
    City,
    Neighborhood,
    Assets,
    Patches,
}
impl WorkbenchMode {
    pub fn title(self) -> &'static str {
        match self {
            Self::Upgrades => "Upgrade editor",
            Self::City => "City painter",
            Self::Neighborhood => "Neighborhood editor",
            Self::Assets => "Asset workbench",
            Self::Patches => "Patch source view",
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Upgrades => "upgrades",
            Self::City => "city",
            Self::Neighborhood => "neighborhood",
            Self::Assets => "assets",
            Self::Patches => "patches",
        }
    }
    fn accepts(self, kind: InputKind) -> bool {
        matches!(
            (self, kind),
            (Self::Upgrades, InputKind::Upgrades)
                | (Self::City, InputKind::City)
                | (Self::Neighborhood, InputKind::Neighborhood)
                | (Self::Assets, InputKind::Asset(_))
                | (Self::Patches, InputKind::PatchSource)
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKind {
    Upgrades,
    City,
    Neighborhood,
    Asset(AssetKind),
    PatchSource,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExchangeFormat {
    Obj,
    Glb,
    Gltf,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Source,
    Json,
    Png,
    Bmp,
    Obj,
    Mtl,
    Glb,
    Gltf,
    EffectiveIff,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadTarget {
    Source(InputKind),
    Skeleton,
    Patch { is_user: bool },
    Exchange(ExchangeFormat),
}

pub struct NeighborhoodFields {
    pub index: usize,
    pub guid: String,
    pub name: String,
    pub description: String,
    pub location: [i32; 2],
}

pub struct Download {
    pub name: String,
    pub mime: &'static str,
    pub bytes: Vec<u8>,
}
struct Snapshot {
    name: String,
    bytes: Vec<u8>,
    hash: String,
}
impl Snapshot {
    fn new(name: &str, bytes: &[u8]) -> Result<Self, String> {
        let name = filename(name)?;
        Ok(Self {
            name,
            bytes: bytes.to_vec(),
            hash: sha256(bytes),
        })
    }
}
struct Revision {
    snapshot: Snapshot,
    label: String,
}
struct Attachment {
    snapshot: Snapshot,
    is_user: bool,
}
struct PendingRead {
    generation: u64,
    size: usize,
    target: ReadTarget,
    source_hash: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AttachmentRow {
    pub name: String,
    pub sha256: String,
    pub bytes: usize,
    pub is_user: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct HistoryRow {
    pub label: String,
    pub before_sha256: String,
}

pub struct WorkbenchSession {
    mode: WorkbenchMode,
    kind: Option<InputKind>,
    current: Option<Snapshot>,
    imported_hash: String,
    source_name: String,
    skeleton: Option<Snapshot>,
    patches: Vec<Attachment>,
    undo: Vec<Revision>,
    redo: Vec<Revision>,
    read_generation: u64,
    pending: Option<PendingRead>,
}
fn filename(name: &str) -> Result<String, String> {
    if name.is_empty() || name.len() > 256 || name.chars().any(char::is_control) {
        return Err("filename must contain 1–256 visible bytes".into());
    }
    let name = name.rsplit(['/', '\\']).next().unwrap_or_default();
    if name.is_empty() || matches!(name, "." | "..") {
        return Err("invalid filename".into());
    }
    Ok(name.to_owned())
}
fn validate(kind: InputKind, bytes: &[u8], limits: &Limits) -> Result<(), String> {
    match kind {
        InputKind::Upgrades => {
            UpgradeDocument::import(bytes, limits)?;
        }
        InputKind::Neighborhood => {
            NeighborhoodDocument::import(bytes, limits)?;
        }
        InputKind::City => {
            CityImage::decode(bytes, limits)?;
        }
        InputKind::Asset(kind) => {
            AssetDocument::import(kind, bytes, limits)?;
        }
        InputKind::PatchSource => {
            ResourceDocument::import(bytes, limits)?;
        }
    }
    Ok(())
}
impl WorkbenchSession {
    pub fn new(mode: WorkbenchMode) -> Self {
        Self {
            mode,
            kind: None,
            current: None,
            imported_hash: String::new(),
            source_name: String::new(),
            skeleton: None,
            patches: vec![],
            undo: vec![],
            redo: vec![],
            read_generation: 0,
            pending: None,
        }
    }
    pub fn loaded(&self) -> bool {
        self.current.is_some()
    }
    pub fn kind(&self) -> Option<InputKind> {
        self.kind
    }
    pub fn filename(&self) -> &str {
        self.current.as_ref().map(|s| s.name.as_str()).unwrap_or("")
    }
    pub fn source_sha256(&self) -> &str {
        self.current.as_ref().map(|s| s.hash.as_str()).unwrap_or("")
    }
    pub fn source_name(&self) -> &str {
        &self.source_name
    }
    pub fn dirty(&self) -> bool {
        self.loaded()
            && (self.source_sha256() != self.imported_hash
                || self.skeleton.is_some()
                || !self.patches.is_empty()
                || (self.mode == WorkbenchMode::Patches && self.source_name != self.filename()))
    }
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }
    pub fn history_bytes(&self) -> usize {
        self.undo
            .iter()
            .chain(&self.redo)
            .map(|r| r.snapshot.bytes.capacity())
            .sum()
    }
    pub fn history(&self) -> Vec<HistoryRow> {
        self.undo
            .iter()
            .rev()
            .map(|r| HistoryRow {
                label: r.label.clone(),
                before_sha256: r.snapshot.hash.clone(),
            })
            .collect()
    }
    pub fn attachments(&self) -> Vec<AttachmentRow> {
        self.patches
            .iter()
            .map(|p| AttachmentRow {
                name: p.snapshot.name.clone(),
                sha256: p.snapshot.hash.clone(),
                bytes: p.snapshot.bytes.len(),
                is_user: p.is_user,
            })
            .collect()
    }
    pub fn skeleton_name(&self) -> Option<&str> {
        self.skeleton.as_ref().map(|s| s.name.as_str())
    }
    pub fn skeleton_sha256(&self) -> Option<&str> {
        self.skeleton.as_ref().map(|s| s.hash.as_str())
    }
    fn source(&self) -> Result<&Snapshot, String> {
        self.current
            .as_ref()
            .ok_or("open a source file first".into())
    }
    fn retained_bytes(&self) -> usize {
        let snapshot = |s: &Snapshot| s.bytes.capacity() + s.name.capacity() + s.hash.capacity();
        // Reserve the graphical pane's bounded text/forms and image blob/surface
        // in the same per-panel budget as its portable model operations.
        (if self.mode == WorkbenchMode::City {
            8
        } else {
            2
        }) * 1024
            * 1024
            + self.current.as_ref().map(snapshot).unwrap_or(0)
            + self.skeleton.as_ref().map(snapshot).unwrap_or(0)
            + self
                .patches
                .iter()
                .map(|p| snapshot(&p.snapshot))
                .sum::<usize>()
            + self
                .undo
                .iter()
                .chain(&self.redo)
                .map(|r| snapshot(&r.snapshot) + r.label.capacity())
                .sum::<usize>()
    }
    fn operation_limits(&self, extra: usize) -> Result<Limits, String> {
        let mut limits = workbench_limits();
        limits.max_total_decoded_bytes = limits
            .max_total_decoded_bytes
            .checked_sub(self.retained_bytes())
            .and_then(|n| n.checked_sub(extra))
            .ok_or("workbench aggregate memory limit exceeded")?;
        Ok(limits)
    }
    fn guard(&self, expected: &str) -> Result<(), String> {
        if !expected.eq_ignore_ascii_case(&self.source()?.hash) {
            return Err("source SHA-256 conflict; inspect this revision again".into());
        }
        Ok(())
    }
    pub fn begin_read(&mut self, target: ReadTarget, size: usize) -> Result<u64, String> {
        if size > MAX_WORKBENCH_FILE_BYTES {
            return Err("file exceeds the 4 MiB workbench limit".into());
        }
        match target {
            ReadTarget::Source(kind) if self.mode.accepts(kind) => (),
            ReadTarget::Source(_) => {
                return Err("source format belongs to another workbench".into())
            }
            ReadTarget::Skeleton if self.kind == Some(InputKind::Asset(AssetKind::Animation)) => (),
            ReadTarget::Patch { .. } if self.kind == Some(InputKind::PatchSource) => (),
            ReadTarget::Exchange(ExchangeFormat::Obj)
                if self.kind == Some(InputKind::Asset(AssetKind::Fsom)) => {}
            ReadTarget::Exchange(ExchangeFormat::Glb | ExchangeFormat::Gltf)
                if matches!(
                    self.kind,
                    Some(InputKind::Asset(AssetKind::Fsom | AssetKind::Animation))
                ) => {}
            _ => return Err("this attachment does not match the open source format".into()),
        }
        self.operation_limits(size.checked_mul(2).ok_or("upload byte count overflow")?)?;
        self.read_generation = self
            .read_generation
            .checked_add(1)
            .ok_or("upload generation exhausted")?;
        self.pending = Some(PendingRead {
            generation: self.read_generation,
            size,
            target,
            source_hash: self.source_sha256().to_owned(),
        });
        Ok(self.read_generation)
    }
    pub fn is_current_read(&self, generation: u64) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|p| p.generation == generation)
    }
    pub fn cancel_read(&mut self, generation: u64) {
        if self.is_current_read(generation) {
            self.pending = None;
        }
    }
    pub fn complete_read(
        &mut self,
        generation: u64,
        name: &str,
        bytes: &[u8],
    ) -> Result<bool, String> {
        let pending = self.pending.as_ref().ok_or("no pending upload")?;
        if pending.generation != generation {
            return Err("upload was superseded".into());
        }
        if pending.size != bytes.len() {
            self.pending = None;
            return Err("upload size changed during reading".into());
        }
        let pending = self.pending.take().expect("pending upload checked");
        filename(name)?;
        if pending.source_hash != self.source_sha256() {
            return Err("source changed while attachment was reading".into());
        }
        let limits = self.operation_limits(bytes.len() * 2)?;
        match pending.target {
            ReadTarget::Source(kind) => {
                validate(kind, bytes, &limits)?;
                let candidate = Snapshot::new(name, bytes)?;
                self.imported_hash = candidate.hash.clone();
                self.source_name = candidate.name.clone();
                self.current = Some(candidate);
                self.kind = Some(kind);
                self.undo.clear();
                self.redo.clear();
                self.skeleton = None;
                self.patches.clear();
                Ok(true)
            }
            ReadTarget::Skeleton => {
                vitaboy::decode_skeleton(bytes, &limits).map_err(|e| e.to_string())?;
                self.skeleton = Some(Snapshot::new(name, bytes)?);
                Ok(true)
            }
            ReadTarget::Patch { is_user } => {
                let length = self
                    .patches
                    .iter()
                    .try_fold(bytes.len(), |n, p| n.checked_add(p.snapshot.bytes.len()))
                    .ok_or("patch byte count overflow")?;
                if self.patches.len() >= MAX_ATTACHMENTS || length > MAX_WORKBENCH_FILE_BYTES {
                    return Err("patch attachment budget exceeded".into());
                }
                let name = filename(name)?;
                if self.patches.iter().any(|p| p.snapshot.name == name) {
                    return Err("patch filenames must be unique".into());
                }
                if bytes
                    .len()
                    .checked_mul(16)
                    .and_then(|n| n.checked_add(65536))
                    .is_none_or(|n| n > limits.max_total_decoded_bytes)
                {
                    return Err("patch descriptor working-copy limit exceeded".into());
                }
                let file = iff::decode(bytes, &limits).map_err(|e| e.to_string())?;
                let descriptor = file
                    .chunks
                    .iter()
                    .find(|c| c.key.kind == *b"PIFF")
                    .ok_or("patch file has no PIFF descriptor")?;
                // The actual resolver selects the first PIFF descriptor. Reject
                // an unreadable descriptor before changing retained inputs.
                wonderland_legacy_formats::semantic::decode_piff(&descriptor.data, &limits)
                    .map_err(|e| e.to_string())?;
                self.patches.push(Attachment {
                    snapshot: Snapshot::new(&name, bytes)?,
                    is_user,
                });
                Ok(true)
            }
            ReadTarget::Exchange(format) => {
                self.import_exchange(&pending.source_hash, format, bytes)
            }
        }
    }
    fn publish(&mut self, bytes: Vec<u8>, label: &str) -> Result<bool, String> {
        let source = self.source()?;
        if source.bytes == bytes {
            return Ok(false);
        }
        if bytes.len() > MAX_WORKBENCH_FILE_BYTES || label.len() > 128 {
            return Err("candidate or history label exceeds limits".into());
        }
        validate(
            self.kind.ok_or("source format missing")?,
            &bytes,
            &self.operation_limits(bytes.capacity())?,
        )?;
        let needed = source.bytes.capacity().max(bytes.capacity());
        if needed > MAX_WORKBENCH_HISTORY_BYTES {
            return Err("candidate cannot fit a reversible history entry".into());
        }
        let candidate = Snapshot {
            name: source.name.clone(),
            hash: sha256(&bytes),
            bytes,
        };
        self.redo.clear();
        while self.undo.len() >= MAX_HISTORY_ENTRIES
            || self.history_bytes().saturating_add(needed) > MAX_WORKBENCH_HISTORY_BYTES
        {
            self.undo.remove(0);
        }
        let previous = self.current.replace(candidate).expect("source was checked");
        self.undo.push(Revision {
            snapshot: previous,
            label: label.into(),
        });
        self.pending = None;
        Ok(true)
    }
    fn restore(&mut self, redo: bool) -> Result<bool, String> {
        let list = if redo { &self.redo } else { &self.undo };
        let Some(revision) = list.last() else {
            return Ok(false);
        };
        validate(
            self.kind.ok_or("source format missing")?,
            &revision.snapshot.bytes,
            &self.operation_limits(0)?,
        )?;
        if self.source()?.bytes.capacity() > MAX_WORKBENCH_HISTORY_BYTES {
            return Err("restoration cannot retain its reverse history entry".into());
        }
        let revision = if redo {
            self.redo.pop()
        } else {
            self.undo.pop()
        }
        .expect("history was checked");
        let previous = self
            .current
            .replace(revision.snapshot)
            .expect("source was checked");
        let reverse = Revision {
            snapshot: previous,
            label: revision.label,
        };
        if redo {
            self.undo.push(reverse);
        } else {
            self.redo.push(reverse);
        }
        while self.history_bytes() > MAX_WORKBENCH_HISTORY_BYTES {
            // Keep the just-created reverse operation. Evict the farthest older
            // revision only after all validation succeeds and the target moves.
            if redo {
                if self.undo.len() > 1 {
                    self.undo.remove(0);
                } else {
                    self.redo.remove(0);
                }
            } else if self.redo.len() > 1 {
                self.redo.remove(0);
            } else {
                self.undo.remove(0);
            }
        }
        self.pending = None;
        Ok(true)
    }
    pub fn undo(&mut self) -> Result<bool, String> {
        self.restore(false)
    }
    pub fn redo(&mut self) -> Result<bool, String> {
        self.restore(true)
    }
    pub fn document_json(&self) -> Result<Value, String> {
        let source = self.source()?;
        let limits = self.operation_limits(0)?;
        match self.kind.ok_or("source format missing")? {
            InputKind::Upgrades => Ok(UpgradeDocument::import(&source.bytes, &limits)?
                .tree()
                .clone()),
            InputKind::Neighborhood => Ok(Value::Array(
                NeighborhoodDocument::import(&source.bytes, &limits)?
                    .entries()
                    .to_vec(),
            )),
            InputKind::Asset(kind) => {
                AssetDocument::import(kind, &source.bytes, &limits)?.data_json(&limits)
            }
            InputKind::City => {
                let image = CityImage::decode(&source.bytes, &limits)?;
                Ok(
                    json!({"width":image.width(),"height":image.height(),"pixels":image.pixels().len(),
                    "format":if source.bytes.starts_with(b"BM") {"BMP"} else {"PNG"},
                    "opaque":image.pixels().iter().all(|p|p[3] == 255),"first_rgba":image.pixel(0,0)?}),
                )
            }
            InputKind::PatchSource => {
                let view = self.patch_view()?;
                // The full report is available as a download. Keep the graphical
                // table bounded even for an IFF with thousands of resources.
                Ok(
                    json!({"source_name":view.source_name,"source_sha256":view.source_sha256,
                    "effective_sha256":view.effective_sha256,"applied":view.applied,"suppressed":view.suppressed,
                    "resource_count":view.resources.len(),"resources":view.resources.iter().take(100).collect::<Vec<_>>() }),
                )
            }
        }
    }
    pub fn apply_json_text(
        &mut self,
        expected: &str,
        bytes: &[u8],
        label: &str,
    ) -> Result<bool, String> {
        let extra = bytes
            .len()
            .checked_mul(2)
            .ok_or("JSON edit byte count overflow")?;
        let limits = self.operation_limits(extra)?;
        let tree = json_support::parse(bytes, &limits)?;
        let edits: Vec<JsonEdit> = serde_json::from_value(tree).map_err(|e| e.to_string())?;
        self.apply_json(expected, &edits, label)
    }
    pub fn apply_json(
        &mut self,
        expected: &str,
        edits: &[JsonEdit],
        label: &str,
    ) -> Result<bool, String> {
        self.guard(expected)?;
        let source = self.source()?;
        let limits = self.operation_limits(0)?;
        let bytes = match self.kind.ok_or("source format missing")? {
            InputKind::Upgrades => {
                let mut d = UpgradeDocument::import(&source.bytes, &limits)?;
                d.apply(expected, edits, &limits)?;
                d.export(&limits)?
            }
            InputKind::Neighborhood => {
                let mut d = NeighborhoodDocument::import(&source.bytes, &limits)?;
                d.apply(expected, edits, &limits)?;
                d.export(&limits)?
            }
            InputKind::Asset(kind) => {
                let mut d = AssetDocument::import(kind, &source.bytes, &limits)?;
                d.apply(expected, edits, &limits)?;
                d.export(&limits)?
            }
            _ => {
                return Err(
                    "JSON editing is available for upgrades, neighborhoods and typed assets".into(),
                )
            }
        };
        self.publish(bytes, label)
    }
    /// Apply the ordinary neighborhood form without inventing an optional
    /// Description field when its empty control represents absent/null source data.
    pub fn apply_neighborhood_fields(
        &mut self,
        expected: &str,
        fields: &NeighborhoodFields,
    ) -> Result<bool, String> {
        self.guard(expected)?;
        if self.kind != Some(InputKind::Neighborhood) {
            return Err("open neighborhoods first".into());
        }
        if [&fields.guid, &fields.name, &fields.description]
            .iter()
            .any(|s| s.len() > workbench_limits().max_string_bytes)
        {
            return Err("neighborhood form string exceeds limits".into());
        }
        let data = self.document_json()?;
        let entry = data
            .get(fields.index)
            .ok_or("neighborhood index out of range")?;
        let index = fields.index.to_string();
        let mut edits = vec![
            JsonEdit::set([index.as_str(), "GUID"], json!(fields.guid)),
            JsonEdit::set([index.as_str(), "Name"], json!(fields.name)),
            JsonEdit::set([index.as_str(), "Location", "X"], json!(fields.location[0])),
            JsonEdit::set([index.as_str(), "Location", "Y"], json!(fields.location[1])),
        ];
        if !fields.description.is_empty() || entry.get("Description").is_some_and(Value::is_string)
        {
            edits.push(JsonEdit::set(
                [index.as_str(), "Description"],
                json!(fields.description),
            ));
        }
        drop(data);
        self.apply_json(expected, &edits, "Update neighborhood")
    }

    /// Edit a source vector using decimal binary32 values. Visible FSOm bounds
    /// are recomputed in this same guarded transaction when a position changes.
    pub fn apply_transform(
        &mut self,
        expected: &str,
        path: &[String],
        components: &[f32],
    ) -> Result<bool, String> {
        self.guard(expected)?;
        let Some(InputKind::Asset(kind)) = self.kind else {
            return Err("open a typed asset first".into());
        };
        if path.len() > 6
            || path.iter().any(|part| part.len() > 24)
            || !(2..=4).contains(&components.len())
            || components.iter().any(|v| !v.is_finite())
        {
            return Err("invalid transform path, width or finite component".into());
        }
        let index = |text: &str| -> Result<usize, String> {
            let value: usize = text.parse().map_err(|_| "invalid transform index")?;
            if value.to_string() != text {
                return Err("transform indices must be canonical decimal values".into());
            }
            Ok(value)
        };
        let vertex_width = |field: &str| -> Result<usize, String> {
            match field {
                "position" | "normal" => Ok(3),
                "texture_coordinate" => Ok(2),
                _ => Err("unsupported vertex transform".into()),
            }
        };
        let segments: Vec<_> = path.iter().map(String::as_str).collect();
        let mut fsom_position = None;
        let width = match (kind, segments.as_slice()) {
            (AssetKind::Fsom, ["groups", group, geometry, "vertices", vertex, field]) => {
                let selected = [index(group)?, index(geometry)?, index(vertex)?];
                if *field == "position" {
                    fsom_position = Some(selected);
                }
                vertex_width(field)?
            }
            (AssetKind::Mesh, ["vertices", vertex, field]) => {
                index(vertex)?;
                vertex_width(field)?
            }
            (AssetKind::Animation, [field @ ("translations" | "rotations"), sample]) => {
                index(sample)?;
                if *field == "rotations" {
                    4
                } else {
                    3
                }
            }
            (AssetKind::Skeleton, ["bones", bone, field @ ("translation" | "rotation")]) => {
                index(bone)?;
                if *field == "rotation" {
                    4
                } else {
                    3
                }
            }
            (AssetKind::Nbhm, ["houses", house, "position"]) => {
                index(house)?;
                3
            }
            _ => return Err("unsupported stored transform path for this source format".into()),
        };
        if components.len() != width {
            return Err("transform width does not match the source vector".into());
        }
        let data = self.document_json()?;
        let mut old = &data;
        for part in &segments {
            old = if old.is_array() {
                old.get(index(part)?)
            } else {
                old.get(*part)
            }
            .ok_or("selected transform does not exist")?;
        }
        let value = Value::Array(components.iter().map(|v| json!(v.to_bits())).collect());
        if old == &value {
            return Ok(false);
        }
        let mut edits = vec![JsonEdit {
            path: path.to_vec(),
            value,
            remove: false,
        }];
        if let Some(selected) = fsom_position {
            let mut minimum = None::<[f32; 3]>;
            let mut maximum = [0.0f32; 3];
            let mut visit = |position: &Value, replacement: Option<&[f32]>| -> Result<(), String> {
                let mut point = [0.0f32; 3];
                if let Some(replacement) = replacement {
                    point.copy_from_slice(replacement);
                } else {
                    let values = position
                        .as_array()
                        .filter(|a| a.len() == 3)
                        .ok_or("invalid source position")?;
                    for (target, value) in point.iter_mut().zip(values) {
                        *target = f32::from_bits(
                            value
                                .as_u64()
                                .and_then(|v| u32::try_from(v).ok())
                                .ok_or("invalid source float bits")?,
                        );
                    }
                }
                if let Some(min) = minimum.as_mut() {
                    for axis in 0..3 {
                        min[axis] = min[axis].min(point[axis]);
                        maximum[axis] = maximum[axis].max(point[axis]);
                    }
                } else {
                    minimum = Some(point);
                    maximum = point;
                }
                Ok(())
            };
            for (group_index, group) in data["groups"]
                .as_array()
                .ok_or("missing FSOm groups")?
                .iter()
                .enumerate()
            {
                for (geometry_index, geometry) in group
                    .as_array()
                    .ok_or("invalid FSOm group")?
                    .iter()
                    .enumerate()
                {
                    for (vertex_index, vertex) in geometry["vertices"]
                        .as_array()
                        .ok_or("missing FSOm vertices")?
                        .iter()
                        .enumerate()
                    {
                        visit(
                            &vertex["position"],
                            ([group_index, geometry_index, vertex_index] == selected)
                                .then_some(components),
                        )?;
                    }
                }
            }
            // Portal masks are excluded from the visible bounds in the original
            // editor. Ordinary depth masks participate in the same bounds.
            if data["mask_type"] != 2 && !data["depth_mask"].is_null() {
                for vertex in data["depth_mask"]["vertices"]
                    .as_array()
                    .ok_or("invalid FSOm mask")?
                {
                    visit(&vertex["position"], None)?;
                }
            }
            edits.push(JsonEdit::set(
                ["bounds"],
                json!([
                    minimum.unwrap_or([0.0; 3]).map(f32::to_bits),
                    maximum.map(f32::to_bits)
                ]),
            ));
        }
        drop(data);
        self.apply_json(expected, &edits, "Update stored transform")
    }

    pub fn city_image(&self) -> Result<CityImage, String> {
        if self.kind != Some(InputKind::City) {
            return Err("open a city image first".into());
        }
        CityImage::decode(&self.source()?.bytes, &self.operation_limits(0)?)
    }
    pub fn paint(
        &mut self,
        expected: &str,
        layer: MapLayer,
        x: i32,
        y: i32,
        radius: usize,
        rgb: [u8; 3],
    ) -> Result<bool, String> {
        self.guard(expected)?;
        let limits = self.operation_limits(0)?;
        let mut image = self.city_image()?;
        image.validate(layer, true)?;
        let before = image.clone();
        image.brush(x, y, radius, rgb, layer, &limits)?;
        if image == before {
            return Ok(false);
        }
        let bytes = if self.source()?.bytes.starts_with(b"BM") {
            image.encode_bmp(&limits)?
        } else {
            image.encode_png(&limits)?
        };
        self.publish(bytes, "Paint city layer")
    }
    pub fn road(
        &mut self,
        expected: &str,
        x: i32,
        y: i32,
        length: usize,
        direction: u8,
        erase: bool,
    ) -> Result<bool, String> {
        self.guard(expected)?;
        if length == 0 {
            return Ok(false);
        }
        let limits = self.operation_limits(0)?;
        let mut image = self.city_image()?;
        image.validate(MapLayer::Road, true)?;
        let before = image.clone();
        image.road_stroke(x, y, length, direction, erase, &limits)?;
        if image == before {
            return Ok(false);
        }
        let bytes = if self.source()?.bytes.starts_with(b"BM") {
            image.encode_bmp(&limits)?
        } else {
            image.encode_png(&limits)?
        };
        self.publish(bytes, if erase { "Erase road" } else { "Draw road" })
    }
    pub fn nearest(&self, x: i32, y: i32) -> Result<Option<usize>, String> {
        if self.kind != Some(InputKind::Neighborhood) {
            return Err("open neighborhoods first".into());
        }
        NeighborhoodDocument::import(&self.source()?.bytes, &self.operation_limits(0)?)?
            .nearest(x, y)
    }
    pub fn set_patch_source_name(&mut self, expected: &str, name: &str) -> Result<bool, String> {
        self.guard(expected)?;
        if self.kind != Some(InputKind::PatchSource) {
            return Err("open a patch source first".into());
        }
        let name = filename(name)?;
        if name == self.source_name {
            return Ok(false);
        }
        self.source_name = name;
        self.pending = None;
        Ok(true)
    }
    pub fn remove_patch(&mut self, index: usize) -> Result<bool, String> {
        if index >= self.patches.len() {
            return Err("patch index out of range".into());
        }
        self.patches.remove(index);
        self.pending = None;
        Ok(true)
    }
    pub fn move_patch(&mut self, index: usize, up: bool) -> Result<bool, String> {
        let target = if up {
            index.checked_sub(1)
        } else {
            index.checked_add(1)
        }
        .ok_or("patch order boundary")?;
        if index >= self.patches.len() || target >= self.patches.len() {
            return Err("patch order boundary".into());
        }
        self.patches.swap(index, target);
        self.pending = None;
        Ok(true)
    }
    fn patch_view(&self) -> Result<PatchView, String> {
        if self.kind != Some(InputKind::PatchSource) {
            return Err("open a patch source first".into());
        }
        let inputs: Vec<_> = self
            .patches
            .iter()
            .map(|p| PatchInput {
                name: &p.snapshot.name,
                is_user: p.is_user,
                bytes: &p.snapshot.bytes,
            })
            .collect();
        PatchView::resolve(
            &self.source_name,
            &self.source()?.bytes,
            &inputs,
            &self.operation_limits(0)?,
        )
    }
    fn package(&self) -> Result<GltfPackage, String> {
        let source = self.source()?;
        let limits = self.operation_limits(0)?;
        match self.kind {
            Some(InputKind::Asset(AssetKind::Fsom)) => {
                let document = MeshOverrideDocument::import(&source.bytes, &limits)?;
                GltfPackage::from_fsom(document.mesh(), &source.hash, &limits)
            }
            Some(InputKind::Asset(AssetKind::Animation)) => {
                let rig = self
                    .skeleton
                    .as_ref()
                    .ok_or("attach the animation's actual skeleton first")?;
                let animation =
                    vitaboy::decode_animation(&source.bytes, &limits).map_err(|e| e.to_string())?;
                let skeleton =
                    vitaboy::decode_skeleton(&rig.bytes, &limits).map_err(|e| e.to_string())?;
                GltfPackage::from_animation(&animation, &skeleton, &source.hash, &rig.hash, &limits)
            }
            _ => Err("glTF exchange requires an FSOm mesh or animation with a skeleton".into()),
        }
    }
    pub fn import_exchange(
        &mut self,
        expected: &str,
        format: ExchangeFormat,
        bytes: &[u8],
    ) -> Result<bool, String> {
        self.guard(expected)?;
        let source = self.source()?;
        let limits = self.operation_limits(
            bytes
                .len()
                .checked_mul(2)
                .ok_or("exchange bytes overflow")?,
        )?;
        let encoded = if format == ExchangeFormat::Obj {
            if self.kind != Some(InputKind::Asset(AssetKind::Fsom)) {
                return Err("OBJ exchange requires an FSOm source".into());
            }
            let mut document = MeshOverrideDocument::import(&source.bytes, &limits)?;
            document.import_obj(expected, bytes, &limits)?;
            document.export().to_vec()
        } else {
            let package = if format == ExchangeFormat::Glb {
                GltfPackage::from_glb(bytes, &limits)?
            } else {
                GltfPackage::from_gltf(bytes, &limits)?
            };
            match self.kind {
                Some(InputKind::Asset(AssetKind::Fsom)) => {
                    let document = MeshOverrideDocument::import(&source.bytes, &limits)?;
                    let edited = package.apply_fsom(document.mesh(), &source.hash, &limits)?;
                    if &edited == document.mesh() {
                        return Ok(false);
                    }
                    reconstruction::encode_fsom(&edited, &limits).map_err(|e| e.to_string())?
                }
                Some(InputKind::Asset(AssetKind::Animation)) => {
                    let rig = self
                        .skeleton
                        .as_ref()
                        .ok_or("attach the animation's actual skeleton first")?;
                    let animation = vitaboy::decode_animation(&source.bytes, &limits)
                        .map_err(|e| e.to_string())?;
                    let skeleton =
                        vitaboy::decode_skeleton(&rig.bytes, &limits).map_err(|e| e.to_string())?;
                    let edited = package.apply_animation(
                        &animation,
                        &skeleton,
                        &source.hash,
                        &rig.hash,
                        &limits,
                    )?;
                    if edited == animation {
                        return Ok(false);
                    }
                    vitaboy::encode_animation(&edited, &limits).map_err(|e| e.to_string())?
                }
                _ => return Err("glTF exchange requires an FSOm source or animation".into()),
            }
        };
        self.publish(encoded, "Import source-bound exchange")
    }
    pub fn export_source(&self) -> Result<Download, String> {
        self.export(ExportFormat::Source)
    }
    pub fn export(&self, format: ExportFormat) -> Result<Download, String> {
        let source = self.source()?;
        let limits = self.operation_limits(0)?;
        let stem = source
            .name
            .rsplit_once('.')
            .map(|p| p.0)
            .unwrap_or(&source.name);
        let (bytes, extension, mime) = match format {
            ExportFormat::Source => {
                validate(
                    self.kind.ok_or("source format missing")?,
                    &source.bytes,
                    &limits,
                )?;
                return Ok(Download {
                    name: source.name.clone(),
                    mime: "application/octet-stream",
                    bytes: source.bytes.clone(),
                });
            }
            ExportFormat::Json => {
                let bytes = if self.kind == Some(InputKind::PatchSource) {
                    self.patch_view()?.metadata_json(&limits)?
                } else {
                    json_support::encode(&self.document_json()?, &limits)?
                };
                (bytes, "inspection.json", "application/json")
            }
            ExportFormat::Png => (self.city_image()?.encode_png(&limits)?, "png", "image/png"),
            ExportFormat::Bmp => (self.city_image()?.encode_bmp(&limits)?, "bmp", "image/bmp"),
            ExportFormat::Obj | ExportFormat::Mtl => {
                if self.kind != Some(InputKind::Asset(AssetKind::Fsom)) {
                    return Err("OBJ/MTL export requires an FSOm source".into());
                }
                let document = MeshOverrideDocument::import(&source.bytes, &limits)?;
                if format == ExportFormat::Obj {
                    (document.export_obj(&limits)?, "obj", "text/plain")
                } else {
                    (document.export_mtl(&limits)?, "mtl", "text/plain")
                }
            }
            ExportFormat::Glb => (self.package()?.to_glb(&limits)?, "glb", "model/gltf-binary"),
            ExportFormat::Gltf => (self.package()?.to_gltf(&limits)?, "gltf", "model/gltf+json"),
            ExportFormat::EffectiveIff => (
                self.patch_view()?.effective_bytes().to_vec(),
                "effective.iff",
                "application/octet-stream",
            ),
        };
        Ok(Download {
            name: format!("{stem}.{extension}"),
            mime,
            bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wonderland_creator::json_support::JsonEdit;

    fn open(session: &mut WorkbenchSession, kind: InputKind, name: &str, bytes: &[u8]) {
        let ticket = session
            .begin_read(ReadTarget::Source(kind), bytes.len())
            .unwrap();
        session.complete_read(ticket, name, bytes).unwrap();
    }

    #[test]
    fn guarded_json_transactions_preserve_original_and_history_after_failure() {
        let original = br#"{ "Version": 2, "Files": [], "future": "kept" }"#;
        let mut session = WorkbenchSession::new(WorkbenchMode::Upgrades);
        open(&mut session, InputKind::Upgrades, "upgrades.json", original);
        let guard = session.source_sha256().to_string();
        assert!(!session
            .apply_json(
                &guard,
                &[JsonEdit::set(["future"], json!("kept"))],
                "No change"
            )
            .unwrap());
        assert_eq!(session.undo_len(), 0);
        assert!(session
            .apply_json(
                &guard,
                &[JsonEdit::set(["future"], json!("edited"))],
                "Update future field"
            )
            .unwrap());
        let changed = session.export_source().unwrap().bytes;
        assert_ne!(changed, original);
        assert!(session
            .apply_json(
                &guard,
                &[JsonEdit::set(["Version"], json!(0))],
                "Stale edit"
            )
            .is_err());
        assert_eq!(session.export_source().unwrap().bytes, changed);
        assert!(session.undo().unwrap());
        assert_eq!(session.export_source().unwrap().bytes, original);
        assert!(!session.dirty());
        assert!(session.redo().unwrap());
        assert_eq!(session.export_source().unwrap().bytes, changed);
        assert!(session.dirty());
    }

    #[test]
    fn asynchronous_uploads_bind_size_generation_and_original_source() {
        let mut session = WorkbenchSession::new(WorkbenchMode::Upgrades);
        let original = br#"{"Version":2,"Files":[]}"#;
        open(&mut session, InputKind::Upgrades, "upgrades.json", original);
        let old = session
            .begin_read(ReadTarget::Source(InputKind::Upgrades), original.len())
            .unwrap();
        let current = session
            .begin_read(ReadTarget::Source(InputKind::Upgrades), 2)
            .unwrap();
        assert!(session.complete_read(old, "late.json", original).is_err());
        assert!(session.complete_read(current, "bad.json", b"{}").is_err());
        assert_eq!(session.export_source().unwrap().bytes, original);
        assert!(session
            .begin_read(
                ReadTarget::Source(InputKind::Upgrades),
                MAX_WORKBENCH_FILE_BYTES + 1
            )
            .is_err());
        assert_eq!(session.export_source().unwrap().bytes, original);
    }

    #[test]
    fn city_forms_keep_validated_pixels_and_original_noop_encoding() {
        let limits = workbench_limits();
        let source = CityImage::from_rgba(512, 512, vec![[0, 0, 0, 255]; 512 * 512], &limits)
            .unwrap()
            .encode_png(&limits)
            .unwrap();
        let mut session = WorkbenchSession::new(WorkbenchMode::City);
        open(&mut session, InputKind::City, "terrain.png", &source);
        let guard = session.source_sha256().to_string();
        assert!(!session
            .paint(&guard, MapLayer::Terrain, 0, 0, 0, [0, 0, 0])
            .unwrap());
        assert_eq!(session.export_source().unwrap().bytes, source);
        assert!(session
            .paint(&guard, MapLayer::Terrain, 0, 0, 0, [1, 2, 3])
            .is_err());
        assert!(session
            .paint(&guard, MapLayer::Terrain, 0, 0, 0, [255, 0, 0])
            .unwrap());
        let edited = session.export_source().unwrap();
        let image = CityImage::decode(&edited.bytes, &limits).unwrap();
        assert_eq!(image.pixel(0, 0).unwrap(), [255, 0, 0, 255]);
        assert_eq!(image.pixel(1, 0).unwrap(), [0, 0, 0, 255]);
        assert!(session.undo().unwrap());
        assert_eq!(session.export_source().unwrap().bytes, source);
    }

    #[test]
    fn neighborhood_edits_preserve_source_order_and_explicit_identity() {
        let source = br#"[{"GUID":"a","Name":"First","Location":{"X":0,"Y":1}},{"GUID":"b","Name":"Second","Location":{"X":2,"Y":1}}]"#;
        let mut session = WorkbenchSession::new(WorkbenchMode::Neighborhood);
        open(
            &mut session,
            InputKind::Neighborhood,
            "neighborhoods.json",
            source,
        );
        assert_eq!(session.nearest(1, 1).unwrap(), Some(0));
        let guard = session.source_sha256().to_owned();
        assert!(session
            .apply_json(
                &guard,
                &[JsonEdit::set(["1", "GUID"], json!("a"))],
                "Duplicate identity"
            )
            .is_err());
        assert_eq!(session.export_source().unwrap().bytes, source);
        session
            .apply_json(
                &guard,
                &[JsonEdit::set(["1", "Name"], json!("Updated"))],
                "Rename neighborhood",
            )
            .unwrap();
        assert_eq!(session.document_json().unwrap()[1]["GUID"], "b");
    }

    #[test]
    fn neighborhood_form_preserves_absent_and_null_description_on_noop() {
        for source in [
            br#"[ {"GUID":"a","Name":"First","Location":{"X":0,"Y":1},"future":7} ]"#.as_slice(),
            br#"[ {"GUID":"a","Name":"First","Description":null,"Location":{"X":0,"Y":1},"future":7} ]"#.as_slice(),
        ] {
            let mut session = WorkbenchSession::new(WorkbenchMode::Neighborhood);
            open(&mut session, InputKind::Neighborhood, "neighborhoods.json", source);
            let guard = session.source_sha256().to_owned();
            let mut fields = NeighborhoodFields {
                index: 0, guid: "a".into(), name: "First".into(),
                description: String::new(), location: [0, 1],
            };
            assert!(!session.apply_neighborhood_fields(&guard, &fields).unwrap());
            assert_eq!(session.export_source().unwrap().bytes, source);
            fields.name = "Renamed".into();
            assert!(session.apply_neighborhood_fields(&guard, &fields).unwrap());
            let data = session.document_json().unwrap();
            assert_eq!(data[0]["future"], 7);
            assert_eq!(data[0].get("Description"), serde_json::from_slice::<Value>(source).unwrap()[0].get("Description"));
            assert!(session.undo().unwrap());
            assert_eq!(session.export_source().unwrap().bytes, source);
        }
    }

    #[test]
    fn fsom_form_updates_visible_bounds_and_source_exchange_guards_together() {
        // Independent literal little-endian v3 layout in a real gzip stream:
        // one triangle, unit Z normals, and a signed-zero first X coordinate.
        let source =
            include_bytes!("../../../tests/tools/fixtures/creator-workbench-triangle.fsom");
        let mut session = WorkbenchSession::new(WorkbenchMode::Assets);
        open(
            &mut session,
            InputKind::Asset(AssetKind::Fsom),
            "triangle.fsom",
            source,
        );
        let guard = session.source_sha256().to_owned();
        let path: Vec<String> = ["groups", "0", "0", "vertices", "1", "position"]
            .map(str::to_owned)
            .into();
        assert!(!session
            .apply_transform(&guard, &path, &[1., 0., 0.])
            .unwrap());
        assert_eq!(session.export_source().unwrap().bytes, source);
        let original_glb = session.export(ExportFormat::Glb).unwrap().bytes;
        assert!(!session
            .import_exchange(&guard, ExchangeFormat::Glb, &original_glb)
            .unwrap());
        let obj = session.export(ExportFormat::Obj).unwrap().bytes;
        assert!(!session
            .import_exchange(&guard, ExchangeFormat::Obj, &obj)
            .unwrap());
        assert!(session
            .apply_transform(&guard, &path, &[2., 0., 0.])
            .unwrap());
        let changed = session.export_source().unwrap().bytes;
        let mesh = reconstruction::decode_fsom(&changed, &workbench_limits()).unwrap();
        assert_eq!(mesh.bounds[1].map(vitaboy::F32Bits::get), [2., 1., 0.]);
        assert_eq!(mesh.groups[0][0].vertices[0].position[0].0, 0x8000_0000);
        let changed_guard = session.source_sha256().to_owned();
        assert!(session
            .import_exchange(&changed_guard, ExchangeFormat::Glb, &original_glb)
            .is_err());
        assert_eq!(session.export_source().unwrap().bytes, changed);
        let mut glb = session.export(ExportFormat::Glb).unwrap().bytes;
        glb[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(session
            .import_exchange(&changed_guard, ExchangeFormat::Glb, &glb)
            .is_err());
        assert_eq!(session.export_source().unwrap().bytes, changed);
        assert!(session.undo().unwrap());
        assert_eq!(session.export_source().unwrap().bytes, source);
    }

    #[test]
    fn animation_exchange_requires_actual_skeleton_and_retains_its_dirty_state() {
        use vitaboy::*;
        let limits = workbench_limits();
        let f = F32Bits::from_f32;
        let rig = Skeleton {
            version: 1,
            name: "rig".into(),
            root: 0,
            coordinate_policy: CoordinatePolicy::FreeSo,
            bones: vec![Bone {
                unknown: 0,
                name: "ROOT".into(),
                parent_name: "NULL".into(),
                properties_flag: 0,
                properties: PropertyList { items: vec![] },
                translation: [f(0.); 3],
                rotation: [f(0.), f(0.), f(0.), f(1.)],
                can_translate: 1,
                can_rotate: 1,
                can_blend: 1,
                wiggle_value: f(0.),
                wiggle_power: f(0.),
                index: 0,
                parent: None,
                children: vec![],
            }],
        };
        let animation = Animation {
            version: 2,
            name: "one-frame".into(),
            duration_ms: f(1000.),
            distance: f(0.),
            is_moving: 0,
            translations: vec![[f(3.), f(6.), f(9.)]],
            rotations: vec![[f(0.), f(0.), f(0.), f(1.)]],
            motions: vec![Motion {
                unknown: 7,
                bone_name: "ROOT".into(),
                frame_count: 1,
                duration_ms: f(1000.),
                translation_flag: 1,
                rotation_flag: 1,
                first_translation_index: 0,
                first_rotation_index: 0,
                properties_flag: 0,
                properties: vec![],
                time_properties_flag: 0,
                time_properties: vec![],
            }],
            num_frames: 1,
            coordinate_policy: CoordinatePolicy::FreeSo,
        };
        let source = encode_animation(&animation, &limits).unwrap();
        let mut session = WorkbenchSession::new(WorkbenchMode::Assets);
        open(
            &mut session,
            InputKind::Asset(AssetKind::Animation),
            "one.anim",
            &source,
        );
        assert!(!session.dirty());
        assert!(session.export(ExportFormat::Glb).is_err());
        assert!(session
            .begin_read(ReadTarget::Exchange(ExchangeFormat::Obj), 1)
            .is_err());
        let skeleton = encode_skeleton(&rig, &limits).unwrap();
        let ticket = session
            .begin_read(ReadTarget::Skeleton, skeleton.len())
            .unwrap();
        session
            .complete_read(ticket, "rig.skel", &skeleton)
            .unwrap();
        assert!(
            session.dirty(),
            "attached source skeleton would be lost on unload"
        );
        let guard = session.source_sha256().to_owned();
        let glb = session.export(ExportFormat::Glb).unwrap().bytes;
        assert!(!session
            .import_exchange(&guard, ExchangeFormat::Glb, &glb)
            .unwrap());
        let mut replacement = rig;
        replacement.name = "other-rig".into();
        let skeleton = encode_skeleton(&replacement, &limits).unwrap();
        let ticket = session
            .begin_read(ReadTarget::Skeleton, skeleton.len())
            .unwrap();
        session
            .complete_read(ticket, "replacement.skel", &skeleton)
            .unwrap();
        assert!(session
            .import_exchange(&guard, ExchangeFormat::Glb, &glb)
            .is_err());
        assert_eq!(session.export_source().unwrap().bytes, source);
    }

    #[test]
    fn patch_uploads_validate_descriptors_before_preserving_ordered_provenance() {
        // Literal IFF/PIFF layouts, independent of the resource/patch writers.
        fn envelope(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
            let mut bytes = vec![0; 64];
            let signature = b"IFF FILE 2.5:TYPE FOLLOWED BY SIZE\0 JAMIE DOORNBOS & MAXIS 1";
            bytes[..signature.len()].copy_from_slice(signature);
            bytes.extend(kind);
            bytes.extend(((76 + payload.len()) as u32).to_be_bytes());
            bytes.extend(1u16.to_be_bytes());
            bytes.extend([0; 66]);
            bytes.extend(payload);
            bytes
        }
        fn text(bytes: &mut Vec<u8>, value: &[u8]) {
            bytes.push(u8::try_from(value.len()).unwrap());
            bytes.extend(value);
        }
        fn patch(value: &[u8]) -> Vec<u8> {
            let mut bytes = vec![2, 0];
            text(&mut bytes, b"Source.iff");
            text(&mut bytes, b"fixture");
            bytes.extend(1u16.to_le_bytes());
            bytes.extend(b"ZZZZ");
            bytes.extend(1u16.to_le_bytes());
            text(&mut bytes, b"");
            bytes.push(0); // patch existing
            text(&mut bytes, b"changed");
            bytes.extend(17u16.to_le_bytes());
            bytes.extend(1u16.to_le_bytes());
            bytes.extend((value.len() as u32).to_le_bytes());
            bytes.extend(2u32.to_le_bytes());
            bytes.extend([0, 3, 0]); // remove three bytes at zero
            bytes.extend([0, value.len() as u8, 1]); // add replacement at zero
            bytes.extend(value);
            envelope(b"PIFF", &bytes)
        }
        let source = envelope(b"ZZZZ", b"abc");
        let mut session = WorkbenchSession::new(WorkbenchMode::Patches);
        open(&mut session, InputKind::PatchSource, "Source.iff", &source);
        let bad = envelope(b"PIFF", &[2]);
        let ticket = session
            .begin_read(ReadTarget::Patch { is_user: false }, bad.len())
            .unwrap();
        assert!(session.complete_read(ticket, "bad.piff", &bad).is_err());
        assert!(session.attachments().is_empty());
        for (name, value, is_user) in [
            ("official.piff", b"official".as_slice(), false),
            ("user.piff", b"user".as_slice(), true),
        ] {
            let bytes = patch(value);
            let ticket = session
                .begin_read(ReadTarget::Patch { is_user }, bytes.len())
                .unwrap();
            session.complete_read(ticket, name, &bytes).unwrap();
        }
        let metadata = session.document_json().unwrap();
        assert_eq!(metadata["applied"][0]["name"], "user.piff");
        assert_eq!(metadata["applied"][0]["order"], 1);
        assert_eq!(metadata["suppressed"][0]["name"], "official.piff");
        let effective = session.export(ExportFormat::EffectiveIff).unwrap().bytes;
        assert_eq!(
            iff::decode(&effective, &workbench_limits()).unwrap().chunks[0].data,
            b"user"
        );
        session.move_patch(1, true).unwrap();
        assert_eq!(session.document_json().unwrap()["applied"][0]["order"], 0);
        session.remove_patch(0).unwrap();
        let effective = session.export(ExportFormat::EffectiveIff).unwrap().bytes;
        assert_eq!(
            iff::decode(&effective, &workbench_limits()).unwrap().chunks[0].data,
            b"official"
        );
        let guard = session.source_sha256().to_owned();
        session.set_patch_source_name(&guard, "source.iff").unwrap();
        assert_eq!(
            session.export(ExportFormat::EffectiveIff).unwrap().bytes,
            source
        );
        assert_eq!(session.export_source().unwrap().bytes, source);
    }

    #[test]
    fn growing_candidate_keeps_latest_undo_and_redo_available_at_history_limit() {
        let original = br#"{"Version":2,"Files":[],"future":"old"}"#;
        let mut session = WorkbenchSession::new(WorkbenchMode::Upgrades);
        open(&mut session, InputKind::Upgrades, "upgrades.json", original);
        // An older retained revision fills the byte budget. Its content is not
        // the target of this undo; this exercises eviction before publication.
        session.undo.push(Revision {
            label: "Older edit".into(),
            snapshot: Snapshot {
                name: "upgrades.json".into(),
                hash: "older".into(),
                bytes: vec![b' '; MAX_WORKBENCH_HISTORY_BYTES - 1024],
            },
        });
        let guard = session.source_sha256().to_owned();
        session
            .apply_json(
                &guard,
                &[JsonEdit::set(["future"], json!("x".repeat(8192)))],
                "Larger value",
            )
            .unwrap();
        let current = session.export_source().unwrap().bytes;
        assert!(session.undo().unwrap());
        assert_eq!(session.export_source().unwrap().bytes, original);
        assert!(session.redo().unwrap());
        assert_eq!(session.export_source().unwrap().bytes, current);
        assert!(session.history_bytes() <= MAX_WORKBENCH_HISTORY_BYTES);
    }
}
