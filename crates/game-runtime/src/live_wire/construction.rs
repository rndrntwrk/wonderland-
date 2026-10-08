//! Server-owned native construction admission. No durable receipt is minted here.
use super::player::PlayerBinding;
use super::{CompareWriter, codec, guard};
use crate::sim_core::world::build::{BuildAuthority, BuildPreview};
use crate::{AcceptedCommand, EntityRef, Facing, GameRuntime, LotPosition, PrincipalKey, TilePos};
use bincode::Options;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_REQUEST_BYTES: usize = 128 * 1024;
const MAGIC: &[u8; 8] = b"WLCB\x01\r\n\x1a";

/// User choices only. No price, owner, object footprint, ID reservation or receipt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Selection {
    Floor {
        tile: TilePos,
        pattern: u16,
    },
    SolidWall {
        tile: TilePos,
        sides: u8,
    },
    Terrain {
        x: u16,
        y: u16,
        height: i16,
    },
    Purchase {
        catalog_id: u32,
        position: LotPosition,
        facing: Facing,
    },
    Move {
        entity: EntityRef,
        expected_revision: u64,
        position: LotPosition,
        facing: Facing,
    },
    Remove {
        entity: EntityRef,
        expected_revision: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstructionRequest {
    pub binding: PlayerBinding,
    pub request_id: u64,
    pub lot_id: u64,
    pub authority_epoch: u64,
    pub architecture_revision: u64,
    pub permissions_revision: u64,
    pub account_revision: u64,
    pub catalog_revision: u64,
    pub selections: Vec<Selection>,
}

/// Obtain from the authenticated session and current account/catalog providers.
/// This type intentionally has no deserializer. It must never be client input.
#[derive(Clone, Debug)]
pub struct ConstructionGrant {
    pub binding: PlayerBinding,
    pub principal: PrincipalKey,
    pub authority: BuildAuthority,
    pub floor_patterns: BTreeSet<u16>,
}

/// Retain on the server. A client confirms the exact hash and price it reviewed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstructionQuote {
    principal: PrincipalKey,
    request: ConstructionRequest,
    content_hash: [u8; 32],
    preview: BuildPreview,
}
impl ConstructionQuote {
    pub fn request_id(&self) -> u64 {
        self.request.request_id
    }
    pub fn preview(&self) -> &BuildPreview {
        &self.preview
    }
    pub fn consent(&self) -> ConstructionConsent {
        ConstructionConsent {
            request_id: self.request_id(),
            preview_hash: self.preview.preview_hash,
            cost: self.preview.cost,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstructionConsent {
    pub request_id: u64,
    pub preview_hash: [u8; 32],
    pub cost: i64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConstructionError {
    InvalidWire,
    InvalidRequest,
    NotAuthority,
    AdmissionChanged,
    PermissionDenied,
    StaleQuote,
    UnknownMaterial,
    UnsupportedWall,
    InvalidSelection,
    ConsentMismatch,
    SourceValidation(String),
}
impl std::fmt::Display for ConstructionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "construction: {self:?}")
    }
}
impl std::error::Error for ConstructionError {}
pub type Result<T> = std::result::Result<T, ConstructionError>;

fn valid_request(request: &ConstructionRequest) -> Result<()> {
    if request.request_id == 0
        || request.lot_id == 0
        || request.authority_epoch == 0
        || request.binding.source_epoch == 0
        || request.binding.lot_incarnation == 0
        || request.binding.lot_location == 0
        || request.binding.avatar_id == 0
        || request.selections.is_empty()
        || request.selections.len() > crate::sim_core::world::build::MAX_BUILD_EDITS
    {
        return Err(ConstructionError::InvalidRequest);
    }
    Ok(())
}

/// Separate versioned packet: existing gameplay action/receipt formats do not change.
/// The transport must cap message bytes before allocating them. Rust consumes
/// binary u64 fields directly; a JS Number intermediary is not supported.
pub fn encode_request(request: &ConstructionRequest) -> Result<Vec<u8>> {
    valid_request(request)?;
    let len = usize::try_from(
        codec(MAX_REQUEST_BYTES)
            .serialized_size(request)
            .map_err(|_| ConstructionError::InvalidWire)?,
    )
    .map_err(|_| ConstructionError::InvalidWire)?;
    if len > MAX_REQUEST_BYTES - 16 {
        return Err(ConstructionError::InvalidWire);
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(16 + len)
        .map_err(|_| ConstructionError::InvalidWire)?;
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&(len as u64).to_le_bytes());
    codec(len)
        .serialize_into(&mut bytes, request)
        .map_err(|_| ConstructionError::InvalidWire)?;
    Ok(bytes)
}
pub fn decode_request(bytes: &[u8]) -> Result<ConstructionRequest> {
    if bytes.len() < 16 || bytes.len() > MAX_REQUEST_BYTES || &bytes[..8] != MAGIC {
        return Err(ConstructionError::InvalidWire);
    }
    let length = u64::from_le_bytes(
        bytes[8..16]
            .try_into()
            .map_err(|_| ConstructionError::InvalidWire)?,
    );
    if length != (bytes.len() - 16) as u64 {
        return Err(ConstructionError::InvalidWire);
    }
    let raw = &bytes[16..];
    let budget = guard::Budget::new(131_072, 8 * 1024 * 1024, 64);
    let value: ConstructionRequest = codec(raw.len())
        .deserialize_seed(guard::ValueSeed::new(&budget), raw)
        .map_err(|_| ConstructionError::InvalidWire)?;
    let mut comparison = CompareWriter { remaining: raw };
    codec(raw.len())
        .serialize_into(&mut comparison, &value)
        .map_err(|_| ConstructionError::InvalidWire)?;
    if !comparison.remaining.is_empty() {
        return Err(ConstructionError::InvalidWire);
    }
    valid_request(&value)?;
    Ok(value)
}

fn validate_grant(
    runtime: &GameRuntime,
    grant: &ConstructionGrant,
    request: &ConstructionRequest,
) -> Result<()> {
    valid_request(request)?;
    if runtime.sim().role() != crate::RuntimeRole::Authority {
        return Err(ConstructionError::NotAuthority);
    }
    let state = runtime.sim().state();
    let authority = &grant.authority;
    if grant.binding != request.binding
        || request.lot_id != state.lot_id
        || request.authority_epoch != state.authority_epoch
    {
        return Err(ConstructionError::AdmissionChanged);
    }
    let actor = state
        .entities
        .get(&authority.actor.object_id)
        .filter(|e| {
            e.info.reference == authority.actor
                && e.info.is_avatar
                && !e.info.dead
                && e.info.persistent_id == grant.binding.avatar_id
        })
        .ok_or(ConstructionError::AdmissionChanged)?;
    if !state.ids.is_live(actor.info.reference)
        || grant.principal.0 == 0
        || !state
            .interaction_access
            .get(&authority.actor)
            .is_some_and(|access| access.principal == grant.principal)
        || !authority.connected
        || !authority.can_build
        || authority.owner.0 != grant.binding.avatar_id
    {
        return Err(ConstructionError::PermissionDenied);
    }
    if request.architecture_revision != state.world.lot.revision().architecture
        || request.permissions_revision != authority.permissions_revision
        || request.account_revision != authority.account_revision
        || request.catalog_revision != authority.catalog_revision
    {
        return Err(ConstructionError::StaleQuote);
    }
    Ok(())
}

/// Build a pure source-validated quote. `operation` is allocated by the server's
/// durable journal, not the browser request ID. Hold this quote on the server.
/// This stage neither reserves real entity IDs nor mutates money or world state.
pub fn quote(
    runtime: &GameRuntime,
    grant: &ConstructionGrant,
    operation: u64,
    request: &ConstructionRequest,
) -> Result<ConstructionQuote> {
    use crate::sim_core::world::{
        WorldObject,
        build::{BuildEdit, BuildIntent},
        tiles::Diagonal,
    };
    validate_grant(runtime, grant, request)?;
    if operation == 0 {
        return Err(ConstructionError::InvalidRequest);
    }
    let state = runtime.sim().state();
    let mut allocator = state.ids.clone();
    let mut edits = Vec::new();
    edits
        .try_reserve_exact(request.selections.len())
        .map_err(|_| ConstructionError::InvalidRequest)?;
    for selection in &request.selections {
        edits.push(match *selection {
            Selection::Floor { tile, pattern } => {
                if pattern != 0 && !grant.floor_patterns.contains(&pattern) {
                    return Err(ConstructionError::UnknownMaterial);
                }
                BuildEdit::SetFloor { tile, pattern }
            }
            Selection::SolidWall { tile, sides } => {
                let old = &state
                    .world
                    .lot
                    .tile(tile)
                    .ok_or(ConstructionError::InvalidSelection)?
                    .wall;
                if sides & !15 != 0
                    || old.diagonal != Diagonal::None
                    || old.half_floors.is_some()
                    || old.occupied & !sides != 0
                {
                    return Err(ConstructionError::UnsupportedWall);
                }
                // Occupancy is source state, never an editable client bitfield.
                let mut wall = old.clone();
                wall.sides = sides;
                wall.room_separators = sides;
                BuildEdit::SetWall { tile, wall }
            }
            Selection::Terrain { x, y, height } => BuildEdit::SetTerrainVertex { x, y, height },
            Selection::Purchase {
                catalog_id,
                position,
                facing,
            } => {
                let reference = allocator
                    .allocate()
                    .map_err(|e| ConstructionError::SourceValidation(e.to_string()))?;
                let mut object = WorldObject::new(reference, position);
                object.facing = facing;
                // preview_build replaces footprint/rules/owner/entrypoints from
                // the actual content catalogue. No client geometry is accepted.
                BuildEdit::PlaceObject { catalog_id, object }
            }
            Selection::Move {
                entity,
                expected_revision,
                position,
                facing,
            } => BuildEdit::MoveObject {
                entity,
                expected_revision,
                position,
                facing,
            },
            Selection::Remove {
                entity,
                expected_revision,
            } => BuildEdit::DeleteObject {
                entity,
                expected_revision,
            },
        });
    }
    let intent = BuildIntent::new(
        operation,
        grant.authority.actor,
        request.architecture_revision,
        edits,
    );
    let preview = runtime
        .preview_build(&intent, &grant.authority)
        .map_err(|e| ConstructionError::SourceValidation(e.to_string()))?;
    Ok(ConstructionQuote {
        principal: grant.principal,
        request: request.clone(),
        content_hash: state.content.content_hash,
        preview,
    })
}

/// Revalidate on the serialized authority turn immediately before enqueueing the
/// returned BeginBuild. It is NOT a durable confirmation. The server must compare
/// account/catalog revisions in its durable store and dispatch the exact request
/// idempotently. A lost response must be reconciled, never blindly resubmitted.
pub fn confirm(
    runtime: &GameRuntime,
    grant: &ConstructionGrant,
    offer: &ConstructionQuote,
    consent: ConstructionConsent,
) -> Result<AcceptedCommand> {
    if consent != offer.consent() {
        return Err(ConstructionError::ConsentMismatch);
    }
    if runtime.sim().state().content.content_hash != offer.content_hash {
        return Err(ConstructionError::StaleQuote);
    }
    let current = quote(
        runtime,
        grant,
        offer.preview.intent.operation,
        &offer.request,
    )?;
    if current != *offer {
        return Err(ConstructionError::StaleQuote);
    }
    Ok(AcceptedCommand::BeginBuild {
        preview: current.preview,
        authority: grant.authority.clone(),
    })
}
