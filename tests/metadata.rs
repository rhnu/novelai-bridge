use base64::{Engine, engine::general_purpose::STANDARD};
use novelai_bridge::{
    VibeMetadataSource, extract_embedded_vibe_document_from_png_bytes,
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
    assert_eq!(metadata.character_prompts, vec!["alice".to_owned()]);
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
