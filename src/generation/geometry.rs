use serde::Serialize;

use super::{GenerateImageRequest, GenerationInput};
use crate::{BridgeError, GenerationField, ImageDecodePolicy, ImageSize, InvalidRequest};

const MAX_ENHANCE_PIXELS: u64 = 3_145_728;

/// Dimensions selected once for request preparation, expected results and local quotes.
///
/// Max Enhance uses different frontend algorithms for expected result pixels and quote pixels.
/// The result dimensions are a prediction; returned media still undergoes independent validation.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct GenerationGeometry {
    /// Input dimensions sent on the wire and used when preparing a source image.
    pub input_size: ImageSize,
    /// Expected provider output dimensions before the response is received.
    pub output_size: ImageSize,
    /// Dimensions used by the frontend's quote and sample-limit rules.
    pub pricing_size: ImageSize,
}

pub(super) fn resolve(request: &GenerateImageRequest) -> Result<GenerationGeometry, BridgeError> {
    let input_size = request.core.size;
    let max_enhance =
        matches!(&request.operation, GenerationInput::ImageToImage(input) if input.max_enhance);
    let geometry = if max_enhance {
        max_geometry(input_size).ok_or(InvalidRequest::InvalidEnhanceDimensions {
            width: input_size.width,
            height: input_size.height,
        })?
    } else {
        GenerationGeometry {
            input_size,
            output_size: input_size,
            pricing_size: input_size,
        }
    };
    ImageDecodePolicy::default()
        .check_dimensions(geometry.output_size.width, geometry.output_size.height)
        .map_err(|source| InvalidRequest::UndecodableImage {
            field: GenerationField::Size.into(),
            source: Box::new(source),
        })?;
    Ok(geometry)
}

#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Provider geometry uses floating point scaling followed by explicit positive dimension flooring"
)]
fn max_geometry(input_size: ImageSize) -> Option<GenerationGeometry> {
    if input_size.width == 0 || input_size.height == 0 {
        return None;
    }
    let scale = (MAX_ENHANCE_PIXELS as f64 / input_size.pixels() as f64)
        .sqrt()
        .min(2.0);
    let output_size = ImageSize {
        width: (f64::from(input_size.width) * scale).floor() as u32,
        height: (f64::from(input_size.height) * scale).floor() as u32,
    };
    let base_width = (input_size.width / 16).checked_mul(32)?;
    let base_height = (input_size.height / 16).checked_mul(32)?;
    let base_pixels = u64::from(base_width) * u64::from(base_height);
    if base_pixels == 0 || output_size.width == 0 || output_size.height == 0 {
        return None;
    }
    let quote_scale = (MAX_ENHANCE_PIXELS as f64 / base_pixels as f64)
        .sqrt()
        .min(1.0);
    let scaled_width = f64::from(base_width) * quote_scale / 32.0;
    let scaled_height = f64::from(base_height) * quote_scale / 32.0;
    let mut pricing_size = ImageSize {
        width: (scaled_width.round() as u32).checked_mul(32)?,
        height: (scaled_height.round() as u32).checked_mul(32)?,
    };
    if pricing_size.pixels() > MAX_ENHANCE_PIXELS {
        pricing_size = ImageSize {
            width: (scaled_width.floor() as u32).checked_mul(32)?,
            height: (scaled_height.floor() as u32).checked_mul(32)?,
        };
    }
    if pricing_size.width < 32 || pricing_size.height < 32 {
        return None;
    }
    Some(GenerationGeometry {
        input_size,
        output_size,
        pricing_size,
    })
}

#[cfg(test)]
mod tests;
