use std::{borrow::Cow, fmt};

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{
    Deserialize,
    de::{self, DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor},
};

use super::{ResponseBudget, response_message, validated_generated_image};
use crate::{
    BridgeError, DecodeError, DecodeTarget, GeneratedImage, GeneratedImageSource, ImageDecodePolicy,
};

#[derive(Deserialize)]
struct JsonImage<'a> {
    #[serde(borrow)]
    image: Cow<'a, str>,
    index: Option<i64>,
    seed: Option<i64>,
}

pub(super) fn decode(
    payload: &[u8],
    policy: ImageDecodePolicy,
) -> Result<Vec<GeneratedImage>, BridgeError> {
    let mut deserializer = serde_json::Deserializer::from_slice(payload);
    let mut budget = ResponseBudget::new(policy);
    let mut failure = None;
    let result = EnvelopeSeed {
        budget: &mut budget,
        failure: &mut failure,
    }
    .deserialize(&mut deserializer);
    let images = match result {
        Ok(images) => images,
        Err(error) => return Err(failure.unwrap_or_else(|| json_error(error))),
    };
    deserializer.end().map_err(json_error)?;
    if images.is_empty() {
        return Err(response_message("JSON image response contains no images"));
    }
    Ok(images)
}

struct EnvelopeSeed<'a> {
    budget: &'a mut ResponseBudget,
    failure: &'a mut Option<BridgeError>,
}

impl<'de> DeserializeSeed<'de> for EnvelopeSeed<'_> {
    type Value = Vec<GeneratedImage>;

    fn deserialize<D: de::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for EnvelopeSeed<'_> {
    type Value = Vec<GeneratedImage>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an object containing an images array")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut images = None;
        while let Some(key) = map.next_key::<Cow<'de, str>>()? {
            if key == "images" {
                if images.is_some() {
                    return Err(de::Error::duplicate_field("images"));
                }
                images = Some(map.next_value_seed(ImageListSeed {
                    budget: &mut *self.budget,
                    failure: &mut *self.failure,
                })?);
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        images.ok_or_else(|| de::Error::missing_field("images"))
    }
}

struct ImageListSeed<'a> {
    budget: &'a mut ResponseBudget,
    failure: &'a mut Option<BridgeError>,
}

impl<'de> DeserializeSeed<'de> for ImageListSeed<'_> {
    type Value = Vec<GeneratedImage>;

    fn deserialize<D: de::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for ImageListSeed<'_> {
    type Value = Vec<GeneratedImage>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an array of base64 image records")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut images = Vec::new();
        while let Some(image) = seq.next_element_seed(ImageSeed {
            budget: &mut *self.budget,
            failure: &mut *self.failure,
        })? {
            images.push(image);
        }
        Ok(images)
    }
}

struct ImageSeed<'a> {
    budget: &'a mut ResponseBudget,
    failure: &'a mut Option<BridgeError>,
}

impl<'de> DeserializeSeed<'de> for ImageSeed<'_> {
    type Value = GeneratedImage;

    fn deserialize<D: de::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        let position = usize::try_from(self.budget.images).unwrap_or(usize::MAX);
        self.budget
            .begin_image()
            .map_err(|error| retain_failure(self.failure, error))?;
        let record = JsonImage::deserialize(deserializer)?;
        decode_record(&record, position, self.budget)
            .map_err(|error| retain_failure(self.failure, error))
    }
}

fn decode_record(
    record: &JsonImage<'_>,
    position: usize,
    budget: &mut ResponseBudget,
) -> Result<GeneratedImage, BridgeError> {
    let encoded = record.image.as_bytes();
    let padding = encoded
        .iter()
        .rev()
        .take(2)
        .take_while(|&&byte| byte == b'=')
        .count();
    let decoded_size = (encoded.len() as u64)
        .saturating_add(3)
        .saturating_div(4)
        .saturating_mul(3)
        .saturating_sub(padding as u64);
    budget.check_bytes(decoded_size)?;
    let bytes = STANDARD.decode(encoded).map_err(super::response_error)?;
    budget.add_bytes(bytes.len() as u64)?;
    validated_generated_image(
        bytes,
        GeneratedImageSource::Json {
            position,
            index: record.index,
            seed: record.seed,
        },
        budget.policy,
    )
}

fn retain_failure<E: de::Error>(failure: &mut Option<BridgeError>, error: BridgeError) -> E {
    *failure = Some(error);
    E::custom("image record rejected")
}

fn json_error(error: serde_json::Error) -> BridgeError {
    DecodeError::new(DecodeTarget::JsonResponse, Box::new(error)).into()
}
