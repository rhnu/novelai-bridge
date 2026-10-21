use std::io::Cursor;

use image::{DynamicImage, ImageDecoder, ImageReader};

use super::ImageMediaFormat;
use crate::media_policy::{ImageDecodePolicy, ImageLimit, ImageResourceUsage};

pub(super) type CodecError = Box<dyn std::error::Error + Send + Sync>;

pub struct DecodedImage {
    pub pixels: DynamicImage,
    pub usage: ImageResourceUsage,
}

/// Bounds the encoder's output as it is written, before its buffer can grow beyond policy.
pub(super) fn encode_png(
    pixels: &DynamicImage,
    field: crate::RequestField,
    policy: ImageDecodePolicy,
) -> Result<Vec<u8>, crate::BridgeError> {
    let mut writer = BoundedWriter {
        inner: Cursor::new(Vec::new()),
        policy,
        rejection: None,
    };
    if let Err(error) = pixels.write_to(&mut writer, image::ImageFormat::Png) {
        // Some image encoders hide the IO error's concrete type; retain our own policy cause.
        let source: CodecError = writer.rejection.map_or_else(
            || Box::new(error) as CodecError,
            |error| Box::new(error) as CodecError,
        );
        return Err(crate::PreparationError::ImageEncodingFailed { field, source }.into());
    }
    Ok(writer.inner.into_inner())
}

struct BoundedWriter {
    inner: Cursor<Vec<u8>>,
    policy: ImageDecodePolicy,
    rejection: Option<crate::media_policy::ImagePolicyError>,
}

impl std::io::Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.policy
            .check(
                ImageLimit::EncodedBytes,
                self.inner.position().saturating_add(bytes.len() as u64),
            )
            .map_err(|source| {
                self.rejection = Some(source);
                std::io::Error::other(source)
            })?;
        std::io::Write::write(&mut self.inner, bytes)
    }

    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}

impl std::io::Seek for BoundedWriter {
    fn seek(&mut self, position: std::io::SeekFrom) -> std::io::Result<u64> {
        let position = std::io::Seek::seek(&mut self.inner, position)?;
        self.policy
            .check(ImageLimit::EncodedBytes, position)
            .map_err(|source| {
                self.rejection = Some(source);
                std::io::Error::other(source)
            })?;
        Ok(position)
    }
}

/// The common input/prepared-image decoder applies budgets before allocating full pixels.
pub fn decode(bytes: &[u8], policy: ImageDecodePolicy) -> Result<DecodedImage, CodecError> {
    policy.check(ImageLimit::EncodedBytes, bytes.len() as u64)?;
    let raw_format = image::guess_format(bytes)?;
    let format = ImageMediaFormat::from_image_format(raw_format)
        .ok_or_else(|| invalid("unsupported image format"))?;
    if format == ImageMediaFormat::Png {
        let report = crate::metadata::png::extract_png_text_chunks(bytes, policy, true)?;
        let usage = report.usage;
        let raw = report
            .pixels
            .ok_or_else(|| invalid("PNG pixels were not retained"))?;
        return Ok(DecodedImage {
            pixels: png_pixels(raw, usage.width, usage.height)?,
            usage,
        });
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), raw_format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(policy.max_width);
    limits.max_image_height = Some(policy.max_height);
    limits.max_alloc = Some(policy.max_decoded_bytes);
    reader.limits(limits);
    let decoder = reader.into_decoder()?;
    let (width, height) = decoder.dimensions();
    policy.check_dimensions(width, height)?;
    // Lossless RGB WebP decoding also allocates an RGBA pixel workspace. The backend does
    // not enforce max_alloc itself, so account for that actual pixel buffer before decoding.
    let decoded_bytes = if format == ImageMediaFormat::WebP {
        decoder
            .total_bytes()
            .max(u64::from(width) * u64::from(height) * 4)
    } else {
        decoder.total_bytes()
    };
    policy.check(ImageLimit::DecodedBytes, decoded_bytes)?;
    let usage = ImageResourceUsage {
        encoded_bytes: bytes.len() as u64,
        width,
        height,
        decoded_bytes,
        text_chunk_bytes: 0,
        total_text_bytes: 0,
    };
    let pixels = DynamicImage::from_decoder(decoder)?;
    Ok(DecodedImage { pixels, usage })
}

fn png_pixels(
    raw: crate::metadata::png::PngPixels,
    width: u32,
    height: u32,
) -> Result<DynamicImage, CodecError> {
    use png::{BitDepth, ColorType};
    macro_rules! buffer {
        ($variant:ident, $pixel:ty, $data:expr) => {
            image::ImageBuffer::<$pixel, _>::from_raw(width, height, $data)
                .map(DynamicImage::$variant)
                .ok_or_else(|| invalid("PNG pixel buffer size mismatch"))
        };
    }
    if raw.depth == BitDepth::Sixteen {
        let values: Vec<u16> = raw
            .bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        match raw.color {
            ColorType::Grayscale => buffer!(ImageLuma16, image::Luma<u16>, values),
            ColorType::GrayscaleAlpha => buffer!(ImageLumaA16, image::LumaA<u16>, values),
            ColorType::Rgb => buffer!(ImageRgb16, image::Rgb<u16>, values),
            ColorType::Rgba => buffer!(ImageRgba16, image::Rgba<u16>, values),
            ColorType::Indexed => Err(invalid("unexpanded indexed PNG")),
        }
    } else {
        match raw.color {
            ColorType::Grayscale => buffer!(ImageLuma8, image::Luma<u8>, raw.bytes),
            ColorType::GrayscaleAlpha => buffer!(ImageLumaA8, image::LumaA<u8>, raw.bytes),
            ColorType::Rgb => buffer!(ImageRgb8, image::Rgb<u8>, raw.bytes),
            ColorType::Rgba => buffer!(ImageRgba8, image::Rgba<u8>, raw.bytes),
            ColorType::Indexed => Err(invalid("unexpanded indexed PNG")),
        }
    }
}

fn invalid(message: &str) -> CodecError {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message).into()
}
