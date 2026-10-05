//! Small synthetic inputs; no original game payloads.
use crate::*;
use wonderland_render_core::{math::*, AssetKey};
pub fn skeleton() -> Skeleton {
    let mut bones = Vec::new();
    for (i, name) in ["ROOT", "HEAD"].into_iter().enumerate() {
        bones.push(Bone {
            unknown: 0,
            name: name.into(),
            parent_name: if i == 0 { "NULL" } else { "ROOT" }.into(),
            properties_flag: 0,
            properties: PropertyList::default(),
            translation: bits3(if i == 0 { Vec3::ZERO } else { Vec3::Y }),
            rotation: bits4(Quat::IDENTITY),
            can_translate: 1,
            can_rotate: 1,
            can_blend: 1,
            wiggle_value: F32Bits(0),
            wiggle_power: F32Bits(0),
            index: i,
            parent: if i == 0 { None } else { Some(0) },
            children: if i == 0 { vec![1] } else { vec![] },
        });
    }
    Skeleton {
        version: 1,
        name: "synthetic".into(),
        bones,
        root: 0,
        coordinate_policy: CoordinatePolicy::FreeSo,
    }
}
pub fn synthetic_rig() -> Rig {
    Rig::new(skeleton(), AssetKey([11; 32]), AvatarLimits::default()).unwrap()
}
pub fn source_mesh() -> SourceMesh {
    SourceMesh {
        version: 2,
        bone_names: vec!["root".into(), "head".into()],
        faces: vec![[0, 1, 2]],
        bindings: vec![
            BoneBinding {
                bone_index: 0,
                first_real_vertex: 0,
                real_vertex_count: 3,
                first_blend_vertex: 0,
                blend_vertex_count: 0,
            },
            BoneBinding {
                bone_index: 1,
                first_real_vertex: 0,
                real_vertex_count: 0,
                first_blend_vertex: 0,
                blend_vertex_count: 1,
            },
        ],
        vertices: [
            Vec3::new(-0.5, 0.0, 0.0),
            Vec3::new(0.5, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ]
        .into_iter()
        .map(|p| SourceVertex {
            texture_coordinate: [F32Bits(0); 2],
            position: bits3(p),
            normal: bits3(Vec3::Z),
        })
        .collect(),
        blend_vertices: vec![BlendVertex {
            raw_weight: 8192,
            other_vertex: 0,
            position: bits3(Vec3::ZERO),
            normal: bits3(Vec3::ZERO),
        }],
        repeated_real_vertex_count: 3,
        coordinate_policy: CoordinatePolicy::FreeSo,
    }
}
pub fn synthetic_mesh(rig: &Rig) -> PreparedMesh {
    PreparedMesh::prepare(
        rig,
        &source_mesh(),
        AssetKey([12; 32]),
        AvatarLimits::default(),
    )
    .unwrap()
}
pub fn animation(name: &str, a: f32, b: f32) -> Animation {
    Animation {
        version: 2,
        name: name.into(),
        duration_ms: F32Bits::from_f32(100.0),
        distance: F32Bits(0),
        is_moving: 0,
        translations: vec![bits3(Vec3::new(a, 0.0, 0.0)), bits3(Vec3::new(b, 0.0, 0.0))],
        rotations: vec![],
        motions: vec![Motion {
            unknown: 0,
            bone_name: "HEAD".into(),
            frame_count: 2,
            duration_ms: F32Bits::from_f32(100.0),
            translation_flag: 1,
            rotation_flag: 0,
            first_translation_index: 0,
            first_rotation_index: -1,
            properties_flag: 0,
            properties: vec![],
            time_properties_flag: 0,
            time_properties: vec![],
        }],
        num_frames: 3,
        coordinate_policy: CoordinatePolicy::FreeSo,
    }
}

/// Synthetic 14-bone mannequin; 13 boxes, 312 vertices, 156 triangles.
/// Each surface retains different primary/parent bone-local positions.
pub fn representative_rig() -> Rig {
    let definitions = [
        ("ROOT", None, Vec3::ZERO),
        ("PELVIS", Some(0), Vec3::new(0.0, 1.10, 0.0)),
        ("SPINE", Some(1), Vec3::new(0.0, 0.48, 0.0)),
        ("HEAD", Some(2), Vec3::new(0.0, 0.60, 0.0)),
        ("R_UPPER", Some(2), Vec3::new(-0.48, 0.30, 0.0)),
        ("R_LOWER", Some(4), Vec3::new(-0.40, -0.12, 0.0)),
        ("R_FINGER0", Some(5), Vec3::new(-0.34, -0.12, 0.0)),
        ("L_UPPER", Some(2), Vec3::new(0.48, 0.30, 0.0)),
        ("L_LOWER", Some(7), Vec3::new(0.40, -0.12, 0.0)),
        ("L_FINGER0", Some(8), Vec3::new(0.34, -0.12, 0.0)),
        ("R_THIGH", Some(1), Vec3::new(-0.20, -0.12, 0.0)),
        ("R_CALF", Some(10), Vec3::new(0.0, -0.45, 0.0)),
        ("L_THIGH", Some(1), Vec3::new(0.20, -0.12, 0.0)),
        ("L_CALF", Some(12), Vec3::new(0.0, -0.45, 0.0)),
    ];
    let mut bones = Vec::new();
    for (i, (name, parent, translation)) in definitions.iter().enumerate() {
        bones.push(Bone {
            unknown: 0,
            name: (*name).into(),
            parent_name: parent.map(|p| definitions[p].0).unwrap_or("NULL").into(),
            properties_flag: 0,
            properties: PropertyList::default(),
            translation: bits3(*translation),
            rotation: bits4(Quat::IDENTITY),
            can_translate: 1,
            can_rotate: 1,
            can_blend: 1,
            wiggle_value: F32Bits(0),
            wiggle_power: F32Bits(0),
            index: i,
            parent: *parent,
            children: definitions
                .iter()
                .enumerate()
                .filter(|(_, (_, p, _))| *p == Some(i))
                .map(|(j, _)| j)
                .collect(),
        });
    }
    Rig::new(
        Skeleton {
            version: 1,
            name: "synthetic-articulated-mannequin".into(),
            bones,
            root: 0,
            coordinate_policy: CoordinatePolicy::FreeSo,
        },
        AssetKey([61; 32]),
        AvatarLimits::default(),
    )
    .unwrap()
}
pub fn representative_mesh(rig: &Rig) -> PreparedMesh {
    let pose = rig.bind_pose();
    let mut mesh = SourceMesh {
        version: 2,
        bone_names: rig.source().bones.iter().map(|b| b.name.clone()).collect(),
        faces: vec![],
        bindings: vec![],
        vertices: vec![],
        blend_vertices: vec![],
        repeated_real_vertex_count: 0,
        coordinate_policy: CoordinatePolicy::FreeSo,
    };
    let face_corners = [
        ([0, 3, 2, 1], Vec3::new(0.0, 0.0, -1.0)),
        ([4, 5, 6, 7], Vec3::Z),
        ([0, 4, 7, 3], -Vec3::X),
        ([1, 2, 6, 5], Vec3::X),
        ([0, 1, 5, 4], -Vec3::Y),
        ([3, 7, 6, 2], Vec3::Y),
    ];
    for bone in 1..rig.source().bones.len() {
        let name = &rig.source().bones[bone].name;
        let (center, half) = match name.as_str() {
            "PELVIS" => (Vec3::ZERO, Vec3::new(0.33, 0.18, 0.20)),
            "SPINE" => (Vec3::new(0.0, 0.06, 0.0), Vec3::new(0.38, 0.30, 0.20)),
            "HEAD" => (Vec3::new(0.0, 0.18, 0.0), Vec3::new(0.22, 0.24, 0.22)),
            "R_UPPER" | "R_LOWER" => (Vec3::new(-0.18, -0.06, 0.0), Vec3::new(0.24, 0.12, 0.13)),
            "L_UPPER" | "L_LOWER" => (Vec3::new(0.18, -0.06, 0.0), Vec3::new(0.24, 0.12, 0.13)),
            "R_FINGER0" | "L_FINGER0" => (Vec3::ZERO, Vec3::new(0.12, 0.11, 0.12)),
            _ => (Vec3::new(0.0, -0.23, 0.0), Vec3::new(0.14, 0.26, 0.15)),
        };
        let corners = [
            Vec3::new(-1.0, -1.0, -1.0),
            Vec3::new(1.0, -1.0, -1.0),
            Vec3::new(1.0, 1.0, -1.0),
            Vec3::new(-1.0, 1.0, -1.0),
            Vec3::new(-1.0, -1.0, 1.0),
            Vec3::new(1.0, -1.0, 1.0),
            Vec3::new(1.0, 1.0, 1.0),
            Vec3::new(-1.0, 1.0, 1.0),
        ]
        .map(|v| center + Vec3::new(v.x * half.x, v.y * half.y, v.z * half.z));
        let first = mesh.vertices.len() as i32;
        let blend_first = mesh.blend_vertices.len() as i32;
        let parent = rig.source().bones[bone].parent.unwrap();
        let parent_inverse = pose.palette[parent].inverse().unwrap();
        for (indices, normal) in face_corners {
            let base = mesh.vertices.len() as i32;
            for (i, corner) in indices.into_iter().enumerate() {
                let p = corners[corner];
                let uv = [(i == 1 || i == 2) as u8 as f32, (i >= 2) as u8 as f32];
                mesh.vertices.push(SourceVertex {
                    texture_coordinate: uv.map(F32Bits::from_f32),
                    position: bits3(p),
                    normal: bits3(normal),
                });
                mesh.blend_vertices.push(BlendVertex {
                    raw_weight: 8192,
                    other_vertex: mesh.vertices.len() as i32 - 1,
                    position: bits3(
                        parent_inverse.transform_point3(pose.palette[bone].transform_point3(p)),
                    ),
                    normal: bits3(normal),
                });
            }
            mesh.faces.push([base, base + 1, base + 2]);
            mesh.faces.push([base, base + 2, base + 3]);
        }
        mesh.bindings.push(BoneBinding {
            bone_index: bone as i32,
            first_real_vertex: first,
            real_vertex_count: 24,
            first_blend_vertex: 0,
            blend_vertex_count: 0,
        });
        mesh.bindings.push(BoneBinding {
            bone_index: parent as i32,
            first_real_vertex: 0,
            real_vertex_count: 0,
            first_blend_vertex: blend_first,
            blend_vertex_count: 24,
        });
    }
    mesh.repeated_real_vertex_count = mesh.vertices.len() as i32;
    PreparedMesh::prepare(rig, &mesh, AssetKey([62; 32]), AvatarLimits::default()).unwrap()
}
