//! Source-file replacement must not conceal differing payloads behind one identity.
use std::sync::Arc;
use wonderland_world_view::*;
#[path = "../examples/support/masked.rs"]
mod mask_fixture;
#[test]
fn normal_and_portal_documents_have_distinct_payload_and_import_identities() {
    let normal = mask_fixture::masked(ModelMaskKind::Normal);
    let portal = mask_fixture::masked(ModelMaskKind::Portal);
    assert_ne!(normal.models, portal.models);
    assert_ne!(normal.revision.content, portal.revision.content);
    assert_ne!(normal.provenance.origin, portal.provenance.origin);
    assert_eq!(normal.models[0].effective_content, normal.revision.content);
    assert_eq!(portal.models[0].effective_content, portal.revision.content);
    let mut renderer = WorldRenderer::new(Arc::new(normal)).unwrap();
    renderer
        .prepare_gpu(ViewportControls::default(), 256, 192)
        .unwrap();
    renderer.replace_document(Arc::new(portal)).unwrap();
    renderer
        .prepare_gpu(ViewportControls::default(), 256, 192)
        .unwrap();
}

#[test]
fn fixture_content_binding_is_repeatable_and_changes_with_payload_bytes() {
    let mut document = mask_fixture::masked(ModelMaskKind::Normal);
    let first = document.revision.content;
    mask_fixture::bind_content(&mut document);
    assert_eq!(document.revision.content, first);
    document.models[0].textures[0].image.pixels[0][0] -= 1;
    mask_fixture::bind_content(&mut document);
    assert_ne!(document.revision.content, first);
}
