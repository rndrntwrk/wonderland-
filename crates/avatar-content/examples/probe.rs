//! Local verification helper; no original game content is embedded or committed.
//! cargo run -p wonderland-avatar-content --example probe -- CONTENT_DIRECTORY
//! Optional next arguments are exact head/body collection names from inventory.
use std::{fs, path::Path};
use wonderland_avatar_content::*;
fn visit(path: &Path, all: &mut Vec<(String, Vec<u8>)>) -> std::io::Result<()> {
    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        if path.is_dir() {
            visit(&path, all)?;
        } else {
            let text = path.to_string_lossy();
            // Original installer static avatar families. Animation archives are
            // unnecessary for bind-pose validation and are deliberately omitted.
            let family = ["/bodies/", "/heads/", "/hands/"]
                .iter()
                .any(|f| text.contains(f));
            if (family && path.extension().is_some_and(|e| e == "dat"))
                || path.file_name().is_some_and(|n| n == "adult.skel")
            {
                all.push((
                    path.file_name().unwrap().to_string_lossy().into_owned(),
                    fs::read(&path)?,
                ));
            }
        }
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let Some(directory) = args.first() else {
        return Err("content directory required".into());
    };
    let mut all = Vec::new();
    visit(Path::new(directory), &mut all)?;
    let files: Vec<_> = all
        .iter()
        .map(|(name, bytes)| NamedBytes {
            name,
            bytes,
            key: None,
        })
        .collect();
    let limits = ImportLimits::default();
    let descriptors = inventory(&files, &limits)?;
    println!(
        "{} containers/files; {} original resources",
        files.len(),
        descriptors.len()
    );
    for r in descriptors
        .iter()
        .filter(|r| matches!(r.kind, ResourceKind::Collection | ResourceKind::Skeleton))
    {
        println!("{:?}: {} {:?}", r.kind, r.name, r.key);
    }
    let collections = args
        .iter()
        .skip(1)
        .take(2)
        .enumerate()
        .map(|(i, name)| CollectionSpec {
            name: name.clone(),
            role: if i == 0 {
                CollectionRole::Head
            } else {
                CollectionRole::Body
            },
        })
        .collect();
    let content = import(
        ImportRequest {
            files,
            skeleton_name: "adult.skel",
            collections,
        },
        &limits,
    )?;
    println!(
        "rig: {}; choices: {}; encoded textures: {}; issues: {}",
        content.rig.is_some(),
        content.choices.len(),
        content.textures.len(),
        content.issues.len()
    );
    for issue in content.issues.iter().take(20) {
        println!("{issue}");
    }
    println!(
        "ready skin choices: {} / {}",
        content
            .choices
            .iter()
            .flat_map(|c| &c.skins)
            .filter(|s| s.ready)
            .count(),
        content.choices.len() * 3
    );
    for choice in content.choices.iter().take(3) {
        println!("{} {:?} {:?}", choice.key, choice.outfit, choice.skins);
    }
    for (i, skin) in [Skin::Light, Skin::Medium, Skin::Dark]
        .into_iter()
        .enumerate()
    {
        let find = |role| {
            content
                .choices
                .iter()
                .find(|c| c.role == role && c.skins[i].ready)
                .and_then(|c| c.outfit)
        };
        let selection = AppearanceSelection {
            head: find(CollectionRole::Head),
            body: find(CollectionRole::Body),
            skin,
            ..Default::default()
        };
        if selection.head.is_some() && selection.body.is_some() {
            let parts = content
                .compose(&selection)
                .map_err(|issues| format!("actual source composition: {issues:?}"))?;
            println!("composed {:?}: head {:?}, body {:?}, {} actual mesh parts, {} vertices, {} indices",skin,selection.head,selection.body,parts.len(),parts.iter().map(|p|p.mesh.vertices.len()).sum::<usize>(),parts.iter().map(|p|p.mesh.indices.len()).sum::<usize>());
            for part in parts {
                println!(
                    "  {:?}: {} vertices, {} indices, texture {:?}",
                    part.role,
                    part.mesh.vertices.len(),
                    part.mesh.indices.len(),
                    part.texture
                );
            }
        }
    }
    Ok(())
}
