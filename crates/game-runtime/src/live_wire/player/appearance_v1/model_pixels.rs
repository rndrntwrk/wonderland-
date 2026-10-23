//! WLB1 uses fixed-width little-endian bincode. Its original Vec<[u8; 4]>
//! image layout is pixel count followed by four raw channels per pixel, exactly
//! the layout of Vec<u32> under that codec. Decode each pixel as one bounded
//! fixed-width value rather than charging five nested values plus tuple work.
//! Serialization is unchanged; this adapter is not used by general image formats.
use crate::world_view::*;
use serde::{Deserialize, Deserializer};
use wonderland_render_core::{Aabb, AssetKey, RgbaImage, Vec2};

#[derive(Deserialize)]
struct ModelV1 {
    effective_source: AssetKey,
    effective_content: AssetKey,
    context: ModelContext,
    format_version: u32,
    reconstruction_version: u32,
    groups: Vec<Vec<ModelPart>>,
    textures: Vec<TextureV1>,
    bounds: Aabb,
    depth_mask: Option<ModelDepthMask>,
}
#[derive(Deserialize)]
struct TextureV1 {
    selector: ModelTextureSelector,
    effective_asset: AssetKey,
    uv_scale: Vec2,
    #[serde(deserialize_with = "image")]
    image: RgbaImage,
}
fn image<'de, D: Deserializer<'de>>(d: D) -> Result<RgbaImage, D::Error> {
    if d.is_human_readable() {
        return RgbaImage::deserialize(d);
    }
    #[derive(Deserialize)]
    struct ImageV1 {
        width: u32,
        height: u32,
        pixels: Vec<u32>,
    }
    let image = ImageV1::deserialize(d)?;
    // The guard charges >=80 bytes per u32 before accepting each pixel; both
    // the four-byte input and four-byte output buffers fit that existing credit.
    Ok(RgbaImage {
        width: image.width,
        height: image.height,
        pixels: image.pixels.into_iter().map(u32::to_le_bytes).collect(),
    })
}
pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<WorldModel>, D::Error> {
    Ok(Vec::<ModelV1>::deserialize(d)?
        .into_iter()
        .map(|model| WorldModel {
            effective_source: model.effective_source,
            effective_content: model.effective_content,
            context: model.context,
            format_version: model.format_version,
            reconstruction_version: model.reconstruction_version,
            groups: model.groups,
            textures: model
                .textures
                .into_iter()
                .map(|texture| ModelTexture {
                    selector: texture.selector,
                    effective_asset: texture.effective_asset,
                    uv_scale: texture.uv_scale,
                    image: texture.image,
                })
                .collect(),
            bounds: model.bounds,
            depth_mask: model.depth_mask,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_wire::{codec, guard};
    use bincode::Options;
    #[derive(Debug, Deserialize)]
    struct Image(#[serde(deserialize_with = "image")] RgbaImage);
    fn decode(raw: &[u8], items: usize, credit: usize) -> Result<Image, bincode::Error> {
        codec(raw.len()).deserialize_seed(
            guard::ValueSeed::new(&guard::Budget::new(items, credit, 32)),
            raw,
        )
    }
    #[test]
    fn old_pixel_count_and_channel_bytes_are_unchanged() {
        let image = RgbaImage {
            width: 2,
            height: 1,
            pixels: vec![[1, 35, 69, 103], [137, 171, 205, 239]],
        };
        let raw = codec(1024).serialize(&image).unwrap();
        assert_eq!(&raw[8..16], &2u64.to_le_bytes());
        assert_eq!(&raw[16..], &[1, 35, 69, 103, 137, 171, 205, 239]);
        let value = decode(&raw, 100, 8192).unwrap().0;
        assert_eq!(value, image);
        assert_eq!(codec(1024).serialize(&value).unwrap(), raw);
        let json = serde_json::to_string(&image).unwrap();
        let json_value: Image = serde_json::from_str(&json).unwrap();
        assert_eq!(json_value.0, image);
        assert_eq!(serde_json::to_string(&json_value.0).unwrap(), json);
    }
    #[test]
    fn mixed_model_records_preserve_old_binary_and_json_fields() {
        use wonderland_render_core::Vec3;
        #[derive(Deserialize)]
        struct Models(#[serde(deserialize_with = "super::deserialize")] Vec<WorldModel>);
        let models: Vec<_> = [
            ModelContext::Dgrp {
                effective_iff: AssetKey([9; 32]),
                chunk_id: 101,
            },
            ModelContext::Standalone,
            ModelContext::Vitaboy,
        ]
        .into_iter()
        .map(|context| WorldModel {
            effective_source: AssetKey([1; 32]),
            effective_content: AssetKey([2; 32]),
            context,
            format_version: 2,
            reconstruction_version: 0,
            groups: vec![],
            textures: [
                ModelTextureSelector::Custom { id: 7 },
                ModelTextureSelector::Sprite {
                    rotation: 2,
                    ordinal: 3,
                },
            ]
            .into_iter()
            .map(|selector| ModelTexture {
                selector,
                effective_asset: AssetKey([3; 32]),
                uv_scale: Vec2::new(1., 1.),
                image: RgbaImage {
                    width: 2,
                    height: 1,
                    pixels: vec![[1, 35, 69, 103], [137, 171, 205, 239]],
                },
            })
            .collect(),
            bounds: Aabb::new(Vec3::ZERO, Vec3::new(1., 2., 3.)).unwrap(),
            depth_mask: None,
        })
        .collect();
        let raw = codec(65536).serialize(&models).unwrap();
        let budget = guard::Budget::new(10000, 1024 * 1024, 32);
        let decoded: Models = codec(raw.len())
            .deserialize_seed(guard::ValueSeed::new(&budget), &raw)
            .unwrap();
        assert_eq!(decoded.0, models);
        assert_eq!(codec(65536).serialize(&decoded.0).unwrap(), raw);
        let json = serde_json::to_string(&models).unwrap();
        let from_json: Models = serde_json::from_str(&json).unwrap();
        assert_eq!(from_json.0, models);
        assert_eq!(serde_json::to_string(&from_json.0).unwrap(), json);
    }
    #[test]
    fn pixels_remain_count_credit_and_input_bounded() {
        let image = RgbaImage {
            width: 128,
            height: 256,
            pixels: vec![[1, 2, 3, 4]; 32768],
        };
        let raw = codec(1024 * 1024).serialize(&image).unwrap();
        assert_eq!(decode(&raw, 70000, 4 * 1024 * 1024).unwrap().0, image);
        assert!(
            decode(&raw, 100, 4 * 1024 * 1024)
                .unwrap_err()
                .to_string()
                .contains("collection hint")
        );
        assert!(
            decode(&raw, 70000, 1024)
                .unwrap_err()
                .to_string()
                .contains("allocation credit")
        );
        let mut huge = raw[..16].to_vec();
        huge[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(decode(&huge, 70000, 4 * 1024 * 1024).is_err());
        for missing in 1..=4 {
            assert!(decode(&raw[..raw.len() - missing], 70000, 4 * 1024 * 1024).is_err());
        }
    }
}
