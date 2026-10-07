//! Retained presentation state, never authoritative simulation state. The caller
//! feeds EVERY accepted frame, including frames not drawn and invisible avatars.
use super::{ADULT, resolve_timeline};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use wonderland_avatar_content::ImportedContent;
use wonderland_avatar_view::{Pose, PosePlayer};
use wonderland_game_runtime::{AvatarVisual, AvatarVisualFrame, EntityRef};
use wonderland_render_core::AssetKey;
use wonderland_world_view::{WorldError, WorldRevision};

/// Presentation budgets, not player/admission limits. Resource failures diagnose
/// the affected appearance rather than changing any authoritative game outcome.
const MAX_RETAINED_AVATARS: usize = 1024;
const MAX_RETAINED_BONES: usize = 65_536;
#[derive(Clone)]
struct Entry {
    guid: u32,
    pose: Result<PosePlayer, String>,
}
#[derive(Default)]
pub struct NativeAvatarPoseHistory {
    bank: Option<Arc<ImportedContent>>,
    revision: Option<WorldRevision>,
    fingerprint: Option<AssetKey>,
    entries: BTreeMap<EntityRef, Entry>,
}
fn identity(frame: &AvatarVisualFrame) -> Result<AssetKey, WorldError> {
    let mut seen = BTreeSet::new();
    let mut hash = Sha256::new();
    if frame.avatars.len() > MAX_RETAINED_AVATARS {
        return Err(WorldError(
            "Native retained avatar record budget exceeded.".into(),
        ));
    }
    for avatar in &frame.avatars {
        if avatar.entity.object_id.0 <= 0
            || avatar.entity.generation == 0
            || !seen.insert(avatar.entity)
        {
            return Err(WorldError(
                "Invalid or duplicate retained avatar identity.".into(),
            ));
        }
        hash.update(
            serde_json::to_vec(avatar)
                .map_err(|_| WorldError("Invalid retained avatar data.".into()))?,
        );
    }
    Ok(AssetKey(hash.finalize().into()))
}
fn same_scope(a: WorldRevision, b: WorldRevision) -> bool {
    (a.lot_id, a.epoch, a.content) == (b.lot_id, b.epoch, b.content)
}
impl NativeAvatarPoseHistory {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Commit once per tick from the ordered accepted trace. Repeated renders or
    /// texture notifications at that tick cannot compound blends. Missing ticks,
    /// changed content/rig/lot and explicit checkpoints reset unknown history.
    pub fn observe(
        &mut self,
        frame: &AvatarVisualFrame,
        bank: Arc<ImportedContent>,
    ) -> Result<(), WorldError> {
        let fingerprint = identity(frame)?;
        let same_bank = self
            .bank
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(old, &bank));
        let same_source = self
            .revision
            .is_some_and(|old| same_scope(old, frame.revision));
        if same_bank
            && same_source
            && let Some(old) = self.revision
        {
            if frame.revision.tick < old.tick {
                return Err(WorldError("Stale retained avatar tick.".into()));
            }
            if frame.revision.tick == old.tick {
                return if self.revision == Some(frame.revision)
                    && self.fingerprint == Some(fingerprint)
                {
                    Ok(())
                } else {
                    Err(WorldError(
                        "Retained avatar state changed at the same accepted tick.".into(),
                    ))
                };
            }
        }
        let contiguous = same_bank
            && same_source
            && self
                .revision
                .is_some_and(|old| old.tick.checked_add(1) == Some(frame.revision.tick));
        let mut next = BTreeMap::new();
        let mut bones = 0usize;
        for avatar in &frame.avatars {
            let pose = (|| {
                let rig = bank
                    .rig
                    .as_ref()
                    .ok_or("The original avatar skeleton is not loaded.")?;
                if avatar.guid != ADULT || !rig.source().name.eq_ignore_ascii_case("adult") {
                    return Err(
                        "This avatar requires its original OBJD-to-skeleton mapping.".into(),
                    );
                }
                if avatar.container.is_some() {
                    return Err(
                        "Container-bound avatars require their original attachment adapter.".into(),
                    );
                }
                let count = rig.source().bones.len();
                if count > MAX_RETAINED_BONES.saturating_sub(bones) {
                    return Err("Native retained bone budget exceeded.".into());
                }
                let timeline = resolve_timeline(avatar, &bank)?;
                let mut player = if contiguous {
                    self.entries
                        .get(&avatar.entity)
                        .filter(|e| e.guid == avatar.guid)
                        .and_then(|e| e.pose.as_ref().ok())
                        .cloned()
                } else {
                    None
                }
                .unwrap_or_else(|| {
                    PosePlayer::new(
                        wonderland_render_core::EntityRef {
                            object_id: avatar.entity.object_id.0 as u32,
                            generation: avatar.entity.generation,
                        },
                        rig,
                    )
                });
                player
                    .commit(frame.revision.tick, rig, timeline)
                    .map_err(|e| e.to_string())?;
                bones += count;
                Ok(player)
            })();
            next.insert(
                avatar.entity,
                Entry {
                    guid: avatar.guid,
                    pose,
                },
            );
        }
        // Atomic replacement also drops disappeared entities and obsolete clip handles.
        self.entries = next;
        self.bank = Some(bank);
        self.revision = Some(frame.revision);
        self.fingerprint = Some(fingerprint);
        Ok(())
    }
    pub(super) fn validate_frame(
        &self,
        frame: &AvatarVisualFrame,
        bank: &ImportedContent,
    ) -> Result<(), WorldError> {
        if self.revision != Some(frame.revision)
            || self
                .bank
                .as_ref()
                .is_none_or(|old| !std::ptr::eq(Arc::as_ptr(old), bank))
            || self.fingerprint != Some(identity(frame)?)
        {
            return Err(WorldError(
                "Retained pose does not belong to the current frame and content bank.".into(),
            ));
        }
        Ok(())
    }
    pub(super) fn pose(&self, avatar: &AvatarVisual) -> Result<&Pose, String> {
        let entry = self
            .entries
            .get(&avatar.entity)
            .filter(|e| e.guid == avatar.guid)
            .ok_or("Retained avatar pose is unavailable.")?;
        entry
            .pose
            .as_ref()
            .map(PosePlayer::retained)
            .map_err(Clone::clone)
    }
}
