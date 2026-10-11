#![forbid(unsafe_code)]
use std::{sync::Arc, time::Instant};
use wonderland_avatar_view::*;
use wonderland_render_core::{math::*, AssetKey, EntityRef};
fn crowd(count: u32) -> Result<()> {
    let rig = fixtures::representative_rig();
    let mesh = fixtures::representative_mesh(&rig);
    let bounds = mesh.bounds(&rig.bind_pose(), Mat4::IDENTITY)?;
    let view = Aabb::new(Vec3::new(-3.0, -3.0, -3.0), Vec3::new(20.0, 4.0, 20.0)).unwrap();
    let actors: Vec<_> = (0..count)
        .map(|i| AvatarCandidate {
            reference: EntityRef {
                object_id: i + 1,
                generation: 1,
            },
            bounds: bounds
                .transformed(Mat4::from_translation(Vec3::new(
                    (i % 8) as f32 * 4.0,
                    0.0,
                    (i / 8) as f32 * 4.0,
                )))
                .unwrap(),
            visible: true,
            level: 0,
            requires_endpoint: i % 8 == 0,
        })
        .collect();
    let clip = Arc::new(Clip::new(
        &rig,
        fixtures::animation("synthetic-bob", 2.18, 2.25),
        AssetKey([72; 32]),
        AvatarLimits::default(),
    )?);
    let mut players: Vec<_> = actors
        .iter()
        .map(|a| PosePlayer::new(a.reference, &rig))
        .collect();
    let mut palettes = vec![rig.bind_pose(); actors.len()];
    let mut pose_us = 0;
    let mut skin_us = 0;
    let mut updates = 0;
    let mut draws = 0;
    let mut endpoints = 0;
    for tick in 1..=60 {
        let plan = plan_lod(
            tick,
            &actors,
            Vec3::ZERO,
            view,
            0,
            LodPolicy {
                max_pose_updates: 32,
                ..LodPolicy::default()
            },
        )?;
        for (i, (decision, player)) in plan.iter().zip(&mut players).enumerate() {
            let start = Instant::now();
            player.commit(
                tick,
                &rig,
                Timeline {
                    layers: vec![TimelineLayer {
                        clip: clip.clone(),
                        current_frame: (tick % 2) as f32,
                        speed: 1.0,
                        weight: 1.0,
                        backwards: false,
                        end_reached: false,
                        looping: true,
                    }],
                    carry: None,
                },
            )?;
            if decision.update_pose {
                palettes[i] = player.sample(&rig, 0.5)?;
                updates += 1;
            }
            pose_us += start.elapsed().as_micros();
            if actors[i].requires_endpoint {
                let finger = rig.bone_index("R_FINGER0").unwrap();
                let reference =
                    player.sample(&rig, 0.5)?.palette[finger].transform_point3(Vec3::ZERO);
                let actual = palettes[i].palette[finger].transform_point3(Vec3::ZERO);
                assert!((reference - actual).length() < 0.00001);
                endpoints += 1;
            }
            if decision.draw {
                let start = Instant::now();
                mesh.skin(&palettes[i], Mat4::IDENTITY)?;
                skin_us += start.elapsed().as_micros();
                draws += 1;
            }
        }
    }
    println!("kind=synthetic_cpu_reference actors={count} ticks=60 bones={} vertices={} triangles={} shared_prepared_bytes={} palette_updates={updates} draw_instances={draws} endpoint_checks={endpoints} pose_commit_sample_us={pose_us} cpu_skin_us={skin_us} gpu_pass_time=unmeasured backend=none",rig.source().bones.len(),mesh.vertices.len(),mesh.indices.len()/3,mesh.resident_bytes());
    Ok(())
}
fn fixture() -> CookedAvatar {
    CookedAvatar {
        version: 1,
        source_digest: AssetKey([90; 32]),
        rig_resource: ResourceKey {
            group_id: 0,
            file_id: 1,
            type_id: 5,
        },
        skeleton: fixtures::skeleton(),
        meshes: vec![(
            ResourceKey {
                group_id: 0,
                file_id: 2,
                type_id: 3,
            },
            fixtures::source_mesh(),
        )],
        animations: vec![(
            ResourceKey {
                group_id: 0,
                file_id: 3,
                type_id: 4,
            },
            fixtures::animation("synthetic.anim", 0.0, 8.0),
        )],
    }
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
fn run() -> std::result::Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("fixture") => {
            let path = args.get(2).ok_or("usage: fixture OUTPUT.wcav")?;
            let bytes = fixture()
                .encode(AvatarLimits::default())
                .map_err(|e| format!("{e:?}"))?;
            std::fs::write(path, &bytes).map_err(|e| e.to_string())?;
            println!("synthetic_cooked_bytes={} output={path}", bytes.len());
        }
        Some("validate") => {
            let path = args.get(2).ok_or("usage: validate INPUT.wcav")?;
            let metadata = std::fs::metadata(path).map_err(|e| e.to_string())?;
            if metadata.len() > 16 * 1024 * 1024 + 48 {
                return Err("input exceeds maximum cooked bytes".into());
            }
            let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
            let value = CookedAvatar::decode(&bytes, AvatarLimits::default())
                .map_err(|e| format!("{e:?}"))?;
            println!("cooked_version={} rig_bones={} meshes={} animations={} source_digest={:?} signed_marker_bridge={:?}",value.version,value.skeleton.bones.len(),value.meshes.len(),value.animations.len(),value.source_digest,value.validate_a_time_ids());
        }
        Some("crowd") => {
            crowd(32).map_err(|e| format!("{e:?}"))?;
            crowd(64).map_err(|e| format!("{e:?}"))?;
        }
        _ => {
            return Err(
                "usage: wonderland-c-avatar-cooker {fixture OUTPUT.wcav|validate INPUT.wcav|crowd}"
                    .into(),
            )
        }
    }
    Ok(())
}
