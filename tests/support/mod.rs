#![allow(dead_code)]

use std::{
    io::Cursor,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use base64::Engine;
use futures_util::stream;
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use novelai_bridge::{
    BridgeError, ImageStreamChunk, StreamingTransport, Transport, TransportError, TransportHeader,
    TransportMethod, TransportOperation, TransportRequest, TransportResponse, TransportSseBody,
    TransportSseEvent, TransportSseResponse,
};
use serde_json::Value;

#[derive(Clone)]
pub struct MockTransport {
    state: Arc<Mutex<MockTransportState>>,
}

#[derive(Default)]
struct MockTransportState {
    last_path: Option<String>,
    last_body: Option<Value>,
    post_history: Vec<(String, Value)>,
    byte_paths: Vec<String>,
    bytes_response: Vec<u8>,
    encode_vibe_response: Vec<u8>,
    sse_chunks: Vec<ImageStreamChunk>,
    sse_error: Option<BridgeError>,
    json_response: Value,
    json_error: Option<TransportError>,
    bytes_error: Option<TransportError>,
}

impl Default for MockTransport {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(MockTransportState {
                bytes_response: create_png_bytes(2, 2),
                ..MockTransportState::default()
            })),
        }
    }
}

impl MockTransport {
    pub fn with_bytes_response(bytes_response: Vec<u8>) -> Self {
        let transport = Self::default();
        transport.set_bytes_response(bytes_response);
        transport
    }

    pub fn set_bytes_response(&self, bytes_response: Vec<u8>) {
        self.state.lock().unwrap().bytes_response = bytes_response;
    }

    pub fn set_encode_vibe_response(&self, encode_vibe_response: Vec<u8>) {
        self.state.lock().unwrap().encode_vibe_response = encode_vibe_response;
    }

    pub fn set_sse_chunks(&self, sse_chunks: Vec<ImageStreamChunk>) {
        self.state.lock().unwrap().sse_chunks = sse_chunks;
    }

    pub fn set_sse_error(&self, error: BridgeError) {
        self.state.lock().unwrap().sse_error = Some(error);
    }

    pub fn set_json_response(&self, json_response: Value) {
        self.state.lock().unwrap().json_response = json_response;
    }

    pub fn set_json_error(&self, json_error: TransportError) {
        self.state.lock().unwrap().json_error = Some(json_error);
    }

    /// Makes the next `post_bytes` call fail with `bytes_error`.
    pub fn set_bytes_error(&self, bytes_error: TransportError) {
        self.state.lock().unwrap().bytes_error = Some(bytes_error);
    }

    pub fn last_body(&self) -> Option<Value> {
        self.state.lock().unwrap().last_body.clone()
    }

    pub fn last_path(&self) -> Option<String> {
        self.state.lock().unwrap().last_path.clone()
    }

    pub fn byte_paths(&self) -> Vec<String> {
        self.state.lock().unwrap().byte_paths.clone()
    }

    pub fn bodies_for_path(&self, path: &str) -> Vec<Value> {
        self.state
            .lock()
            .unwrap()
            .post_history
            .iter()
            .filter(|(recorded_path, _)| recorded_path == path)
            .map(|(_, body)| body.clone())
            .collect()
    }
}

#[async_trait]
impl Transport for MockTransport {
    async fn execute(
        &self,
        request: TransportRequest,
    ) -> Result<TransportResponse, TransportError> {
        let body = request
            .body
            .as_ref()
            .map(|body| serde_json::from_slice::<Value>(body))
            .transpose()
            .map_err(|source| {
                TransportError::new(
                    TransportOperation::SendRequest,
                    Some(request.endpoint.clone()),
                    Box::new(source),
                )
            })?;
        let mut state = self.state.lock().unwrap();
        state.last_path = Some(request.endpoint.clone());
        if request.method == TransportMethod::Get {
            if let Some(error) = state.json_error.take() {
                return Err(error);
            }
            return Ok(TransportResponse {
                status: 200,
                headers: Vec::new(),
                body: serde_json::to_vec(&state.json_response).unwrap(),
            });
        }
        if let Some(mut body) = body {
            restore_typed_float_view(&mut body);
            state.last_body = Some(body.clone());
            state.post_history.push((request.endpoint.clone(), body));
        }
        state.byte_paths.push(request.endpoint.clone());
        if let Some(error) = state.bytes_error.take() {
            return Err(error);
        }
        let body = if request.endpoint.ends_with("/ai/encode-vibe") {
            state.encode_vibe_response.clone()
        } else {
            state.bytes_response.clone()
        };
        Ok(TransportResponse {
            status: 200,
            headers: Vec::new(),
            body,
        })
    }
}

#[async_trait]
impl StreamingTransport for MockTransport {
    async fn execute_sse(
        &self,
        request: TransportRequest,
    ) -> Result<TransportSseResponse, TransportError> {
        let body = serde_json::from_slice::<Value>(request.body.as_deref().unwrap_or_default())
            .map_err(|source| {
                TransportError::new(
                    TransportOperation::SendRequest,
                    Some(request.endpoint.clone()),
                    Box::new(source),
                )
            })?;
        let mut body = body;
        restore_typed_float_view(&mut body);
        let mut state = self.state.lock().unwrap();
        state.last_path = Some(request.endpoint.clone());
        state.last_body = Some(body.clone());
        state.post_history.push((request.endpoint, body));
        let mut items: Vec<Result<TransportSseEvent, TransportError>> = state
            .sse_chunks
            .iter()
            .map(|chunk| {
                Ok(TransportSseEvent {
                    data: serde_json::to_string(chunk).unwrap(),
                })
            })
            .collect();
        if let Some(error) = state.sse_error.take() {
            match error {
                BridgeError::Transport(error) => items.push(Err(error)),
                _ => items.push(Ok(TransportSseEvent {
                    data: "not json".to_owned(),
                })),
            }
        }
        drop(state);
        Ok(TransportSseResponse {
            status: 200,
            headers: vec![TransportHeader {
                name: "content-type".to_owned(),
                value: b"text/event-stream".to_vec(),
            }],
            body: TransportSseBody::Events(Box::pin(stream::iter(items))),
        })
    }
}

// serde_json text uses the shortest f32 spelling, while serde_json::to_value historically
// widened typed f32 values. Keep existing fixture views stable without changing request bytes.
fn restore_typed_float_view(value: &mut Value) {
    match value {
        Value::Number(number) if number.is_f64() => {
            if let Some(raw) = number.as_f64() {
                #[allow(clippy::cast_possible_truncation)]
                let typed = raw as f32;
                *number = serde_json::Number::from_f64(f64::from(typed)).unwrap();
            }
        }
        Value::Array(items) => items.iter_mut().for_each(restore_typed_float_view),
        Value::Object(fields) => fields.values_mut().for_each(restore_typed_float_view),
        _ => {}
    }
}

pub fn create_zip_with_single_png() -> Vec<u8> {
    use std::io::Write;

    let mut cursor = Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(&mut cursor);
    let options = zip::write::SimpleFileOptions::default();
    writer.start_file("image.png", options).unwrap();
    writer.write_all(&create_png_bytes(2, 2)).unwrap();
    writer.finish().unwrap();
    cursor.into_inner()
}

pub fn create_zip_with_two_images() -> Vec<u8> {
    use std::io::Write;

    let mut cursor = Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(&mut cursor);
    let options = zip::write::SimpleFileOptions::default();
    writer.start_file("first.png", options).unwrap();
    writer.write_all(&create_png_bytes(2, 2)).unwrap();
    writer.start_file("second.png", options).unwrap();
    writer.write_all(&create_png_bytes(3, 2)).unwrap();
    writer.finish().unwrap();
    cursor.into_inner()
}

pub fn create_png_with_comment(comment: &serde_json::Value) -> Vec<u8> {
    create_png_with_comment_text(&comment.to_string())
}

pub fn create_png_with_comment_text(comment: &str) -> Vec<u8> {
    let mut output = Vec::new();
    let mut encoder = png::Encoder::new(&mut output, 2, 2);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .add_text_chunk("Comment".to_owned(), comment.to_owned())
        .expect("add comment");
    let mut writer = encoder.write_header().expect("write header");
    writer
        .write_image_data(&[255_u8; 2 * 2 * 4])
        .expect("write image");
    drop(writer);
    output
}

pub fn create_zip_with_pngs(images: &[(&str, Vec<u8>)]) -> Vec<u8> {
    use std::io::Write;

    let mut cursor = Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(&mut cursor);
    let options = zip::write::SimpleFileOptions::default();
    for (name, bytes) in images {
        writer.start_file(*name, options).expect("start image");
        writer.write_all(bytes).expect("write image");
    }
    writer.finish().expect("finish zip");
    cursor.into_inner()
}

pub fn create_png_base64(width: u32, height: u32) -> String {
    base64::engine::general_purpose::STANDARD.encode(create_png_bytes(width, height))
}

pub fn create_png_bytes(width: u32, height: u32) -> Vec<u8> {
    let image = RgbaImage::from_pixel(width, height, Rgba([255, 255, 255, 255]));
    let mut cursor = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut cursor, ImageFormat::Png)
        .expect("failed to encode png");
    cursor.into_inner()
}
