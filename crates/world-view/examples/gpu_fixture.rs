//! Synthetic source document and independent CPU pixels for the browser GPU gate.
use std::{fs, path::PathBuf, sync::Arc};
use wonderland_world_view::*;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = PathBuf::from(std::env::args().nth(1).ok_or("supply an output directory")?);
    fs::create_dir_all(&output)?;
    let mut source = WorldDocument::from_blueprint_xml(
        "<house><size>4</size><world><floors><floor level=\"0\" x=\"1\" y=\"1\" value=\"9\"/></floors><walls><wall level=\"0\" x=\"2\" y=\"2\" segments=\"1\" topLeftStyle=\"1\"/></walls></world><objects/></house>",
        "tests:source GPU fixture", "gpu-browser-v1",
    )?;
    source.provenance.kind = WorldSourceKind::TestFixture;
    source.source_counts = None;
    let document = Arc::new(source);
    let mut renderer = WorldRenderer::new(Arc::clone(&document))?;
    let mut cpu = WorldRenderer::new(document)?;
    let mut scenes = vec![];
    for (name, controls) in [
        ("default", ViewportControls::default()),
        ("rotated", ViewportControls { yaw_radians: 0.8, ..Default::default() }),
        ("cutaway", ViewportControls { walls: WallMode::Down, ..Default::default() }),
    ] {
        let (frame, stats) = renderer.prepare_gpu(controls, 256, 192)?;
        cpu.render(controls, 256, 192)?;
        let image = cpu.image().ok_or("CPU reference missing")?;
        fs::write(output.join(format!("{name}.rgba")), image.pixels.as_flattened())?;
        fs::write(output.join(format!("{name}.json")), serde_json::to_vec(&frame)?)?;
        let generation = frame.generation.parse()?;
        let mut picks = vec![];
        let mut ownerless = 0;
        for y in (3..189).step_by(5) {
            for x in (3..253).step_by(5) {
                let target = cpu.pick(x, y).map(|pick| pick.target);
                if !(-2..=2).all(|dy| (-2..=2).all(|dx| {
                    cpu.pick((x as i32 + dx) as u32, (y as i32 + dy) as u32).map(|pick| pick.target) == target
                })) { continue; }
                let index = if let Some(target) = target {
                    frame.draws.iter().find_map(|draw| {
                        renderer.resolve_gpu_pick(generation, draw.pick_id, x, y)
                            .filter(|pick| pick.target == target).map(|_| draw.pick_id)
                    }).ok_or("GPU omitted CPU identity")?
                } else { ownerless += 1; 0 };
                picks.push(serde_json::json!({"x": x, "y": y, "index": index}));
            }
        }
        scenes.push(serde_json::json!({"name":name,"width":256,"height":192,"generation":frame.generation,
            "triangles":stats.triangles,"picks":picks,"ownerless":ownerless}));
    }
    fs::write(output.join("manifest.json"), serde_json::to_vec_pretty(&scenes)?)?;
    Ok(())
}
