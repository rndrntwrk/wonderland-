//! Narrow consistency check for the native adapter's already-admitted posed meshes.
//!
//! Native animation changes CPU-skinned geometry on accepted ticks, not content-pack
//! identity. This is NOT server authentication: the native replica and avatar adapter
//! validate authority and resource inputs before constructing this projection.
use super::*;

pub(super) fn native_pose_update(previous: &WorldDocument, next: &WorldDocument) -> bool {
    if previous.provenance.kind != WorldSourceKind::LiveSession
        || next.provenance != previous.provenance
        || next.revision.tick <= previous.revision.tick
        || next.revision.lot_id != previous.revision.lot_id
        || next.revision.epoch != previous.revision.epoch
        || next.revision.content != previous.revision.content
        || next.materials != previous.materials
    {
        return false;
    }
    let is_pose = |model: &&WorldModel| matches!(model.context, ModelContext::Vitaboy);
    // Source object models still need a new content fence. Do not use a broad
    // exemption for every resource merely because one avatar is animated.
    if !previous
        .models
        .iter()
        .filter(|m| !is_pose(m))
        .eq(next.models.iter().filter(|m| !is_pose(m)))
    {
        return false;
    }
    let mut known_models = BTreeMap::new();
    let mut known_images = BTreeMap::new();
    for world in [previous, next] {
        let mut owners = vec![false; world.models.len()];
        for object in &world.objects {
            if let Some(index) = object.model
                && matches!(world.models[index].context, ModelContext::Vitaboy)
            {
                if object.entity.is_none()
                    || object.snapshot.is_some()
                    || object.blueprint.is_some()
                    || object.visual_revision != world.revision.tick
                {
                    return false;
                }
                owners[index] = true;
            }
        }
        for (index, model) in world.models.iter().enumerate() {
            if matches!(model.context, ModelContext::Vitaboy)
                && (!owners[index] || model.effective_content != model.effective_source)
            {
                return false;
            }
            // A repeated pose digest cannot hide changed geometry, in either
            // document. Different accepted poses/outfits may have distinct keys.
            if let Some(known) = known_models.insert(model.effective_source, model)
                && known != model
            {
                return false;
            }
            for texture in &model.textures {
                if let Some(known) = known_images.insert(texture.effective_asset, &texture.image)
                    && known != &texture.image
                {
                    return false;
                }
            }
        }
    }
    true
}
