//! Receiving boundary for C-generated visual derivatives. Neither the receipt
//! nor an FSOf authenticates a source. The expected digest must come from the
//! receiver's independently trusted job result/manifest, not from this receipt.
use super::*;
use serde::{Deserialize, Deserializer};

const MAX_RECEIPT_BYTES: usize = 65_536;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: u16,
    kind: String,
    format: String,
    source_hash: String,
    sha256: String,
    bytes: u64,
    provenance: Provenance,
    revision: Revision,
    options: FacadeExportOptions,
    lighting_state: String,
    geometry: String,
    work_units: u64,
    regions: u16,
    diagnostics: Vec<Diagnostic>,
    diagnostics_total: u64,
    not_a_game_save: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Provenance {
    kind: WorldSourceKind,
    origin: String,
    source_revision: String,
    effective_source: AssetKey,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Revision {
    // A nullable lot ID is still a required field. Missing is not the same as
    // explicitly declared offline provenance.
    #[serde(deserialize_with = "required_nullable_string")]
    lot_id: Option<String>,
    epoch: String,
    tick: String,
    architecture_revision: String,
    content: AssetKey,
}
fn required_nullable_string<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(deserializer)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Diagnostic {
    code: String,
    resource: String,
    message: String,
}
fn bad(message: &str) -> WorldError {
    // Never echo untrusted receipt fields into a service log or UI error.
    WorldError(format!("facade verification: {message}"))
}
impl Provenance {
    fn matches(&self, expected: &WorldProvenance) -> bool {
        self.kind == expected.kind
            && self.origin == expected.origin
            && self.source_revision == expected.source_revision
            && self.effective_source == expected.effective_source
    }
}
impl Revision {
    fn matches(&self, expected: &WorldRevision) -> bool {
        self.lot_id == expected.lot_id.map(|value| value.to_string())
            && self.epoch == expected.epoch.to_string()
            && self.tick == expected.tick.to_string()
            && self.architecture_revision == expected.architecture_revision.to_string()
            && self.content == expected.content
    }
}
/// Verify bytes and metadata against an independently trusted export request.
///
/// `trusted_artifact_sha256` must be supplied by an authenticated job result or
/// trusted manifest. Passing the receipt's own digest defeats this trust check.
/// The expected world/options must likewise be the receiver's admitted inputs.
/// All size, identity, receipt and codec checks finish before a decoded asset is
/// returned. This function performs no publication, database access, gameplay
/// admission or rendering. It does not prove that an untrusted renderer generated
/// semantically correct pixels; use rendering comparisons for that qualification.
///
/// Raw input is capped at 16 MiB and receipt JSON at 64 KiB. FSOf decoding uses
/// the same 16 MiB decoded, 2048-dimension and geometry budgets as the producer.
/// The caller must impose limits before buffering HTTP/files as well.
pub fn verify_world_facade(
    expected_world: &WorldDocument,
    options: FacadeExportOptions,
    trusted_artifact_sha256: AssetKey,
    bytes: &[u8],
    metadata_json: &str,
) -> Result<fsof::Fsof, WorldError> {
    if bytes.len() as u64 > MAX_OUTPUT_BYTES
        || metadata_json.len() > MAX_RECEIPT_BYTES
        || !bytes.starts_with(b"FSOf\x01\0\0\0\x01")
    {
        return Err(bad("unsupported header or input byte budget"));
    }
    let actual_digest = AssetKey(Sha256::digest(bytes).into());
    if actual_digest != trusted_artifact_sha256 {
        return Err(bad("artifact digest differs from trusted request"));
    }
    // Struct deserialization rejects duplicate fields as well as unknown keys,
    // including every nested receipt object. A generic JSON Value would lose
    // duplicate-key evidence before validation.
    let receipt: Receipt = serde_json::from_str(metadata_json)
        .map_err(|_| bad("invalid, incomplete or ambiguous receipt JSON"))?;
    if receipt.schema != 1
        || receipt.kind != "presentation_facade"
        || receipt.format != "FSOf v1 RGBA8 gzip"
        || !receipt.not_a_game_save
        || receipt.lighting_state != "supplied-only; no invented night state"
        || receipt.geometry != "source room topology and facade atlas layout"
        || receipt.bytes != bytes.len() as u64
        || receipt.sha256 != hex(&actual_digest.0)
        || receipt.options != options
        || !receipt.provenance.matches(&expected_world.provenance)
        || !receipt.revision.matches(&expected_world.revision)
    {
        return Err(bad(
            "receipt differs from expected source, options or format",
        ));
    }
    if receipt.work_units > MAX_WORK
        || !(1..=256).contains(&receipt.regions)
        || receipt.diagnostics.len() > 64
        || receipt.diagnostics_total < receipt.diagnostics.len() as u64
        || receipt.diagnostics.iter().any(|d| {
            d.code.chars().count() > 64
                || d.resource.chars().count() > 128
                || d.message.chars().count() > 512
        })
    {
        return Err(bad("receipt count or diagnostic budget"));
    }
    // Same serialization and source bounds as generation; never rerender a
    // source or accept its claimed source_hash without recomputing the key.
    let expected_source = facade_source_hash(expected_world, options)?;
    if receipt.source_hash != hex(&expected_source.0) {
        return Err(bad("receipt source hash differs from trusted request"));
    }
    let decoded =
        fsof::Fsof::decode(bytes, limits()).map_err(|_| bad("invalid or oversized FSOf"))?;
    if decoded.compression != fsof::TextureCompression::Rgba8 || decoded.night.is_some() {
        return Err(bad(
            "FSOf does not contain the declared single RGBA8 light state",
        ));
    }
    Ok(decoded)
}
