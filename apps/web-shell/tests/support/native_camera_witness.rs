//! Test-only geometry: keep strict compositor visibility at edge-on camera angles.
//! The original planar parser/pose fixtures and all their resource bytes stay unchanged.
fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|n| n.to_be_bytes()).collect()
}
pub fn volumetric(mut result: Vec<(String, Vec<u8>)>) -> Vec<(String, Vec<u8>)> {
    let points = [
        [-0.5_f32, 0., -0.5],
        [0.5, 0., -0.5],
        [0., 4., 0.],
        [0., 0., 0.5],
    ];
    let mut mesh = words(&[2, 1]);
    mesh.push(4);
    mesh.extend(b"ROOT");
    // Four faces, one ROOT binding covering all four real vertices, four UVs.
    mesh.extend(words(&[4, 0, 1, 2, 0, 3, 1, 1, 3, 2, 2, 3, 0]));
    mesh.extend(words(&[1, 0, 0, 4, 0, 0, 4]));
    for n in [0_f32, 0., 1., 0., 0., 1., 1., 1.] {
        mesh.extend(n.to_le_bytes());
    }
    mesh.extend(words(&[0, 4]));
    for point in points {
        for n in point.into_iter().chain([0., 0., 1.]) {
            mesh.extend(n.to_le_bytes());
        }
    }
    result
        .iter_mut()
        .find(|(name, _)| name.ends_with(".mesh"))
        .unwrap()
        .1 = mesh;
    result
}
