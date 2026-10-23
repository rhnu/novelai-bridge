use super::{response_error, response_message};
use crate::{BridgeError, ImageMediaFormat, TransportHeader};

#[derive(Copy, Clone)]
pub(super) enum ResponseKind {
    Detect,
    Binary,
    Zip,
    Json,
    Image(ImageMediaFormat),
}

pub(super) fn response_kind(headers: &[TransportHeader]) -> Result<ResponseKind, BridgeError> {
    let mut content_types = headers
        .iter()
        .filter(|header| header.name.eq_ignore_ascii_case("content-type"));
    let Some(header) = content_types.next() else {
        return Ok(ResponseKind::Detect);
    };
    if content_types.next().is_some() {
        return Err(response_message(
            "image response contains multiple Content-Type headers",
        ));
    }
    let value = std::str::from_utf8(&header.value).map_err(response_error)?;
    let media_type = value.split(';').next().unwrap_or_default().trim();
    match media_type.to_ascii_lowercase().as_str() {
        "application/octet-stream" | "binary/octet-stream" => Ok(ResponseKind::Binary),
        "application/zip" => Ok(ResponseKind::Zip),
        "application/json" => Ok(ResponseKind::Json),
        "image/png" => Ok(ResponseKind::Image(ImageMediaFormat::Png)),
        "image/jpeg" => Ok(ResponseKind::Image(ImageMediaFormat::Jpeg)),
        "image/webp" => Ok(ResponseKind::Image(ImageMediaFormat::WebP)),
        _ => Err(response_message(
            "image response Content-Type is unsupported",
        )),
    }
}
