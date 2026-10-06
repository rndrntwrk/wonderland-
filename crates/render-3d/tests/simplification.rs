use wonderland_render_3d::{reconstruction::simplification::*, Error};
use wonderland_render_core::{Mesh, RenderLimits, Vec2, Vec3, Vertex};

fn grid(side: usize) -> Mesh {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for y in 0..side {
        for x in 0..side {
            vertices.push(Vertex {
                position: Vec3::new(x as f32 / 32., y as f32 / 32., 0.),
                normal: Vec3::Z,
                uv: Vec2::new(x as f32 / side as f32, y as f32 / side as f32),
                color: [1.; 4],
            });
        }
    }
    for y in 0..side - 1 {
        for x in 0..side - 1 {
            let a = (y * side + x) as u32;
            let n = side as u32;
            indices.extend([a, a + 1, a + n + 1, a, a + n + 1, a + n]);
        }
    }
    Mesh { vertices, indices }
}

#[test]
fn source_quadric_schedule_reduces_a_plane_deterministically() {
    let mesh = grid(17);
    let before = mesh.clone();
    let mut options = SimplificationOptions::source(mesh.indices.len() / 3);
    options.target_triangles = 32;
    let a = simplify_mesh(&mesh, options).unwrap();
    let b = simplify_mesh(&mesh, options).unwrap();
    assert_eq!(a.mesh.indices, b.mesh.indices);
    assert_eq!(a.mesh.vertices, b.mesh.vertices);
    assert_eq!(mesh, before);
    assert!(a.remaining_triangles < a.original_triangles);
    assert_eq!(a.target_reached, a.remaining_triangles <= 32);
    assert!(a.iterations_used <= 125);
    a.mesh.validate(&RenderLimits::default()).unwrap();
    for v in &a.mesh.vertices {
        assert_eq!(v.position.z, 0.);
        assert!((0.0..=0.5).contains(&v.position.x));
        assert!((0.0..=0.5).contains(&v.position.y));
        assert!(v.normal.z > 0.99);
    }
    for t in a.mesh.indices.chunks_exact(3) {
        let p = t
            .iter()
            .map(|i| a.mesh.vertices[*i as usize].position)
            .collect::<Vec<_>>();
        assert!((p[1] - p[0]).cross(p[2] - p[0]).z > 0.);
    }
}

#[test]
fn no_op_target_compacts_orphans_and_preserves_the_surface() {
    let mut mesh = grid(3);
    let expected = mesh.indices.clone();
    mesh.vertices.push(Vertex {
        position: Vec3::new(100., 100., 100.),
        normal: Vec3::Z,
        uv: Vec2::ZERO,
        color: [1.; 4],
    });
    let mut options = SimplificationOptions::source(8);
    options.target_triangles = 8;
    let out = simplify_mesh(&mesh, options).unwrap();
    assert_eq!(out.mesh.vertices.len(), 9);
    assert_eq!(out.mesh.indices, expected);
    assert_eq!(out.iterations_used, 0);
    assert!(out.target_reached);
}

#[test]
fn simplification_work_and_reference_budgets_fail_without_mutating_input() {
    let mesh = grid(8);
    let before = mesh.clone();
    let mut options = SimplificationOptions::source(mesh.indices.len() / 3);
    options.max_operations = 0;
    assert!(matches!(
        simplify_mesh(&mesh, options),
        Err(Error::BudgetExceeded(_))
    ));
    options.max_operations = 1_000_000;
    options.max_reference_entries = 3;
    assert!(matches!(
        simplify_mesh(&mesh, options),
        Err(Error::BudgetExceeded(_))
    ));
    assert_eq!(mesh, before);
}

#[test]
fn malformed_quadric_inputs_are_rejected_before_collapsing() {
    let mut mesh = grid(3);
    mesh.indices[0] = u32::MAX;
    assert!(simplify_mesh(&mesh, SimplificationOptions::source(8)).is_err());
    let mut mesh = grid(3);
    mesh.vertices[1].position = mesh.vertices[0].position;
    assert!(simplify_mesh(&mesh, SimplificationOptions::source(8)).is_err());
    let mesh = grid(3);
    let mut options = SimplificationOptions::source(8);
    options.aggressiveness = f64::NAN;
    assert!(simplify_mesh(&mesh, options).is_err());
}

fn source_bowl() -> Mesh {
    let mut mesh = grid(9);
    for (i, vertex) in mesh.vertices.iter_mut().enumerate() {
        let x = (i % 9) as i32 - 3;
        let y = (i / 9) as i32 - 3;
        vertex.position.z = (x * x + y * y) as f32 / 4096.;
    }
    mesh
}

#[test]
fn curved_surface_matches_unmodified_csharp_collapse_schedule() {
    // Oracle: TSOClient/tso.common/MeshSimplify/Simplify.cs executed without
    // changes against the repository's MonoGame DLL. Input is the 9x9 grid
    // above, z=((x-3)^2+(y-3)^2)/4096, target 8, aggressiveness 3.5.
    // Intermediate iterations make an arithmetic-induced schedule drift
    // visible even if both implementations eventually reach the target.
    let mesh = source_bowl();
    for (iterations, vertices, triangles) in [
        (1, 76, 123),
        (5, 32, 48),
        (6, 29, 43),
        (20, 11, 13),
        (125, 7, 8),
    ] {
        let mut options = SimplificationOptions::source(128);
        options.target_triangles = 8;
        options.iterations = iterations;
        let output = simplify_mesh(&mesh, options).unwrap();
        assert_eq!(
            (output.mesh.vertices.len(), output.remaining_triangles),
            (vertices, triangles),
            "source collapse schedule at {iterations} iterations"
        );
    }
}

#[test]
fn curved_surface_preserves_source_position_uv_bits_and_triangle_order() {
    let mut options = SimplificationOptions::source(128);
    options.target_triangles = 8;
    let output = simplify_mesh(&source_bowl(), options).unwrap();
    // The same unmodified C# oracle emits these f32 bits in compacted vertex
    // order. Checking exact bits protects source normalization, reciprocal
    // division, quadric-minor order and Vector2.Lerp arithmetic together.
    let expected = [
        [0x3e340000, 0x3b800000, 0x3b880000, 0x3f200000, 0x3c638e37],
        [0x3df7bdfd, 0x3d92cab2, 0xba6b9714, 0x3ed7ec8a, 0x3e86fabe],
        [0x3e740000, 0x3e2c0000, 0x3c000000, 0x3f58e38e, 0x3f18e38e],
        [0x3cc00000, 0x3d700000, 0x3b2e0000, 0x3daaaaaa, 0x3e555556],
        [0x3e4548a0, 0x3e34166c, 0x3b71db81, 0x3f34027f, 0x3f209f8a],
        [0x3db8f21c, 0x3e3b6a43, 0x3ab63152, 0x3ea1cba5, 0x3f29e033],
        [0x3d700000, 0x3e6c0000, 0x3bc40000, 0x3e555557, 0x3f51c71d],
    ];
    let actual: Vec<_> = output
        .mesh
        .vertices
        .iter()
        .map(|v| [v.position.x, v.position.y, v.position.z, v.uv.x, v.uv.y].map(f32::to_bits))
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(
        output.mesh.indices,
        [3, 0, 1, 1, 0, 2, 1, 2, 4, 3, 1, 5, 1, 4, 5, 3, 5, 6, 5, 4, 6, 4, 2, 6]
    );
}
