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
    pub body: Value,
    pub registrations: Vec<String>,
    omitted: Vec<OmittedMedia>,
}

struct OmittedMedia {
    key: String,
    pointer: String,
    data: String,
}

impl CacheState {
    pub(super) fn transform(&mut self, mut body: Value) -> Result<CacheTransaction, BridgeError> {
        let secret = if let Some(secret) = self.secret {
            secret
        } else {
            let secret = super::random_bytes()?;
            self.secret = Some(secret);
            secret
        };
        let mut registrations = Vec::new();
        let mut omitted = Vec::new();
        if let Some(parameters) = body.get_mut("parameters").and_then(Value::as_object_mut) {
            for field in ["image", "mask", "reference_image"] {
                if let Some(Value::String(data)) = parameters.remove(field) {
                    let key = cache_key(&secret, &data);
                    parameters.insert(
                        format!("{field}_cache_secret_key"),
                        Value::String(key.clone()),
                    );
                    if self.known.contains(&key) {
                        omitted.push(OmittedMedia {
                            key,
                            pointer: format!("/parameters/{field}"),
                            data,
                        });
                    } else {
                        registrations.push(key);
                        parameters.insert(field.to_owned(), Value::String(data));
                    }
                }
            }
            for field in ["reference_image_multiple", "director_reference_images"] {
                if let Some(Value::Array(images)) = parameters.remove(field) {
                    let mut cached = Vec::with_capacity(images.len());
                    for (index, image) in images.into_iter().enumerate() {
                        if let Value::String(data) = image {
                            let key = cache_key(&secret, &data);
                            let mut entry = json!({ "cache_secret_key": key });
                            if self.known.contains(&key) {
                                omitted.push(OmittedMedia {
                                    key,
                                    pointer: format!("/parameters/{field}_cached/{index}/data"),
                                    data,
                                });
                            } else {
                                registrations.push(key);
                                entry["data"] = Value::String(data);
                            }
                            cached.push(entry);
                        }
                    }
                    parameters.insert(format!("{field}_cached"), Value::Array(cached));
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
    pub(super) const fn uncached(body: Value) -> Self {
        Self {
            body,
            registrations: Vec::new(),
            omitted: Vec::new(),
        }
    }

    pub(super) fn restore(&mut self, keys: &[String]) -> bool {
        let mut restored = false;
        for media in &mut self.omitted {
            if keys.contains(&media.key) {
                let (parent, field) = media
                    .pointer
                    .rsplit_once('/')
                    .expect("media paths have a field");
                if let Some(object) = self.body.pointer_mut(parent).and_then(Value::as_object_mut) {
                    object.insert(
                        field.to_owned(),
                        Value::String(std::mem::take(&mut media.data)),
                    );
                    self.registrations.push(media.key.clone());
                    restored = true;
                }
            }
        }
        restored
    }
}

fn cache_key(secret: &[u8; 32], data: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC accepts a 32-byte key");
    mac.update(data.as_bytes());
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
