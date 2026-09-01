#![cfg(feature = "png-metadata")]

use base64::{Engine, engine::general_purpose::STANDARD};
use novelai_bridge::{
    GeneratedImageMetadata, PngMetadataWarning, VibeMetadataSource,
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
        GeneratedImageMetadata::UnsupportedFormat
    ));
}

#[test]
fn malformed_comment_is_recoverable_and_diagnostic() {
    let png =
        create_png_with_text_chunks(&[("Comment", "{not-json"), ("prompt", "fallback prompt")]);
    let metadata = parse_png_metadata_from_bytes(&png).expect("valid png");
    assert_eq!(metadata.prompt.as_deref(), Some("fallback prompt"));
    assert_eq!(
        metadata.warnings,
        vec![PngMetadataWarning::InvalidCommentJson]
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
        [PngMetadataWarning::InvalidTextChunk { keyword, .. }] if keyword == "Comment"
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
    assert_eq!(metadata.character_prompts, vec!["alice".to_owned()]);
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
                    "v4-5full": { "encoding": "R0hJSktMTU5PUFFSU1RVVldYWVo=" }
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
        naidata.model_encodings.get("v4-5full"),
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
