use std::collections::{BTreeSet, VecDeque};

use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::Sha256;

use crate::BridgeError;

#[derive(Default)]
pub(in crate::client) struct CacheState {
    secret: Option<[u8; 32]>,
    known: BTreeSet<String>,
    order: VecDeque<String>,
}

pub(super) struct CacheTransaction {
    pub body: crate::image::request_body::MediaBody,
    pub registrations: Vec<String>,
    omitted: Vec<(usize, String)>,
}

impl CacheState {
    pub(super) fn transform(
        &mut self,
        mut body: crate::image::request_body::MediaBody,
    ) -> Result<CacheTransaction, BridgeError> {
        let secret = if let Some(secret) = self.secret {
            secret
        } else {
            let secret = super::random_bytes()?;
            self.secret = Some(secret);
            secret
        };
        let mut registrations = Vec::new();
        let mut omitted = Vec::new();
        if let Some(parameters) = body
            .value
            .get_mut("parameters")
            .and_then(Value::as_object_mut)
        {
            for field in ["reference_image_multiple", "director_reference_images"] {
                if let Some(Value::Array(images)) = parameters.remove(field) {
                    parameters.insert(
                        format!("{field}_cached"),
                        Value::Array(images.into_iter().map(|_| json!({})).collect()),
                    );
                }
            }
            for (index, media) in body.media.iter_mut().enumerate() {
                let Some(path) = media
                    .pointer
                    .strip_prefix("/parameters/")
                    .map(str::to_owned)
                else {
                    continue;
                };
                let key = cache_key(&secret, media.data.bytes());
                if let Some((field, position)) = path.split_once('/') {
                    media.pointer = format!("/parameters/{field}_cached/{position}/data");
                    let entry = parameters
                        .get_mut(&format!("{field}_cached"))
                        .and_then(Value::as_array_mut)
                        .and_then(|entries| {
                            entries.get_mut(
                                position
                                    .parse::<usize>()
                                    .expect("prepared reference index is numeric"),
                            )
                        })
                        .and_then(Value::as_object_mut)
                        .expect("prepared reference arrays have matching media entries");
                    entry.insert("cache_secret_key".to_owned(), Value::String(key.clone()));
                    if !self.known.contains(&key) {
                        entry.insert("data".to_owned(), Value::String(String::new()));
                    }
                } else {
                    parameters.insert(
                        format!("{path}_cache_secret_key"),
                        Value::String(key.clone()),
                    );
                    if self.known.contains(&key) {
                        parameters.remove(&path);
                    }
                }
                if self.known.contains(&key) {
                    media.omitted = true;
                    omitted.push((index, key));
                } else {
                    registrations.push(key);
                }
            }
        }
        Ok(CacheTransaction {
            body,
            registrations,
            omitted,
        })
    }

    pub(super) fn invalidate(&mut self, keys: &[String]) {
        for key in keys {
            self.known.remove(key);
        }
        self.order.retain(|key| self.known.contains(key));
    }

    pub(super) fn register(&mut self, keys: Vec<String>, maximum: usize) {
        for key in keys {
            if maximum > 0 && self.known.insert(key.clone()) {
                self.order.push_back(key);
            }
        }
        while self.order.len() > maximum {
            if let Some(oldest) = self.order.pop_front() {
                self.known.remove(&oldest);
            }
        }
    }
}

impl CacheTransaction {
    pub(super) const fn uncached(body: crate::image::request_body::MediaBody) -> Self {
        Self {
            body,
            registrations: Vec::new(),
            omitted: Vec::new(),
        }
    }

    pub(super) fn restore(&mut self, keys: &[String]) -> bool {
        let mut restored = false;
        for (index, key) in &self.omitted {
            let media = &mut self.body.media[*index];
            if media.omitted && keys.contains(key) {
                let (parent, field) = media
                    .pointer
                    .rsplit_once('/')
                    .expect("media paths have a field");
                let object = self
                    .body
                    .value
                    .pointer_mut(parent)
                    .and_then(Value::as_object_mut)
                    .expect("omitted media retains its parent object");
                object.insert(field.to_owned(), Value::String(String::new()));
                media.omitted = false;
                self.registrations.push(key.clone());
                restored = true;
            }
        }
        restored
    }
}

fn cache_key(secret: &[u8; 32], data: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC accepts a 32-byte key");
    mac.update(data);
    super::hex(&mac.finalize().into_bytes())
}

pub(super) fn invalid_keys(error: &BridgeError) -> Option<Vec<String>> {
    #[derive(Deserialize)]
    struct ErrorBody {
        message: String,
        details: Details,
    }
    #[derive(Deserialize)]
    struct Details {
        #[serde(rename = "invalidKeys")]
        invalid_keys: Vec<String>,
    }
    let BridgeError::Api(error) = error else {
        return None;
    };
    if error.status() != 400 || error.raw_body_truncated {
        return None;
    }
    let body: ErrorBody = serde_json::from_str(error.raw_body.as_deref()?).ok()?;
    (body.message == "INVALID_CACHE_KEYS").then_some(body.details.invalid_keys)
}
