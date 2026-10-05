use wonderland_audio_content::position::*;
#[test]
fn original_iso_squared_pan_and_floor_attenuation() {
    let camera = CameraAudio::Iso {
        screen_offset: [50., 0.],
        world_pixel_width: 100.,
        precise_zoom: 1.,
        zoom: 3,
    };
    assert_eq!(
        source_gain_pan(camera, 2, 1, false, false).unwrap(),
        (0.125, 0.25)
    );
    assert_eq!(source_gain_pan(camera, 1, 1, true, true).unwrap(), (1., 0.));
}
#[test]
fn original_three_d_camera_side_and_zoom_gain() {
    let camera = CameraAudio::ThreeD {
        visual_position: [10., 0., 0.],
        target: [0., 0., 0.],
        position: [0., 0., 30.],
        zoom_3d: 0.,
        precise_zoom: 1.,
    };
    let (gain, pan) = source_gain_pan(camera, 1, 1, false, false).unwrap();
    assert!((gain - 0.75).abs() < 0.00001);
    assert!((pan - (30f32 / 1800f32.sqrt()).powf(2.25)).abs() < 0.00001);
}
#[test]
fn invalid_camera_is_rejected_instead_of_nan_mixer_gain() {
    let camera = CameraAudio::Iso {
        screen_offset: [0., 0.],
        world_pixel_width: 0.,
        precise_zoom: 1.,
        zoom: 3,
    };
    assert!(source_gain_pan(camera, 1, 1, false, false).is_err());
}
