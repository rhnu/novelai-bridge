#![allow(dead_code)]

use std::{
    io::Cursor,
    pin::Pin,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use base64::Engine;
use futures_core::Stream;
use futures_util::stream;
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use novelai_bridge::{BridgeError, ImageStreamChunk, Transport};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

#[derive(Clone, Default)]
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
    json_response: Value,
    json_error: Option<BridgeError>,
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

    pub fn set_json_response(&self, json_response: Value) {
        self.state.lock().unwrap().json_response = json_response;
    }

    pub fn set_json_error(&self, json_error: BridgeError) {
        self.state.lock().unwrap().json_error = Some(json_error);
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
    async fn get_json<TRes: DeserializeOwned + Send>(
        &self,
        path: &str,
    ) -> Result<TRes, BridgeError> {
        let mut state = self.state.lock().unwrap();
        state.last_path = Some(path.to_owned());
        if let Some(err) = state.json_error.clone() {
            return Err(err);
        }
        let payload = state.json_response.clone();
        drop(state);

        serde_json::from_value(payload).map_err(|err| {
            BridgeError::unknown_error(format!("mock get_json deserialize failed: {err}"))
        })
    }

    async fn post_bytes<TReq: Serialize + Send + Sync>(
        &self,
        path: &str,
        body: &TReq,
    ) -> Result<Vec<u8>, BridgeError> {
        let body = serde_json::to_value(body).map_err(|err| {
            BridgeError::unknown_error(format!("mock post_bytes serialize failed: {err}"))
        })?;

        let mut state = self.state.lock().unwrap();
        state.last_path = Some(path.to_owned());
        state.last_body = Some(body.clone());
        state.post_history.push((path.to_owned(), body));
        state.byte_paths.push(path.to_owned());

        if path.ends_with("/ai/encode-vibe") {
            Ok(state.encode_vibe_response.clone())
        } else {
            Ok(state.bytes_response.clone())
        }
    }

    async fn post_sse<TReq: Serialize + Send + Sync>(
        &self,
        path: &str,
        body: &TReq,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<ImageStreamChunk, BridgeError>> + Send>>,
        BridgeError,
    > {
        let body = serde_json::to_value(body).map_err(|err| {
            BridgeError::unknown_error(format!("mock post_sse serialize failed: {err}"))
        })?;

        let mut state = self.state.lock().unwrap();
        state.last_path = Some(path.to_owned());
        state.last_body = Some(body.clone());
        state.post_history.push((path.to_owned(), body));
        let items: Vec<Result<ImageStreamChunk, BridgeError>> =
            state.sse_chunks.clone().into_iter().map(Ok).collect();
        drop(state);

        Ok(Box::pin(stream::iter(items)))
    }
}

pub fn create_zip_with_single_png() -> Vec<u8> {
    use std::io::Write;

    let mut cursor = Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(&mut cursor);
    let options = zip::write::SimpleFileOptions::default();
    writer.start_file("image.png", options).unwrap();
    writer.write_all(&[0_u8, 1, 2, 3]).unwrap();
    writer.finish().unwrap();
    cursor.into_inner()
}

pub fn create_zip_with_two_images() -> Vec<u8> {
    use std::io::Write;

    let mut cursor = Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(&mut cursor);
    let options = zip::write::SimpleFileOptions::default();
    writer.start_file("first.png", options).unwrap();
    writer.write_all(&[11_u8, 22, 33]).unwrap();
    writer.start_file("second.png", options).unwrap();
    writer.write_all(&[44_u8, 55, 66]).unwrap();
    writer.finish().unwrap();
    cursor.into_inner()
}

pub fn create_png_base64(width: u32, height: u32) -> String {
    let image = RgbaImage::from_pixel(width, height, Rgba([255, 255, 255, 255]));
    let mut cursor = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut cursor, ImageFormat::Png)
        .expect("failed to encode png");
    base64::engine::general_purpose::STANDARD.encode(cursor.into_inner())
}
