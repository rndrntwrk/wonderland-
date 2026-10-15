//! Private supervisor DTOs. The edge authenticates custody before forwarding;
//! browser packets supply only WLC1 actions, never these authority records.
use super::{CursorV1, DecimalId, HostBindingV1};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnedControlV2 {
    pub principal: DecimalId,
    pub avatar_id: DecimalId,
    pub control_generation: DecimalId,
    pub session_ref: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerBindingV1 {
    pub source_epoch: DecimalId,
    pub lot_incarnation: DecimalId,
    pub lot_location: u32,
    pub avatar_id: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionBindingV2 {
    pub revision: DecimalId,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoommateSourceV2 {
    pub lot_id: DecimalId,
    pub permissions_level: u8,
    pub is_pending: u8,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionSourceV2 {
    pub registered_at_ms: u64,
    pub created_at_ms: u64,
    pub observed_at_ms: u64,
    pub roommate_lots: Vec<RoommateSourceV2>,
    pub moderation_level: u8,
    pub mayor_nhood: Option<DecimalId>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRevisionsV2 {
    pub owned_assets: u32,
    pub wardrobe: String,
    pub ledger: String,
    pub progression: String,
    pub permissions: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReservedAvatarExportV2 {
    pub schema_version: u32,
    pub authority_id: String,
    pub account_id: DecimalId,
    pub avatar_id: DecimalId,
    pub lot_id: DecimalId,
    pub control_generation: DecimalId,
    pub admission_operation: String,
    pub reservation_digest: String,
    pub session_ref: String,
    pub account_revision: DecimalId,
    pub content_release: String,
    pub protocol_release: String,
    pub expires_at: u64,
    pub permission_binding: PermissionBindingV2,
    pub permission_source: PermissionSourceV2,
    pub source_revisions: SourceRevisionsV2,
    pub portable_json: String,
    pub portable_sha256: String,
    pub export_digest: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrepareContextV2 {
    pub batch_id: DecimalId,
    pub binding: HostBindingV1,
    pub predecessor: CursorV1,
    pub expected_control_revision: DecimalId,
}
