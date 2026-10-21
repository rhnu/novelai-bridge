use base64::{Engine, engine::general_purpose::STANDARD};

use crate::{
    GeneratedImageMetadata, MetadataWarning, VibeMetadataSource,
    extract_embedded_vibe_document_from_png_bytes, inspect_generated_image_metadata,
    parse_png_metadata_from_bytes, parse_png_metadata_from_payload,
};

fn create_png_with_text_chunks(chunks: &[(&str, &str)]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut encoder = png::Encoder::new(&mut output, 2, 2);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    for (keyword, value) in chunks {
        encoder
            .add_text_chunk((*keyword).to_owned(), (*value).to_owned())
            .expect("add text chunk");
    }

    let mut writer = encoder.write_header().expect("write header");
    let pixels = vec![255u8; 2 * 2 * 4];
    writer
        .write_image_data(pixels.as_slice())
        .expect("write image");
    drop(writer);
    output
}

fn create_png_with_post_image_text_chunks(chunks: &[png::text_metadata::TEXtChunk]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut encoder = png::Encoder::new(&mut output, 2, 2);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("write header");
    writer
        .write_image_data(&[255_u8; 2 * 2 * 4])
        .expect("write image");
    for chunk in chunks {
        writer.write_text_chunk(chunk).expect("write text chunk");
    }
    writer.finish().expect("finish png");
    output
}

#[test]
fn inspect_metadata_distinguishes_absent_invalid_and_unsupported() {
    let empty_png = create_png_with_text_chunks(&[]);
    assert!(matches!(
        inspect_generated_image_metadata(&empty_png, Some("image/png")),
        GeneratedImageMetadata::NotPresent
    ));
    assert!(matches!(
        inspect_generated_image_metadata(&[0x89, b'P', b'N', b'G'], Some("image/png")),
        GeneratedImageMetadata::Invalid { .. }
    ));
    assert!(matches!(
        inspect_generated_image_metadata(b"RIFF0000WEBP", Some("image/webp")),
        GeneratedImageMetadata::Invalid { .. }
    ));
}

#[test]
fn complete_carrier_precedence_does_not_fill_missing_fields_from_a_lower_priority_source() {
    use crate::metadata::{
        MetadataOrigin,
        test_support::{ascii_comment, png, tiff},
    };
    let hidden = r#"{"Description":"hidden prompt","Comment":"{\"seed\":42}","Software":"hidden"}"#;
    let exif = tiff(&ascii_comment(
        r#"{"Description":"Exif prompt","Source":"Exif source"}"#,
    ));
    let png_text = png(&[("Comment", r#"{"seed":7}"#)], Some(&exif), Some(hidden));
    let metadata = crate::metadata::parse_image_metadata_from_bytes(&png_text).unwrap();
    assert_eq!(metadata.origin, Some(MetadataOrigin::PngText));
    assert_eq!(metadata.seed, Some(7));
    assert_eq!(metadata.prompt, None);
    assert_eq!(metadata.source, None);
    let exif_only = png(&[], Some(&exif), Some(hidden));
    let metadata = crate::metadata::parse_image_metadata_from_bytes(&exif_only).unwrap();
    assert_eq!(metadata.origin, Some(MetadataOrigin::ExifUserComment));
    assert_eq!(metadata.prompt.as_deref(), Some("Exif prompt"));
    assert_eq!(metadata.seed, None);
    assert_eq!(metadata.software, None);
    let hidden_only = png(&[], None, Some(hidden));
    let metadata = crate::metadata::parse_png_metadata_from_bytes(&hidden_only).unwrap();
    assert_eq!(metadata.origin, Some(MetadataOrigin::StealthPngComp));
    assert_eq!(metadata.seed, Some(42));
    assert_eq!(metadata.prompt.as_deref(), Some("hidden prompt"));
}

#[test]
fn invalid_metadata_warns_but_png_media_failure_cannot_be_hidden_by_fallback() {
    use crate::metadata::{MetadataOrigin, test_support::png};
    let hidden = r#"{"Comment":"{\"seed\":42}"}"#;
    let bytes = png(&[], Some(b"not TIFF"), Some(hidden));
    let metadata = crate::metadata::parse_image_metadata_from_bytes(&bytes).unwrap();
    assert_eq!(metadata.origin, Some(MetadataOrigin::StealthPngComp));
    assert_eq!(metadata.seed, Some(42));
    assert!(matches!(
        metadata.warnings.as_slice(),
        [MetadataWarning::InvalidCarrier {
            origin: MetadataOrigin::ExifUserComment,
            ..
        }]
    ));
    let mut truncated = bytes;
    truncated.truncate(truncated.len() - 12);
    assert!(crate::metadata::parse_image_metadata_from_bytes(&truncated).is_err());
    let malformed = png(&[], None, Some("{not JSON"));
    let metadata = crate::metadata::parse_image_metadata_from_bytes(&malformed).unwrap();
    assert_eq!(metadata.origin, None);
    assert!(matches!(
        metadata.warnings.as_slice(),
        [MetadataWarning::InvalidCarrier {
            origin: MetadataOrigin::StealthPngComp,
            ..
        }]
    ));
}

#[test]
fn generic_reader_recovers_webp_and_jpeg_exif_and_checks_the_same_media_budget() {
    use crate::metadata::{
        MetadataOrigin,
        test_support::{ascii_comment, jpeg, tiff, webp},
    };
    let raw = tiff(&ascii_comment(
        r#"{"Comment":"{\"seed\":123,\"prompt\":\"cat\"}"}"#,
    ));
    for bytes in [webp(&raw), jpeg(&raw)] {
        let metadata = crate::metadata::parse_image_metadata_from_bytes(&bytes).unwrap();
        assert_eq!(metadata.origin, Some(MetadataOrigin::ExifUserComment));
        assert_eq!(metadata.prompt.as_deref(), Some("cat"));
        assert_eq!(metadata.seed, Some(123));
        assert!(crate::metadata::parse_png_metadata_from_bytes(&bytes).is_err());
        let policy = crate::ImageDecodePolicy {
            max_width: 1,
            ..Default::default()
        };
        assert!(
            crate::metadata::parse_image_metadata_from_bytes_with_policy(&bytes, policy).is_err()
        );
    }
}

#[test]
fn malformed_comment_is_recoverable_and_diagnostic() {
    let png =
        create_png_with_text_chunks(&[("Comment", "{not-json"), ("prompt", "fallback prompt")]);
    let metadata = parse_png_metadata_from_bytes(&png).expect("valid png");
    assert_eq!(metadata.prompt.as_deref(), Some("fallback prompt"));
    assert!(
        matches!(metadata.warnings.as_slice(), [MetadataWarning::InvalidCommentJson { message }] if message.contains("line 1"))
    );
}

#[test]
fn parses_text_chunks_after_image_data() {
    let comment = serde_json::json!({
        "prompt": "post-idat prompt",
        "seed": 9876
    })
    .to_string();
    let png = create_png_with_post_image_text_chunks(&[png::text_metadata::TEXtChunk::new(
        "Comment", comment,
    )]);

    let metadata = parse_png_metadata_from_bytes(&png).expect("valid png");
    assert_eq!(metadata.prompt.as_deref(), Some("post-idat prompt"));
    assert_eq!(metadata.seed, Some(9876));
}

#[test]
fn rejects_png_truncated_before_iend() {
    let mut png = create_png_with_text_chunks(&[("prompt", "should not survive truncation")]);
    png.truncate(png.len() - 12);

    assert!(parse_png_metadata_from_bytes(&png).is_err());
    assert!(matches!(
        inspect_generated_image_metadata(&png, Some("image/png")),
        GeneratedImageMetadata::Invalid { .. }
    ));
}

#[test]
fn invalid_compressed_text_is_recoverable_and_diagnostic() {
    let mut output = Vec::new();
    let mut encoder = png::Encoder::new(&mut output, 2, 2);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .add_text_chunk("prompt".to_owned(), "fallback prompt".to_owned())
        .expect("add prompt");
    let mut writer = encoder.write_header().expect("write header");
    writer
        .write_image_data(&[255_u8; 2 * 2 * 4])
        .expect("write image");
    let mut invalid_ztxt = b"Comment\0\0".to_vec();
    invalid_ztxt.extend_from_slice(b"not-zlib-data");
    writer
        .write_chunk(png::chunk::zTXt, &invalid_ztxt)
        .expect("write invalid zTXt");
    writer.finish().expect("finish png");

    let metadata = parse_png_metadata_from_bytes(&output).expect("valid png structure");
    assert_eq!(metadata.prompt.as_deref(), Some("fallback prompt"));
    assert!(matches!(
        metadata.warnings.as_slice(),
        [MetadataWarning::InvalidTextChunk { keyword, .. }] if keyword == "Comment"
    ));
}

#[test]
fn parse_comment_extracts_regular_fields() {
    let comment = serde_json::json!({
        "prompt": "1girl",
        "uc": "lowres",
        "seed": 42,
        "steps": 28,
        "sampler": "k_euler_ancestral",
        "scale": 6.5,
        "width": 832,
        "height": 1216,
        "ucPreset": 1,
        "qualityToggle": true,
        "v4_prompt": {
            "caption": { "char_captions": [ { "char_caption": "alice" } ] }
        }
    })
    .to_string();
    let png = create_png_with_text_chunks(&[("Comment", comment.as_str())]);
    let metadata = parse_png_metadata_from_bytes(png.as_slice()).expect("parse");
    assert_eq!(metadata.prompt.as_deref(), Some("1girl"));
    assert_eq!(metadata.negative_prompt.as_deref(), Some("lowres"));
    assert_eq!(metadata.seed, Some(42));
    assert_eq!(metadata.steps, Some(28));
    assert_eq!(metadata.sampler.as_deref(), Some("k_euler_ancestral"));
    assert_eq!(metadata.scale, Some(6.5));
    assert_eq!(metadata.width, Some(832));
    assert_eq!(metadata.height, Some(1216));
    assert_eq!(metadata.uc_preset, Some(1));
    assert_eq!(metadata.quality_toggle, Some(true));
    assert_eq!(
        metadata.characters,
        vec![crate::metadata::CharacterMetadata {
            source_index: 0,
            prompt: Some("alice".to_owned()),
            negative_prompt: None
        }]
    );
    assert!(metadata.has_generation_data());
}

#[test]
fn parse_vibe_chunks_extracts_naidata_and_direct_encoding() {
    let bundle = serde_json::json!({
        "identifier": "novelai-vibe-transfer-bundle",
        "vibes": [
            {
                "name": "demo",
                "encodings": {
                    "vibe": "QUJDREVGR0hJSktMTU5PUFFSU1Q=",
                    "v4-5full": { "unknown": { "encoding": "R0hJSktMTU5PUFFSU1RVVldYWVo=" } }
                },
                "importInfo": {
                    "strength": 0.55,
                    "information_extracted": 0.73
                }
            }
        ]
    })
    .to_string();
    let encoded_bundle = STANDARD.encode(bundle.as_bytes());
    let png = create_png_with_text_chunks(&[
        ("naidata", encoded_bundle.as_str()),
        (
            "NovelAI_Vibe_Encoding_Base64",
            "U1RVVldYWVo0NTY3ODkwQUJDRA==",
        ),
    ]);
    let metadata = parse_png_metadata_from_bytes(png.as_slice()).expect("parse");
    assert_eq!(metadata.vibe_entries.len(), 2);
    let naidata = metadata
        .vibe_entries
        .iter()
        .find(|entry| entry.source == VibeMetadataSource::NaiDataChunk)
        .expect("naidata entry");
    assert_eq!(naidata.display_name.as_deref(), Some("demo"));
    assert_eq!(
        naidata.encoding.as_deref(),
        Some("QUJDREVGR0hJSktMTU5PUFFSU1Q=")
    );
    assert_eq!(
        naidata
            .model_encodings
            .get("v4-5full")
            .and_then(|records| records.get("unknown")),
        Some(&"R0hJSktMTU5PUFFSU1RVVldYWVo=".to_owned())
    );
    assert!(
        metadata
            .vibe_entries
            .iter()
            .any(|entry| entry.source == VibeMetadataSource::NovelAiVibeEncodingChunk)
    );
}

#[test]
fn parse_payload_and_parameters_fallback() {
    let parameters = "masterpiece, 1girl\nNegative prompt: lowres\nSteps: 30, Sampler: Euler a, CFG scale: 7.5, Seed: 123, Size: 832x1216";
    let png = create_png_with_text_chunks(&[("parameters", parameters)]);
    let payload = format!("data:image/png;base64,{}", STANDARD.encode(png));
    let metadata = parse_png_metadata_from_payload(payload.as_str()).expect("parse");
    assert_eq!(metadata.prompt.as_deref(), Some("masterpiece, 1girl"));
    assert_eq!(metadata.negative_prompt.as_deref(), Some("lowres"));
    assert_eq!(metadata.steps, Some(30));
    assert_eq!(metadata.sampler.as_deref(), Some("Euler a"));
    assert_eq!(metadata.scale, Some(7.5));
    assert_eq!(metadata.seed, Some(123));
    assert_eq!(metadata.width, Some(832));
    assert_eq!(metadata.height, Some(1216));
}

#[test]
fn extract_embedded_vibe_document_reads_naidata() {
    let document = serde_json::json!({
        "identifier": "novelai-vibe-transfer",
        "version": 1,
        "type": "image"
    })
    .to_string();
    let encoded = STANDARD.encode(document.as_bytes());
    let png = create_png_with_text_chunks(&[("naidata", encoded.as_str())]);
    let extracted = extract_embedded_vibe_document_from_png_bytes(png.as_slice()).expect("extract");
    assert!(extracted.is_some());
    assert!(
        extracted
            .unwrap()
            .contains("\"identifier\":\"novelai-vibe-transfer\"")
    );
}

#[test]
fn vibe_metadata_keeps_model_encodings_separate() {
    let document = serde_json::json!({"encodings": {"v4-5full": {"unknown": {"encoding": "AQID"}, "other-params": {"encoding":"BAUG"}}, "v4full": {"unknown":{"encoding":"BwgJ"}}}});
    let png = create_png_with_text_chunks(&[("naidata", &document.to_string())]);
    let metadata = parse_png_metadata_from_bytes(&png).unwrap();
    assert_eq!(metadata.vibe_entries[0].encoding, None);
    assert_eq!(
        metadata.vibe_entries[0].model_encodings["v4-5full"]["unknown"],
        "AQID"
    );
    assert_eq!(
        metadata.vibe_entries[0].model_encodings["v4-5full"]["other-params"],
        "BAUG"
    );
    assert_eq!(
        metadata.vibe_entries[0].model_encodings["v4full"]["unknown"],
        "BwgJ"
    );
}

#[test]
fn embedded_vibe_extraction_preserves_original_json_order_and_strings() {
    let raw = r#"{ "identifier":"novelai-vibe-transfer", "encodings":{"z-last":{"z-params":{"encoding":" Q Q = = "}},"a-first":{"unknown":{"encoding":"BAUG"}}}, "version":1,"type":"encoding" }"#;
    for text in [raw.to_owned(), STANDARD.encode(raw)] {
        let bytes = create_png_with_text_chunks(&[("naidata", &text)]);
        assert_eq!(
            extract_embedded_vibe_document_from_png_bytes(&bytes)
                .unwrap()
                .as_deref(),
            Some(raw)
        );
    }
}

#[test]
fn malformed_reference_strength_does_not_shift_later_values() {
    let comment = serde_json::json!({"reference_image_multiple": ["AQID", "BAUG"], "reference_strength_multiple": ["bad", 0.7]});
    let png = create_png_with_text_chunks(&[("Comment", &comment.to_string())]);
    let metadata = parse_png_metadata_from_bytes(&png).unwrap();
    assert_eq!(metadata.vibe_entries[0].strength, None);
    assert_eq!(metadata.vibe_entries[1].strength, Some(0.7));
}

#[test]
fn character_metadata_preserves_empty_slots_and_pairs_source_indices() {
    let comment = serde_json::json!({
        "v4_prompt": {"caption": {"char_captions": [{"char_caption": "alice"}, {"char_caption": "bob"}]}},
        "v4_negative_prompt": {"caption": {"char_captions": [{"char_caption": ""}, {"char_caption": "bad bob"}]}}
    });
    let metadata = parse_png_metadata_from_bytes(&create_png_with_text_chunks(&[(
        "Comment",
        &comment.to_string(),
    )]))
    .unwrap();
    assert_eq!(metadata.characters.len(), 2);
    assert_eq!(metadata.characters[0].source_index, 0);
    assert_eq!(metadata.characters[0].negative_prompt, None);
    assert_eq!(metadata.characters[1].source_index, 1);
    assert_eq!(metadata.characters[1].prompt.as_deref(), Some("bob"));
    assert_eq!(
        metadata.characters[1].negative_prompt.as_deref(),
        Some("bad bob")
    );
}

#[test]
fn non_finite_and_unrepresentable_numeric_metadata_is_diagnostic() {
    let comment = r#"{"scale":1e40,"cfg_rescale":"NaN","reference_image_multiple":["AQID"],"reference_strength_multiple":["inf"]}"#;
    let metadata =
        parse_png_metadata_from_bytes(&create_png_with_text_chunks(&[("Comment", comment)]))
            .unwrap();
    assert_eq!(metadata.scale, None);
    assert_eq!(metadata.cfg_rescale, None);
    assert_eq!(metadata.vibe_entries[0].strength, None);
    assert_eq!(
        metadata
            .warnings
            .iter()
            .filter(|warning| matches!(warning, MetadataWarning::InvalidNumericValue { .. }))
            .count(),
        3
    );
    assert!(metadata.raw_comment_json.is_some());

    let metadata = parse_png_metadata_from_bytes(&create_png_with_text_chunks(&[(
        "parameters",
        "cat\nSteps: 20, CFG scale: NaN",
    )]))
    .unwrap();
    assert_eq!(metadata.scale, None);
    assert!(matches!(
        metadata.warnings.as_slice(),
        [MetadataWarning::InvalidNumericValue {
            field: super::MetadataField::Scale,
            ..
        }]
    ));
}

#[test]
fn text_total_budget_is_checked_before_png_metadata_is_retained() {
    let bytes = create_png_with_text_chunks(&[("one", "abcd"), ("two", "efgh")]);
    let policy = crate::media_policy::ImageDecodePolicy {
        max_total_text_bytes: 7,
        ..crate::media_policy::ImageDecodePolicy::default()
    };
    let error = super::parse_png_metadata_from_bytes_with_policy(&bytes, policy).unwrap_err();
    let mut error: &(dyn std::error::Error + 'static) = &error;
    loop {
        if let Some(error) = error.downcast_ref::<crate::media_policy::ImagePolicyError>() {
            assert_eq!(error.limit, crate::media_policy::ImageLimit::TotalTextBytes);
            break;
        }
        error = error.source().expect("resource rejection keeps its source");
    }
}

#[test]
fn inspection_keeps_the_complete_failure_chain() {
    let inspected = inspect_generated_image_metadata(b"invalid", Some("image/png"));
    assert!(
        matches!(inspected, GeneratedImageMetadata::Invalid { message } if message.contains("invalid PNG signature"))
    );
    let bytes = create_png_with_text_chunks(&[("Comment", r#""{broken""#)]);
    let parsed = parse_png_metadata_from_bytes(&bytes).unwrap();
    assert!(
        matches!(parsed.warnings.as_slice(), [MetadataWarning::InvalidCommentJson { message }] if message.contains("line 1"))
    );
}

#[test]
fn compressed_ascii_text_uses_the_full_configured_budget() {
    let expected = "x".repeat(1024 * 1024 + 1);
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
    encoder.set_color(png::ColorType::Grayscale);
    encoder
        .add_ztxt_chunk("prompt".to_owned(), expected.clone())
        .unwrap();
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[0]).unwrap();
    writer.finish().unwrap();
    let metadata = parse_png_metadata_from_bytes(&bytes).unwrap();
    assert_eq!(metadata.prompt.as_deref(), Some(expected.as_str()));
    assert_eq!(metadata.warnings, []);
}

#[test]
fn metadata_debug_does_not_disclose_prompt_or_vibe_payloads() {
    let secret = "private prompt marker";
    let bytes = create_png_with_text_chunks(&[
        ("prompt", secret),
        ("NovelAI_Vibe_Encoding_Base64", "c2VjcmV0IGVuY29kaW5n"),
    ]);
    let metadata = parse_png_metadata_from_bytes(&bytes).unwrap();
    let view = format!("{metadata:?}");
    assert!(!view.contains(secret));
    assert!(!view.contains("c2VjcmV0IGVuY29kaW5n"));
    assert!(!format!("{:?}", metadata.vibe_entries).contains("c2VjcmV0IGVuY29kaW5n"));
}

#[test]
fn corrupted_post_image_text_crc_is_a_fatal_structure_error() {
    let mut bytes = create_png_with_post_image_text_chunks(&[png::text_metadata::TEXtChunk::new(
        "prompt", "cat",
    )]);
    let offset = bytes.windows(4).position(|bytes| bytes == b"tEXt").unwrap();
    bytes[offset + 4] ^= 1;
    assert!(parse_png_metadata_from_bytes(&bytes).is_err());
    {
        assert!(crate::decode_generated_images(bytes.clone()).is_err());
        assert!(crate::EncodedImage::from_bytes(bytes).is_err());
    }
}

#[test]
fn compressed_latin1_expansion_is_recoverable_and_does_not_consume_text_budget() {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
    encoder.set_color(png::ColorType::Grayscale);
    encoder
        .add_ztxt_chunk("oversize".to_owned(), "éé".to_owned())
        .unwrap();
    encoder
        .add_ztxt_chunk("prompt".to_owned(), "cat".to_owned())
        .unwrap();
    encoder
        .add_ztxt_chunk("extra".to_owned(), "x".to_owned())
        .unwrap();
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[0]).unwrap();
    writer.finish().unwrap();
    let policy = crate::ImageDecodePolicy {
        max_text_chunk_bytes: 3,
        max_total_text_bytes: 4,
        ..Default::default()
    };
    let parsed = super::parse_png_metadata_from_bytes_with_policy(&bytes, policy).unwrap();
    assert_eq!(parsed.prompt.as_deref(), Some("cat"));
    assert_eq!(parsed.text_chunks["extra"], ["x"]);
    assert!(!parsed.text_chunks.contains_key("oversize"));
    assert!(
        matches!(parsed.warnings.as_slice(), [MetadataWarning::InvalidTextChunk { keyword, .. }] if keyword == "oversize")
    );
}

#[test]
fn uncompressed_itxt_budget_counts_text_without_header_fields() {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
    encoder.set_color(png::ColorType::Grayscale);
    encoder
        .add_itxt_chunk("prompt".to_owned(), "a".to_owned())
        .unwrap();
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[0]).unwrap();
    writer.finish().unwrap();
    let policy = crate::ImageDecodePolicy {
        max_text_chunk_bytes: 1,
        max_total_text_bytes: 1,
        ..Default::default()
    };
    let parsed = super::parse_png_metadata_from_bytes_with_policy(&bytes, policy).unwrap();
    assert_eq!(parsed.prompt.as_deref(), Some("a"));
}
