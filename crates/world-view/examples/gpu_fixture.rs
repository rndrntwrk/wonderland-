//! Synthetic source document and independent CPU pixels for the browser GPU gate.
use std::{fs, path::PathBuf, sync::Arc};
use wonderland_world_view::*;
#[path = "support/lighting.rs"]
mod lighting_fixture;
#[path = "support/masked.rs"]
mod mask_fixture;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("supply an output directory")?,
    );
    fs::create_dir_all(&output)?;
    let mut source = WorldDocument::from_blueprint_xml(
        "<house><size>4</size><world><floors><floor level=\"0\" x=\"1\" y=\"1\" value=\"9\"/></floors><walls><wall level=\"0\" x=\"2\" y=\"2\" segments=\"1\" tlp=\"1\" trp=\"1\" blp=\"1\" brp=\"1\" tls=\"1\" trs=\"1\"/></walls></world><objects/></house>",
        "tests:source GPU fixture",
        "gpu-browser-v1",
    )?;
    source.provenance.kind = WorldSourceKind::TestFixture;
    source.source_counts = None;
    let mut mixed = mask_fixture::masked(ModelMaskKind::Portal);
    let mut unmasked = mixed.models[0].clone();
    unmasked.effective_source = wonderland_render_core::AssetKey([33; 32]);
    unmasked.depth_mask = None;
    unmasked.textures[0].effective_asset = wonderland_render_core::AssetKey([34; 32]);
    unmasked.textures[0].image.pixels[0] = [220, 20, 20, 160];
    mixed.models.push(unmasked);
    let mut other = mixed.objects[0].clone();
    other.entity.as_mut().unwrap().object_id = 43;
    other.model = Some(1);
    other.position_tiles = wonderland_render_core::Vec3::new(2.5, 1.5, 0.);
    other.yaw_radians = 1.2;
    mixed.objects.push(other);
    let document = Arc::new(source);
    let shadowed = lighting_fixture::lit_world();
    let mut unshadowed = shadowed.clone();
    unshadowed.lighting.as_mut().unwrap().geometry[0]
        .walls
        .clear();
    let mut scenes = vec![];
    for (name, controls, document) in [
        (
            "default",
            ViewportControls::default(),
            Arc::clone(&document),
        ),
        (
            "rotated",
            ViewportControls {
                yaw_radians: 0.8,
                ..Default::default()
            },
            Arc::clone(&document),
        ),
        (
            "cutaway",
            ViewportControls {
                walls: WallMode::Down,
                ..Default::default()
            },
            Arc::clone(&document),
        ),
        (
            "normal-mask",
            ViewportControls::default(),
            Arc::new(mask_fixture::masked(ModelMaskKind::Normal)),
        ),
        (
            "portal-mask",
            ViewportControls::default(),
            Arc::new(mask_fixture::masked(ModelMaskKind::Portal)),
        ),
        ("mixed-mask", ViewportControls::default(), Arc::new(mixed)),
        (
            "room-lit",
            ViewportControls::default(),
            Arc::new(unshadowed),
        ),
        (
            "room-shadow",
            ViewportControls::default(),
            Arc::new(shadowed),
        ),
    ] {
        fs::write(
            output.join(format!("{name}.world.json")),
            serde_json::to_vec(document.as_ref())?,
        )?;
        let mut renderer = WorldRenderer::new(Arc::clone(&document))?;
        let mut cpu = WorldRenderer::new(document)?;
        let (frame, stats) = renderer.prepare_gpu(controls, 256, 192)?;
        cpu.render(controls, 256, 192)?;
        let image = cpu.image().ok_or("CPU reference missing")?;
        fs::write(
            output.join(format!("{name}.rgba")),
            image.pixels.as_flattened(),
        )?;
        fs::write(
            output.join(format!("{name}.json")),
            serde_json::to_vec(&frame)?,
        )?;
        let generation = frame.generation.parse()?;
        let mut picks = vec![];
        let mut ownerless = 0;
        for y in (3..189).step_by(5) {
            for x in (3..253).step_by(5) {
                let target = cpu.pick(x, y).map(|pick| pick.target);
                if !(-2..=2).all(|dy| {
                    (-2..=2).all(|dx| {
                        cpu.pick((x as i32 + dx) as u32, (y as i32 + dy) as u32)
                            .map(|pick| pick.target)
                            == target
                    })
                }) {
                    continue;
                }
                let object = matches!(target, Some(WorldPickTarget::Object { .. }));
                let index = if let Some(target) = target {
                    frame
                        .draws
                        .iter()
                        .find_map(|draw| {
                            renderer
                                .resolve_gpu_pick(generation, draw.pick_id, x, y)
                                .filter(|pick| pick.target == target)
                                .map(|_| draw.pick_id)
                        })
                        .ok_or("GPU omitted CPU identity")?
                } else {
                    ownerless += 1;
                    0
                };
                let color = image.pixels[(y * 256 + x) as usize];
                let portal_final = color[2] > 100 && color[2] > color[0].saturating_mul(2);
                picks.push(serde_json::json!({"x": x, "y": y, "index": index, "object": object, "portal_final": portal_final}));
            }
        }
        scenes.push(
            serde_json::json!({"name":name,"width":256,"height":192,"generation":frame.generation,
            "triangles":stats.triangles,"picks":picks,"ownerless":ownerless}),
        );
    }
    fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&scenes)?,
    )?;
    Ok(())
}
