//! Export the actual original XML world and its depth-rendered image for review.
use std::{
    fs::File,
    io::{BufWriter, Write},
    sync::Arc,
    time::Instant,
};
use wonderland_world_view::{ViewportControls, WorldDocument, WorldRenderer};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args().skip(1);
    let output = arguments
        .next()
        .ok_or("usage: original_lot <output.ppm> [world.json]")?;
    let document = Arc::new(WorldDocument::original_empty_lot()?);
    if let Some(path) = arguments.next() {
        serde_json::to_writer(BufWriter::new(File::create(path)?), &*document)?;
    }
    let mut renderer = WorldRenderer::new(Arc::clone(&document))?;
    let start = Instant::now();
    let stats = renderer.render(ViewportControls::default(), 768, 512)?;
    let initial = start.elapsed();
    let start = Instant::now();
    renderer.render(ViewportControls::default(), 768, 512)?;
    let cached = start.elapsed();
    let image = renderer.image().ok_or("renderer produced no image")?;
    let mut writer = BufWriter::new(File::create(output)?);
    write!(writer, "P6\n{} {}\n255\n", image.width, image.height)?;
    for pixel in &image.pixels {
        writer.write_all(&pixel[..3])?;
    }
    writer.flush()?;
    eprintln!(
        "Original source: {}\n{} × {} tiles; {} triangles; {} diagnostics\nNative debug sample: initial {initial:?}, cached geometry {cached:?}",
        document.provenance.origin,
        document.lot.width,
        document.lot.height,
        stats.triangles,
        stats.diagnostics.len()
    );
    Ok(())
}
