//! A schema-valid small input whose repeated instances exceed expanded work.
use crate::mask_fixture as masked;
use wonderland_world_view::*;

pub fn overbudget() -> WorldDocument {
    let mut document = masked::masked(ModelMaskKind::Normal);
    document.provenance.origin = "tests:expanded-scene-refusal".into();
    let triangle = document.models[0].groups[0][0].mesh.indices[..3].to_vec();
    document.models[0].groups[0][0].mesh.indices = triangle.repeat(70_000);
    let original = document.objects[0].clone();
    document.objects = (0..32)
        .map(|index| {
            let mut object = original.clone();
            object.entity.as_mut().unwrap().object_id = 100 + index;
            object
        })
        .collect();
    masked::bind_content(&mut document);
    document
        .validate()
        .expect("valid normalized input, not malformed JSON");
    document
}
