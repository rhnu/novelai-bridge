use std::borrow::Cow;

use image::{DynamicImage, imageops};

use crate::{
    BridgeError, ImageSize, RequestField,
    media_policy::{ImageDecodePolicy, ImageLimit},
};

// The published NovelAI V5 image client chooses from these canvases in this order.
const BUCKETS: [(u32, u32); 97] = [
    (256, 4096),
    (256, 4032),
    (256, 3968),
    (256, 3904),
    (256, 3840),
    (256, 3776),
    (256, 3712),
    (256, 3648),
    (256, 3584),
    (256, 3520),
    (256, 3456),
    (256, 3392),
    (256, 3328),
    (320, 3264),
    (320, 3200),
    (320, 3136),
    (320, 3072),
    (320, 3008),
    (320, 2944),
    (320, 2880),
    (320, 2816),
    (320, 2752),
    (384, 2688),
    (384, 2624),
    (384, 2560),
    (384, 2496),
    (384, 2432),
    (384, 2368),
    (448, 2304),
    (448, 2240),
    (448, 2176),
    (448, 2112),
    (512, 2048),
    (512, 1984),
    (512, 1920),
    (512, 1856),
    (576, 1792),
    (576, 1728),
    (576, 1664),
    (640, 1600),
    (640, 1536),
    (704, 1472),
    (704, 1408),
    (768, 1344),
    (768, 1280),
    (832, 1216),
    (896, 1152),
    (960, 1088),
    (1024, 1024),
    (1088, 960),
    (1152, 896),
    (1216, 832),
    (1280, 768),
    (1344, 768),
    (1408, 704),
    (1472, 704),
    (1536, 640),
    (1600, 640),
    (1664, 576),
    (1728, 576),
    (1792, 576),
    (1856, 512),
    (1920, 512),
    (1984, 512),
    (2048, 512),
    (2112, 448),
    (2176, 448),
    (2240, 448),
    (2304, 448),
    (2368, 384),
    (2432, 384),
    (2496, 384),
    (2560, 384),
    (2624, 384),
    (2688, 384),
    (2752, 320),
    (2816, 320),
    (2880, 320),
    (2944, 320),
    (3008, 320),
    (3072, 320),
    (3136, 320),
    (3200, 320),
    (3264, 320),
    (3328, 256),
    (3392, 256),
    (3456, 256),
    (3520, 256),
    (3584, 256),
    (3648, 256),
    (3712, 256),
    (3776, 256),
    (3840, 256),
    (3904, 256),
    (3968, 256),
    (4032, 256),
    (4096, 256),
];

pub(super) fn target(source: ImageSize) -> ImageSize {
    if source.width == 0 || source.height == 0 {
        return source;
    }
    let ratio = log_aspect_ratio(source.width, source.height);
    BUCKETS
        .into_iter()
        .min_by(|&(left_width, left_height), &(right_width, right_height)| {
            let left = (ratio - log_aspect_ratio(left_width, left_height)).abs();
            let right = (ratio - log_aspect_ratio(right_width, right_height)).abs();
            left.total_cmp(&right)
        })
        .map_or(source, |(width, height)| ImageSize { width, height })
}

fn log_aspect_ratio(width: u32, height: u32) -> f64 { (f64::from(width) / f64::from(height)).ln() }

pub(super) fn crop_to_fill(
    field: RequestField,
    decoded: &DynamicImage,
    target: ImageSize,
    policy: ImageDecodePolicy,
) -> Result<DynamicImage, BridgeError> {
    let (width, height) = (decoded.width(), decoded.height());
    let (scaled_width, scaled_height) = if u64::from(target.width) * u64::from(height)
        >= u64::from(target.height) * u64::from(width)
    {
        (target.width, scale(height, target.width, width))
    } else {
        (scale(width, target.height, height), target.height)
    };

    let resize_needed = (width, height) != (scaled_width, scaled_height);
    let mut buffers = vec![u64::from(width) * u64::from(height) * 4];
    if resize_needed {
        // The resizer creates an RGBA32F vertical-pass buffer before the RGBA8 result.
        buffers.extend([
            u64::from(width) * u64::from(scaled_height) * 16,
            u64::from(scaled_width) * u64::from(scaled_height) * 4,
        ]);
    }
    for decoded_bytes in buffers {
        policy
            .check(ImageLimit::DecodedBytes, decoded_bytes)
            .map_err(|source| super::preparation::invalid_policy(field, source))?;
    }
    let source = decoded
        .as_rgba8()
        .map_or_else(|| Cow::Owned(decoded.to_rgba8()), Cow::Borrowed);
    let resized = if resize_needed {
        Cow::Owned(imageops::resize(
            &*source,
            scaled_width,
            scaled_height,
            imageops::FilterType::CatmullRom,
        ))
    } else {
        source
    };
    if resized.dimensions() == (target.width, target.height) {
        return Ok(DynamicImage::ImageRgba8(resized.into_owned()));
    }
    let cropped = imageops::crop_imm(
        &*resized,
        (scaled_width - target.width) / 2,
        (scaled_height - target.height) / 2,
        target.width,
        target.height,
    )
    .to_image();
    Ok(DynamicImage::ImageRgba8(cropped))
}

fn scale(source: u32, target_side: u32, source_side: u32) -> u32 {
    let numerator = u64::from(source) * u64::from(target_side);
    let denominator = u64::from(source_side);
    u32::try_from((numerator + denominator / 2) / denominator)
        .expect("the media dimensions were bounded before calculating the cover size")
}
