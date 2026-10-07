//! Scoped history of presented samples, not VM state or a hidden tick replay.
use super::*;
use std::{
    io::{self, Write},
    sync::Arc,
};

/// Presentation working-set bound, not a server player/lot capacity limit.
/// Two poses per retained instance, bounded below by a total bone budget;
/// no old mesh or texture copies.
pub const MAX_RETAINED_AVATARS: usize = 1024;

struct Entry {
    guid: u32,
    before: Pose,
    current: Pose,
}
/// Retains the original skeleton's untouched channels across observed samples.
/// Clear at a new native checkpoint or imported-bank/session replacement. Neither
/// snapshots nor unseen ticks provide enough evidence to invent earlier bones.
#[derive(Default)]
pub struct NativeAvatarHistory {
    owner: Arc<()>,
    bank: Option<Arc<ImportedContent>>,
    revision: Option<WorldRevision>,
    fingerprint: Option<AssetKey>,
    entries: BTreeMap<wonderland_render_core::EntityRef, Entry>,
}
/// An immutable, one-commit preparation. The opaque owner token prevents an
/// out-of-order result or a preparation from another history committing here.
pub struct NativeAvatarSample {
    owner: Arc<()>,
    bank: Arc<ImportedContent>,
    revision: WorldRevision,
    fingerprint: AssetKey,
    projection: NativeAvatarProjection,
    entries: BTreeMap<wonderland_render_core::EntityRef, Entry>,
}
impl NativeAvatarSample {
    pub fn texture_keys(&self) -> &BTreeSet<AssetKey> {
        self.projection.texture_keys()
    }
}
fn same_scope(a: WorldRevision, b: WorldRevision) -> bool {
    a.lot_id == b.lot_id && a.epoch == b.epoch && a.content == b.content
}
impl NativeAvatarHistory {
    /// Detached preparation only. Re-evaluating the same tick starts from its
    /// saved BEFORE pose so weighted layers never compound on resource wakeups.
    pub fn prepare(
        &self,
        frame: &AvatarVisualFrame,
        world: &WorldDocument,
        bank: &Arc<ImportedContent>,
    ) -> Result<NativeAvatarSample, WorldError> {
        if frame.avatars.len() > MAX_RETAINED_AVATARS
            || bank.rig.as_ref().is_some_and(|rig| {
                frame.avatars.len().saturating_mul(rig.source().bones.len()) > 65_536
            })
        {
            return Err(WorldError(
                "Native retained-pose working-set limit exceeded.".into(),
            ));
        }
        let continuing = self.bank.as_ref().is_some_and(|old| Arc::ptr_eq(old, bank))
            && self
                .revision
                .is_some_and(|old| same_scope(old, frame.revision));
        let mut hash = Sha256::new();
        serde_json::to_writer(DigestWriter(&mut hash), &frame.avatars)
            .map_err(|e| WorldError(e.to_string()))?;
        let fingerprint = AssetKey(hash.finalize().into());
        let same_tick = continuing
            && self
                .revision
                .is_some_and(|old| old.tick == frame.revision.tick);
        if continuing
            && self
                .revision
                .is_some_and(|old| old.tick > frame.revision.tick)
            || same_tick
                && (self.revision != Some(frame.revision) || self.fingerprint != Some(fingerprint))
        {
            return Err(WorldError(
                "Stale or conflicting native pose sample.".into(),
            ));
        }
        let mut seeds = BTreeMap::new();
        if continuing {
            for avatar in &frame.avatars {
                let Ok(object_id) = u32::try_from(avatar.entity.object_id.0) else {
                    continue;
                };
                let id = wonderland_render_core::EntityRef {
                    object_id,
                    generation: avatar.entity.generation,
                };
                if let Some(previous) = self
                    .entries
                    .get(&id)
                    .filter(|entry| entry.guid == avatar.guid)
                {
                    seeds.insert(
                        id,
                        if same_tick {
                            previous.before.clone()
                        } else {
                            previous.current.clone()
                        },
                    );
                }
            }
        }
        let mut projection = NativeAvatarProjection::prepare_seeded(frame, world, bank, &seeds)?;
        let mut entries = BTreeMap::new();
        if let Some(rig) = bank.rig.as_ref() {
            for (id, (guid, current)) in std::mem::take(&mut projection.poses) {
                let before = seeds.remove(&id).unwrap_or_else(|| rig.bind_pose());
                entries.insert(
                    id,
                    Entry {
                        guid,
                        before,
                        current,
                    },
                );
            }
        }
        Ok(NativeAvatarSample {
            owner: Arc::clone(&self.owner),
            bank: Arc::clone(bank),
            revision: frame.revision,
            fingerprint,
            projection,
            entries,
        })
    }
    /// World and history commit together only after the existing scene/resource
    /// validation succeeds. Missing decoded pixels remain explicit diagnostics;
    /// a valid skeleton can still be retained while those pixels are loading.
    pub fn apply(
        &mut self,
        sample: NativeAvatarSample,
        world: &mut WorldDocument,
        decoded: &BTreeMap<AssetKey, RgbaImage>,
    ) -> Result<(), WorldError> {
        if !Arc::ptr_eq(&sample.owner, &self.owner) {
            return Err(WorldError("Superseded native pose preparation.".into()));
        }
        sample.projection.apply(world, &sample.bank, decoded)?;
        for diagnostic in &mut world.diagnostics {
            if diagnostic.code == "native_avatar_current_pose" {
                diagnostic.message = "Original resources sampled at the accepted frame with retained channels from observed poses. Pre-checkpoint/unobserved history, head seeking and container/bone attachments are not reconstructed.".into();
            }
        }
        self.owner = Arc::new(());
        self.bank = Some(sample.bank);
        self.revision = Some(sample.revision);
        self.fingerprint = Some(sample.fingerprint);
        self.entries = sample.entries; // Prunes removed generations immediately.
        Ok(())
    }
    pub fn clear(&mut self) {
        // A new token also fences already prepared results, even after clear.
        *self = Self::default();
    }
    pub fn retained_avatar_count(&self) -> usize {
        self.entries.len()
    }
}
// Hash the accepted input without allocating another complete JSON record.
struct DigestWriter<'a>(&'a mut Sha256);
impl Write for DigestWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
