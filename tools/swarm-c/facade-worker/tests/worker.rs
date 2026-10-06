use wonderland_facade_worker::*;
use wonderland_render_core::*;

#[test]
fn source_worker_outputs_original_container_and_576_source_thumbnail() {
    let source = source_fixture();
    let bytes = encode_source_request(&source).unwrap();
    let root =
        std::env::temp_dir().join(format!("wonderland-source-worker-{}", std::process::id()));
    let input = root.with_extension("wlcsrc");
    std::fs::write(&input, bytes).unwrap();
    let decoded = read_source_request(&input).unwrap();
    assert_eq!(decoded.world, source.world);
    let summary = render_source_to_directory(decoded, &root).unwrap();
    assert_eq!(summary.image_count, 6);
    let fsof = wonderland_render_core::derivatives::fsof::Fsof::decode(
        &std::fs::read(root.join("facade.fsof")).unwrap(),
        Default::default(),
    )
    .unwrap();
    assert_eq!(
        [fsof.floor_width, fsof.floor_height, fsof.wall_width],
        [384, 256, 512]
    );
    assert!(fsof.night.is_some());
    assert_eq!(fsof.geometry.wall.vertices.len(), 32);
    assert!(std::fs::read_to_string(root.join("metadata.json"))
        .unwrap()
        .contains("fsof_sha256"));
    let png = std::fs::read(root.join("thumbnail-day.png")).unwrap();
    assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 576);
    std::fs::remove_file(input).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn fixture_round_trips_and_renders_all_six_images() {
    let input = synthetic_fixture();
    let bytes = encode_request(&input).unwrap();
    use bincode::Options;
    let decoded: DerivativeInput = bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_little_endian()
        .reject_trailing_bytes()
        .deserialize(&bytes[8..])
        .unwrap();
    assert_eq!(input, decoded);
    let prepared = PreparedDerivative::new(decoded, DerivativeRenderLimits::default()).unwrap();
    let output = prepared.render().unwrap();
    assert_eq!(output.images().len(), 6);
    for image in output.images() {
        assert!(
            image.image.pixels.iter().any(|p| p[3] > 0),
            "{:?}",
            image.role
        );
    }
    assert_eq!(output.images()[0].image.width, 384);
    assert_eq!(output.images()[0].image.height, 256);
    assert_eq!(output.images()[2].image.width, 512);
    assert_eq!(output.images()[2].image.height, 72);
    assert_ne!(output.images()[0].image, output.images()[1].image);
    assert_eq!(output.digest(), prepared.render().unwrap().digest());
}

#[test]
fn png_bytes_are_deterministic_and_preserve_alpha() {
    let image = RgbaImage {
        width: 2,
        height: 1,
        pixels: vec![[1, 2, 3, 0], [250, 251, 252, 128]],
    };
    let mut a = Vec::new();
    let mut b = Vec::new();
    assert_eq!(
        write_png(&mut a, &image).unwrap(),
        write_png(&mut b, &image).unwrap()
    );
    assert_eq!(a, b);
    assert_eq!(&a[..8], b"\x89PNG\r\n\x1a\n");
    assert!(a
        .windows(9)
        .any(|bytes| bytes == [0, 1, 2, 3, 0, 250, 251, 252, 128]));
}

#[test]
fn malformed_png_image_is_rejected_without_writing_header() {
    let mut bytes = Vec::new();
    assert!(write_png(
        &mut bytes,
        &RgbaImage {
            width: 2,
            height: 1,
            pixels: vec![[0; 4]]
        }
    )
    .is_err());
    assert!(bytes.is_empty());
}

#[test]
fn transformed_entity_and_texture_changes_change_the_effective_input_hash() {
    let source = synthetic_fixture();
    let original =
        PreparedDerivative::new(source.clone(), DerivativeRenderLimits::default()).unwrap();
    let mut moved = source.clone();
    moved.frame.entities[0].transform.translation.x += 1.;
    moved.frame.entities[0].visual_revision += 1;
    assert_ne!(
        original.key(),
        PreparedDerivative::new(moved, DerivativeRenderLimits::default())
            .unwrap()
            .key()
    );
    let mut texture = source;
    texture.materials[0].night.texture.as_mut().unwrap().pixels[0][0] += 1;
    assert_ne!(
        original.key(),
        PreparedDerivative::new(texture, DerivativeRenderLimits::default())
            .unwrap()
            .key()
    );
}
