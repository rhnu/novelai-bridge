//! Check collection and payload budgets before constructing the owned document.
use std::{borrow::Cow, fmt};

use serde::de::{self, DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};

use super::{VibeDocumentError, VibeDocumentLimits, VibeDocumentResource};

#[derive(Copy, Clone)]
enum Node {
    File,
    BundleItem,
    Vibes,
    Encodings,
    Parameters,
    Record,
    Params,
    ImportInfo,
    Payload,
}

struct Budget {
    limits: VibeDocumentLimits,
    vibes: usize,
    cache_entries: usize,
    payload_bytes: usize,
    failure: Option<VibeDocumentError>,
}

impl Budget {
    fn check<E: de::Error>(
        &mut self,
        resource: VibeDocumentResource,
        value: usize,
    ) -> Result<(), E> {
        self.limits.check(resource, value).map_err(|error| {
            self.failure = Some(error);
            E::custom("vibe document resource budget exceeded")
        })
    }
}

pub(super) fn check(
    bytes: &[u8],
    bundle: bool,
    limits: VibeDocumentLimits,
) -> Result<(), VibeDocumentError> {
    let mut budget = Budget {
        limits,
        vibes: usize::from(!bundle),
        cache_entries: 0,
        payload_bytes: 0,
        failure: None,
    };
    limits.check(VibeDocumentResource::Vibes, budget.vibes)?;
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    let result = Seed {
        budget: &mut budget,
        node: Node::File,
    }
    .deserialize(&mut decoder);
    if let Err(error) = result {
        return Err(budget.failure.unwrap_or_else(|| error.into()));
    }
    decoder.end()?;
    Ok(())
}

struct Seed<'a> {
    budget: &'a mut Budget,
    node: Node,
}

impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = ();

    fn deserialize<D: de::Deserializer<'de>>(self, decoder: D) -> Result<(), D::Error> {
        if matches!(self.node, Node::BundleItem) {
            self.budget.vibes = self.budget.vibes.saturating_add(1);
            self.budget
                .check(VibeDocumentResource::Vibes, self.budget.vibes)?;
        }
        decoder.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Seed<'_> {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an official vibe document field")
    }

    fn visit_unit<E: de::Error>(self) -> Result<(), E> { Ok(()) }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<(), E> {
        if !matches!(self.node, Node::Payload) {
            return Err(de::Error::invalid_type(de::Unexpected::Str(value), &self));
        }
        self.budget
            .check(VibeDocumentResource::PayloadBytes, value.len())?;
        self.budget.payload_bytes = self.budget.payload_bytes.saturating_add(value.len());
        self.budget.check(
            VibeDocumentResource::TotalPayloadBytes,
            self.budget.payload_bytes,
        )
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<(), E> { self.visit_str(&value) }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        if !matches!(self.node, Node::Vibes) {
            return Err(de::Error::invalid_type(de::Unexpected::Seq, &self));
        }
        while seq
            .next_element_seed(Seed {
                budget: &mut *self.budget,
                node: Node::BundleItem,
            })?
            .is_some()
        {}
        Ok(())
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        while let Some(key) = map.next_key::<Cow<'de, str>>()? {
            let child = match self.node {
                Node::Encodings | Node::Parameters => {
                    self.budget.cache_entries = self.budget.cache_entries.saturating_add(1);
                    self.budget.check(
                        VibeDocumentResource::CacheEntries,
                        self.budget.cache_entries,
                    )?;
                    Some(if matches!(self.node, Node::Encodings) {
                        Node::Parameters
                    } else {
                        Node::Record
                    })
                },
                Node::File | Node::BundleItem => match key.as_ref() {
                    "vibes" => Some(Node::Vibes),
                    "encodings" => Some(Node::Encodings),
                    "image" | "thumbnail" => Some(Node::Payload),
                    "importInfo" => Some(Node::ImportInfo),
                    _ => None,
                },
                Node::Record => match key.as_ref() {
                    "encoding" => Some(Node::Payload),
                    "params" => Some(Node::Params),
                    _ => None,
                },
                Node::Params | Node::ImportInfo if key == "mask" => Some(Node::Payload),
                _ => None,
            };
            if let Some(node) = child {
                map.next_value_seed(Seed {
                    budget: &mut *self.budget,
                    node,
                })?;
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(())
    }
}
