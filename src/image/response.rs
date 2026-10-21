use super::{GeneratedImage, ImageMediaFormat, output::GeneratedImageSource};
use crate::{
    BridgeError, DecodeError, DecodeTarget, GeneratedImageMetadata,
    media_policy::{ImageDecodePolicy, ImageLimit},
};

mod archive;
mod headers;
mod json;

/// Decodes raw, ZIP or JSON images without HTTP headers.
///
/// Metadata inspection is best-effort. Any malformed image rejects the complete response.
/// Use [`decode_generated_image_response`] when response headers are available.
///
/// # Errors
/// Returns response errors for invalid containers, images or exceeded media budgets.
pub fn decode_generated_images(payload: Vec<u8>) -> Result<Vec<GeneratedImage>, BridgeError> {
    decode_generated_images_with_policy(payload, ImageDecodePolicy::default())
}

/// Decodes raw, ZIP or JSON images within explicit media budgets.
///
/// # Errors
/// Returns response errors for invalid containers, images or exceeded media budgets.
pub fn decode_generated_images_with_policy(
    payload: Vec<u8>,
    policy: ImageDecodePolicy,
) -> Result<Vec<GeneratedImage>, BridgeError> {
    decode_payload(payload, policy, headers::ResponseKind::Detect)
}

/// Decodes an HTTP image response, requiring its media type to agree with its content.
///
/// Missing media types are detected from the body. An octet-stream is limited to binary raw
/// images or ZIP archives. Unknown, duplicate and conflicting media types are rejected.
///
/// # Errors
/// Returns response errors for inconsistent headers, invalid content or exceeded budgets.
pub fn decode_generated_image_response(
    response: crate::BufferedResponse,
) -> Result<Vec<GeneratedImage>, BridgeError> {
    decode_generated_image_response_with_policy(response, ImageDecodePolicy::default())
}

/// Decodes an HTTP image response within explicit media budgets.
///
/// # Errors
/// Returns response errors for inconsistent headers, invalid content or exceeded budgets.
pub fn decode_generated_image_response_with_policy(
    response: crate::BufferedResponse,
    policy: ImageDecodePolicy,
) -> Result<Vec<GeneratedImage>, BridgeError> {
    let kind = headers::response_kind(&response.headers)?;
    decode_payload(response.body, policy, kind)
}

fn decode_payload(
    payload: Vec<u8>,
    policy: ImageDecodePolicy,
    kind: headers::ResponseKind,
) -> Result<Vec<GeneratedImage>, BridgeError> {
    use headers::ResponseKind;
    match kind {
        ResponseKind::Zip => archive::decode(payload, policy),
        ResponseKind::Json => json::decode(&payload, policy),
        ResponseKind::Detect if is_json(&payload) => json::decode(&payload, policy),
        ResponseKind::Detect | ResponseKind::Binary if payload.starts_with(b"PK") => {
            archive::decode(payload, policy)
        },
        ResponseKind::Detect | ResponseKind::Binary | ResponseKind::Image(_) => {
            let mut budget = ResponseBudget::new(policy);
            budget.begin_image()?;
            budget.add_bytes(payload.len() as u64)?;
            let image = validated_generated_image(payload, GeneratedImageSource::Raw, policy)?;
            if let ResponseKind::Image(expected) = kind
                && image.format() != expected
            {
                return Err(response_message(
                    "image Content-Type does not match its content",
                ));
            }
            Ok(vec![image])
        },
    }
}

fn is_json(payload: &[u8]) -> bool {
    payload.iter().find(|byte| !byte.is_ascii_whitespace()) == Some(&b'{')
}

struct ResponseBudget {
    policy: ImageDecodePolicy,
    images: u64,
    bytes: u64,
}

impl ResponseBudget {
    const fn new(policy: ImageDecodePolicy) -> Self {
        Self {
            policy,
            images: 0,
            bytes: 0,
        }
    }

    fn begin_image(&mut self) -> Result<(), BridgeError> {
        let images = self.images.saturating_add(1);
        self.policy
            .check(ImageLimit::Images, images)
            .map_err(response_error)?;
        self.images = images;
        Ok(())
    }

    fn check_bytes(&self, bytes: u64) -> Result<(), BridgeError> {
        self.policy
            .check(ImageLimit::EncodedBytes, bytes)
            .map_err(response_error)?;
        self.policy
            .check(
                ImageLimit::TotalImageBytes,
                self.bytes.saturating_add(bytes),
            )
            .map_err(response_error)
    }

    fn add_bytes(&mut self, bytes: u64) -> Result<(), BridgeError> {
        self.check_bytes(bytes)?;
        self.bytes = self.bytes.saturating_add(bytes);
        Ok(())
    }
}

pub fn validated_generated_image(
    bytes: Vec<u8>,
    source: GeneratedImageSource,
    policy: ImageDecodePolicy,
) -> Result<GeneratedImage, BridgeError> {
    policy
        .check(ImageLimit::EncodedBytes, bytes.len() as u64)
        .map_err(response_error)?;
    let format =
        ImageMediaFormat::from_image_format(image::guess_format(&bytes).map_err(response_error)?)
            .ok_or_else(|| response_message("response image format is unsupported"))?;

    if let GeneratedImageSource::Archive { name, .. } = &source
        && let Some(extension) = ImageMediaFormat::from_extension(name)
        && extension != format
    {
        return Err(response_message(
            "image archive extension does not match content",
        ));
    }
    let metadata = if format == ImageMediaFormat::Png {
        let report = crate::metadata::png::extract_png_text_chunks(&bytes, policy, false)
            .map_err(response_error)?;
        let parsed = crate::metadata::interpret_png_text(report);
        if parsed.is_empty() {
            GeneratedImageMetadata::NotPresent
        } else {
            GeneratedImageMetadata::Parsed(Box::new(parsed))
        }
    } else {
        super::codec::decode(&bytes, policy)
            .map_err(|source| DecodeError::new(DecodeTarget::ImageResponse, source))?;
        GeneratedImageMetadata::UnsupportedFormat
    };
    Ok(GeneratedImage {
        bytes,
        format,
        metadata,
        source,
    })
}

fn response_error(source: impl std::error::Error + Send + Sync + 'static) -> BridgeError {
    DecodeError::new(DecodeTarget::ImageResponse, Box::new(source)).into()
}

fn response_message(message: &str) -> BridgeError {
    response_error(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        message,
    ))
}

#[cfg(test)]
mod tests;
