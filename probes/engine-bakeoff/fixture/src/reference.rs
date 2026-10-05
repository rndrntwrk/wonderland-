use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::Path};
use wonderland_engine_fixture::{
    audio_reference, hash_hex, reference_frame, representative_scene, HEIGHT, WIDTH,
};
use wonderland_render_core::ViewMode;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).unwrap_or_else(|| "output".into());
    let path = Path::new(&path);
    fs::create_dir_all(path)?;
    let mut records = Vec::new();
    for (name, mode) in [
        ("full2d", ViewMode::Full2D),
        ("hybrid2d", ViewMode::Hybrid2D),
        ("full3d", ViewMode::Full3D),
    ] {
        for avatars in [32, 64] {
            let scene = representative_scene(mode, avatars, 30)?;
            let frame = reference_frame(&scene)?;
            let basename = format!("{name}-{avatars}");
            let mut image = fs::File::create(path.join(format!("{basename}.ppm")))?;
            write!(image, "P6\n{WIDTH} {HEIGHT}\n255\n")?;
            for pixel in &frame.image.pixels {
                image.write_all(&pixel[..3])?;
            }
            let mut ids = fs::File::create(path.join(format!("{basename}.ids")))?;
            for id in &frame.ids {
                ids.write_all(&id.map_or(0, |id| id.object_id).to_le_bytes())?;
                ids.write_all(&id.map_or(0, |id| id.generation).to_le_bytes())?;
            }
            let mut depth = fs::File::create(path.join(format!("{basename}.depth")))?;
            for value in &frame.depths {
                depth.write_all(&value.to_le_bytes())?;
            }
            let vertices: usize = scene
                .draws
                .iter()
                .map(|draw| draw.mesh.vertices.len())
                .sum();
            let triangles: usize = scene
                .draws
                .iter()
                .map(|draw| draw.mesh.indices.len() / 3)
                .sum();
            let selected_pixels = frame.ids.iter().filter(|id| id.is_some()).count();
            let entry = format!(
                r#"{{"name":"{basename}","mode":"{name}","avatars":{avatars},"tick":30,"fixtureHash":"{}","referenceHash":"{}","vertices":{vertices},"triangles":{triangles},"pickablePixels":{selected_pixels}}}"#,
                hash_hex(scene.hash),
                hash_hex(frame.digest)
            );
            println!("{entry}");
            records.push(entry);
        }
    }
    let mut audio = Vec::new();
    for hz in [30, 60, 120] {
        let (pcm, starts) = audio_reference(hz)?;
        let bytes: Vec<_> = pcm.iter().flat_map(|sample| sample.to_le_bytes()).collect();
        let hash: [u8; 32] = Sha256::digest(&bytes).into();
        if hz == 30 {
            fs::write(path.join("audio-reference.pcm16le"), &bytes)?;
        }
        audio.push(format!(
            r#"{{"renderHz":{hz},"acceptedCues":{starts},"samples":{},"pcmHash":"{}"}}"#,
            pcm.len(),
            hash_hex(hash)
        ));
    }
    fs::write(path.join("manifest.json"), format!("{{\n\"evidence\":\"synthetic-cpu-reference\",\n\"width\":{WIDTH},\"height\":{HEIGHT},\n\"scenes\":[{}],\n\"audio\":[{}]\n}}\n", records.join(",\n"), audio.join(",\n")))?;
    Ok(())
}
