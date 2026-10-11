use wonderland_engine_fixture::{reference_frame, representative_scene};
use wonderland_render_core::ViewMode;

#[test]
fn later_coplanar_mesh_matches_the_engines_source_less_equal_policy() {
    let mut scene = representative_scene(ViewMode::Full2D, 0, 30).unwrap();
    let mut first = scene.draws[0].clone();
    let mut second = first.clone();
    first.material.color = [1.0; 4];
    second.material.color = [1.0; 4];
    for vertex in &mut first.mesh.vertices {
        vertex.color = [1.0, 0.0, 0.0, 1.0];
    }
    for vertex in &mut second.mesh.vertices {
        vertex.color = [0.0, 0.0, 1.0, 1.0];
    }
    scene.draws = vec![first, second];
    scene.sprites.clear();
    let output = reference_frame(&scene).unwrap();
    assert!(output
        .image
        .pixels
        .iter()
        .any(|pixel| *pixel == [0, 0, 255, 255]));
    assert!(!output
        .image
        .pixels
        .iter()
        .any(|pixel| *pixel == [255, 0, 0, 255]));
}
