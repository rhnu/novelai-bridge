use image::{
    DynamicImage, Rgba, RgbaImage,
    imageops::{self, FilterType},
};

use crate::{BridgeError, ImageSize, InvalidRequest, RequestField};
const REFERENCE_TARGETS: [(u32, u32); 3] = [(1024, 1536), (1536, 1024), (1472, 1472)];

pub(super) fn fit_decoded(
    field: RequestField,
    decoded: &DynamicImage,
    size: ImageSize,
    filter: FilterType,
) -> Result<Vec<u8>, BridgeError> {
    let policy = crate::media_policy::ImageDecodePolicy::default();
    policy
        .check(
            crate::media_policy::ImageLimit::DecodedBytes,
            u64::from(size.width) * u64::from(size.height) * 4,
        )
        .map_err(|source| InvalidRequest::UndecodableImage {
            field,
            source: Box::new(source),
        })?;
    policy
        .check(
            crate::media_policy::ImageLimit::DecodedBytes,
            u64::from(decoded.width()) * u64::from(decoded.height()) * 4,
        )
        .map_err(|source| InvalidRequest::UndecodableImage {
            field,
            source: Box::new(source),
        })?;
    let source = decoded.to_rgba8();
    let (scaled_width, scaled_height) =
        fit_within(source.width(), source.height(), size.width, size.height);
    let resized = imageops::resize(&source, scaled_width, scaled_height, filter);
    let mut canvas = RgbaImage::from_pixel(size.width, size.height, Rgba([0, 0, 0, 255]));
    imageops::overlay(
        &mut canvas,
        &resized,
        i64::from((size.width - scaled_width) / 2),
        i64::from((size.height - scaled_height) / 2),
    );
    super::codec::encode_png(&DynamicImage::ImageRgba8(canvas), field, policy)
}

fn fit_within(sw: u32, sh: u32, tw: u32, th: u32) -> (u32, u32) {
    if u64::from(tw) * u64::from(sh) <= u64::from(th) * u64::from(sw) {
        (
            tw,
            u32::try_from(
                (u64::from(sh) * u64::from(tw) / u64::from(sw))
                    .max(1)
                    .min(u64::from(th)),
            )
            .unwrap_or(th),
        )
    } else {
        (
            u32::try_from(
                (u64::from(sw) * u64::from(th) / u64::from(sh))
                    .max(1)
                    .min(u64::from(tw)),
            )
            .unwrap_or(tw),
            th,
        )
    }
}

pub(super) fn pick_reference_size(source: ImageSize) -> ImageSize {
    REFERENCE_TARGETS
        .into_iter()
        .min_by_key(|&(w, h)| {
            let (sw, sh) = fit_within(source.width, source.height, w, h);
            u64::from(w) * u64::from(h) - u64::from(sw) * u64::from(sh)
        })
        .map_or(source, |(width, height)| ImageSize { width, height })
}
