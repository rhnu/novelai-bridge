use std::io::{Cursor, Write};

use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::json;

use super::*;
use crate::{BufferedResponse, GenerateImageResult, ImageResponseFormat, TransportHeader};

fn png() -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
    encoder.set_color(png::ColorType::Rgba);
    encoder
        .add_text_chunk("Comment".to_owned(), r#"{"seed":7}"#.to_owned())
        .unwrap();
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[255; 16]).unwrap();
    writer.finish().unwrap();
    bytes
}

fn archive(count: usize) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..count {
        writer
            .start_file(
                format!("{index}.png"),
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(&png()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn json_images(count: usize) -> Vec<u8> {
    serde_json::to_vec(&json!({ "images": (0..count).map(|_| json!({"image": STANDARD.encode(png())})).collect::<Vec<_>>() })).unwrap()
}

fn response(body: Vec<u8>, content_type: Option<&str>) -> BufferedResponse {
    BufferedResponse {
        status: 201,
        body,
        headers: content_type
            .map(|value| TransportHeader {
                name: "Content-Type".to_owned(),
                value: value.as_bytes().to_vec(),
            })
            .into_iter()
            .collect(),
    }
}

fn has_limit(error: &(dyn std::error::Error + 'static), limit: ImageLimit) -> bool {
    error
        .downcast_ref::<crate::ImagePolicyError>()
        .is_some_and(|error| error.limit == limit)
        || error
            .source()
            .is_some_and(|source| has_limit(source, limit))
}

#[test]
fn json_preserves_array_position_provider_identity_and_embedded_seed_independently() {
    let payload = serde_json::to_vec(&json!({
        "extension": { "ignored": true },
        "images": [
            {"image":STANDARD.encode(png()),"index":-7,"seed":11,"future":true},
            {"image":STANDARD.encode(png())},
            {"image":STANDARD.encode(png()),"index":i64::MAX,"seed":i64::MIN}
        ]
    }))
    .unwrap();
    let images =
        decode_generated_image_response(response(payload, Some("application/json"))).unwrap();
    let result = GenerateImageResult {
        resolved_seed: 99,
        images,
    };
    assert_eq!(result.resolved_seed, 99);
    for image in &result.images {
        assert_eq!(image.seed(), Some(7));
    }
    assert_eq!(
        result.images[0].source(),
        &GeneratedImageSource::Json {
            position: 0,
            index: Some(-7),
            seed: Some(11)
        }
    );
    assert_eq!(
        result.images[1].source(),
        &GeneratedImageSource::Json {
            position: 1,
            index: None,
            seed: None
        }
    );
    assert_eq!(
        result.images[2].source(),
        &GeneratedImageSource::Json {
            position: 2,
            index: Some(i64::MAX),
            seed: Some(i64::MIN)
        }
    );
    assert_eq!(result.images[0].bytes(), png());
}

#[test]
fn image_response_content_types_must_match_the_selected_container() {
    for (content_type, payload, accepted) in [
        ("application/json", json_images(1), true),
        ("application/zip", archive(1), true),
        ("image/png", png(), true),
        ("Application/JSON; charset=utf-8", json_images(1), true),
        ("application/octet-stream", png(), true),
        ("application/octet-stream", archive(1), true),
        ("application/json", png(), false),
        ("application/zip", png(), false),
        ("image/png", json_images(1), false),
        ("image/png", archive(1), false),
        ("image/jpeg", png(), false),
        ("text/plain", png(), false),
        ("application/octet-stream", json_images(1), false),
        ("image/png, application/json", png(), false),
        ("", png(), false),
    ] {
        assert_eq!(
            decode_generated_image_response(response(payload, Some(content_type))).is_ok(),
            accepted,
            "{content_type}"
        );
    }
    for payload in [png(), archive(1), json_images(1)] {
        assert!(decode_generated_image_response(response(payload.clone(), None)).is_ok());
        assert!(decode_generated_images(payload).is_ok());
    }
}

#[test]
fn invalid_or_repeated_content_types_never_fall_back_to_a_raw_image() {
    let mut invalid = response(png(), Some("image/png"));
    invalid.headers[0].value = vec![0xFF];
    assert!(decode_generated_image_response(invalid).is_err());
    for second in ["image/png", "application/json"] {
        let mut repeated = response(png(), Some("image/png"));
        repeated.headers.push(TransportHeader {
            name: "content-type".to_owned(),
            value: second.as_bytes().to_vec(),
        });
        assert!(decode_generated_image_response(repeated).is_err());
    }
}

#[test]
fn malformed_json_or_a_non_first_image_rejects_the_complete_response() {
    for value in [
        json!({}),
        json!({"images":[]}),
        json!({"images":null}),
        json!({"images":[{}]}),
        json!({"images":[{"image":STANDARD.encode(png()),"index":1.5}]}),
        json!({"images":[{"image":STANDARD.encode(png()),"seed":"11"}]}),
        json!({"images":[{"image":STANDARD.encode(png())},{"image":"!invalid!"}]}),
        json!({"images":[{"image":STANDARD.encode(png())},{"image":STANDARD.encode(b"not an image")}]}),
    ] {
        assert!(
            decode_generated_images(serde_json::to_vec(&value).unwrap()).is_err(),
            "{value}"
        );
    }
    let mut trailing = json_images(1);
    trailing.extend_from_slice(b" {}");
    assert!(decode_generated_images(trailing).is_err());
    assert!(decode_generated_images(b"{\"images\":[],\"images\":[]}".to_vec()).is_err());
}

#[test]
fn every_container_obeys_the_same_image_count_and_total_byte_budgets() {
    for payload in [png(), archive(1), json_images(1)] {
        let policy = ImageDecodePolicy {
            max_images: 0,
            ..Default::default()
        };
        let error = decode_generated_images_with_policy(payload, policy).unwrap_err();
        assert!(has_limit(&error, ImageLimit::Images));
    }
    for payload in [archive(2), json_images(2)] {
        let policy = ImageDecodePolicy {
            max_images: 1,
            ..Default::default()
        };
        let error = decode_generated_images_with_policy(payload, policy).unwrap_err();
        assert!(has_limit(&error, ImageLimit::Images));
    }
    for (payload, maximum) in [
        (png(), png().len() - 1),
        (archive(2), 2 * png().len() - 1),
        (json_images(2), 2 * png().len() - 1),
    ] {
        let policy = ImageDecodePolicy {
            max_total_image_bytes: maximum as u64,
            ..Default::default()
        };
        let error = decode_generated_images_with_policy(payload, policy).unwrap_err();
        assert!(has_limit(&error, ImageLimit::TotalImageBytes));
    }
}

#[test]
fn json_enforces_count_and_decoded_byte_budgets_before_parsing_or_allocating_next_image() {
    let value = json!({"images":[{"image":STANDARD.encode(png())},{"image": {"not":"an image"}}]});
    let policy = ImageDecodePolicy {
        max_images: 1,
        ..Default::default()
    };
    let error = decode_generated_images_with_policy(serde_json::to_vec(&value).unwrap(), policy)
        .unwrap_err();
    assert!(has_limit(&error, ImageLimit::Images));
    for encoded in ["!!!!", "========"] {
        let value = json!({"images":[{"image":encoded}]});
        let policy = ImageDecodePolicy {
            max_encoded_bytes: 1,
            ..Default::default()
        };
        let error =
            decode_generated_images_with_policy(serde_json::to_vec(&value).unwrap(), policy)
                .unwrap_err();
        assert!(has_limit(&error, ImageLimit::EncodedBytes));
    }
}

#[test]
fn response_format_is_an_editable_output_option_with_zip_default() {
    use crate::{GenerateImageRequest, Model, OutputParams};
    let request = GenerateImageRequest::builder("cat", Model::NaiDiffusion5Full)
        .response_format(ImageResponseFormat::Json)
        .build();
    assert_eq!(request.output.response_format, ImageResponseFormat::Json);
    assert_eq!(ImageResponseFormat::Zip.accept(), "application/zip");
    assert_eq!(ImageResponseFormat::Json.accept(), "application/json");
    let value = serde_json::to_value(&request).unwrap();
    assert_eq!(value["output"]["response_format"], "json");
    assert_eq!(
        serde_json::from_value::<GenerateImageRequest>(value).unwrap(),
        request
    );
    let mut output = serde_json::to_value(OutputParams::default()).unwrap();
    output.as_object_mut().unwrap().remove("response_format");
    assert_eq!(
        serde_json::from_value::<OutputParams>(output)
            .unwrap()
            .response_format,
        ImageResponseFormat::Zip
    );
    assert!(serde_json::from_str::<ImageResponseFormat>("\"xml\"").is_err());
}
