//! Original synthetic fixture only. Writes deterministic OBJ data to stdout.
use wonderland_render_3d::{
    city::facade::{bake_facade, to_obj},
    lot::{synthetic_lot, BuildOptions},
};
use wonderland_render_core::AssetKey;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let facade = bake_facade(
        &synthetic_lot(),
        &BuildOptions::default(),
        AssetKey([0x53; 32]),
        1,
        0.5,
    )?;
    let identity: String = facade
        .identity
        .0
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    eprintln!(
        "synthetic facade: identity={identity}, vertices={}, triangles={}, missing={:?}",
        facade.mesh.vertices.len(),
        facade.mesh.indices.len() / 3,
        facade.missing_assets
    );
    print!("{}", to_obj(&facade)?);
    Ok(())
}
