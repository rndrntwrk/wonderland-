//! Frozen WLB1 appearance record. The file-facing WorldDocument may grow, but
//! positional bincode fields must not change without a new bootstrap version.
//! In particular, serde skip_serializing_if is not a binary optional field.
use crate::world_view::*;
use serde::{Deserialize, Deserializer, Serializer, ser::SerializeStruct};

pub(super) fn serialize<S: Serializer>(
    world: &WorldDocument,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    if world.lighting.is_some() || world.schema_version != WORLD_SCHEMA_VERSION {
        return Err(serde::ser::Error::custom(
            "WLB1 does not carry a versioned lighting recipe",
        ));
    }
    let mut record = serializer.serialize_struct("WorldDocument", 11)?;
    record.serialize_field("schema_version", &world.schema_version)?;
    record.serialize_field("provenance", &world.provenance)?;
    record.serialize_field("revision", &world.revision)?;
    record.serialize_field("lot", &world.lot)?;
    record.serialize_field("objects", &world.objects)?;
    record.serialize_field("models", &world.models)?;
    record.serialize_field("materials", &world.materials)?;
    record.serialize_field("source_counts", &world.source_counts)?;
    record.serialize_field("category", &world.category)?;
    record.serialize_field("sounds", &world.sounds)?;
    record.serialize_field("diagnostics", &world.diagnostics)?;
    record.end()
}

#[derive(Deserialize)]
struct AppearanceV1 {
    schema_version: u16,
    provenance: WorldProvenance,
    revision: WorldRevision,
    lot: WorldLot,
    objects: Vec<WorldObject>,
    models: Vec<WorldModel>,
    materials: Vec<WorldMaterial>,
    source_counts: Option<SourceCounts>,
    category: Option<i32>,
    sounds: Vec<BlueprintSound>,
    diagnostics: Vec<WorldDiagnostic>,
}
pub(super) fn deserialize<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<WorldDocument, D::Error> {
    let v = AppearanceV1::deserialize(deserializer)?;
    if v.schema_version != WORLD_SCHEMA_VERSION {
        return Err(serde::de::Error::custom(
            "unsupported WLB1 appearance schema",
        ));
    }
    Ok(WorldDocument {
        schema_version: v.schema_version,
        lighting: None,
        provenance: v.provenance,
        revision: v.revision,
        lot: v.lot,
        objects: v.objects,
        models: v.models,
        materials: v.materials,
        source_counts: v.source_counts,
        category: v.category,
        sounds: v.sounds,
        diagnostics: v.diagnostics,
    })
}
