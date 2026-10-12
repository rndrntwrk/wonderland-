//! Original TextureUtils.UploadWithMips/UploadWithAvgMips byte arithmetic.
//! These are explicit source mip pixels, not implementation-defined GPU averages.
use super::*;

pub(super) fn chain(
    image: &RgbaImage,
    policy: SourceMipPolicy,
) -> Result<Vec<RgbaImage>, WorldError> {
    let mut levels: Vec<RgbaImage> = Vec::new();
    let (mut w, mut h) = (image.width, image.height);
    while w / 2 != 0 || h / 2 != 0 {
        let old = levels.last().unwrap_or(image);
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut pixels = Vec::with_capacity(nw as usize * nh as usize);
        for y in 0..nh {
            let yb = y * 2 == h || h / 2 == 0;
            for x in 0..nw {
                // Decimate's historical height comparison is intentional; AvgDecimate uses width.
                let boundary = match policy {
                    SourceMipPolicy::SpriteMaxAlpha => h,
                    SourceMipPolicy::CustomAverageAlpha => w,
                };
                let xb = x * 2 == boundary || w / 2 == 0;
                let i = (y * 2 * w + x * 2) as usize;
                let get = |index: usize| {
                    old.pixels.get(index).copied().ok_or_else(|| {
                        WorldError("source mip index is outside the normalized image".into())
                    })
                };
                let values = [
                    get(i)?,
                    if xb { [0; 4] } else { get(i + 1)? },
                    if yb { [0; 4] } else { get(i + w as usize)? },
                    if xb || yb {
                        [0; 4]
                    } else {
                        get(i + 1 + w as usize)?
                    },
                ];
                let (mut sums, mut count, mut alpha_max) = ([0u32; 4], 0u32, 0u8);
                for pixel in values {
                    alpha_max = alpha_max.max(pixel[3]);
                    if pixel[3] != 0 {
                        count += 1;
                        for c in 0..4 {
                            sums[c] += u32::from(pixel[c]);
                        }
                    }
                }
                let divisor = count.max(1);
                pixels.push([
                    (sums[0] / divisor) as u8,
                    (sums[1] / divisor) as u8,
                    (sums[2] / divisor) as u8,
                    match policy {
                        SourceMipPolicy::SpriteMaxAlpha => alpha_max,
                        SourceMipPolicy::CustomAverageAlpha => (sums[3] / 4) as u8,
                    },
                ]);
            }
        }
        levels.push(RgbaImage {
            width: nw,
            height: nh,
            pixels,
        });
        // Preserve the original upload loop's raw halving, including a zero short axis.
        w /= 2;
        h /= 2;
    }
    Ok(levels)
}
