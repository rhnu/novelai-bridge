use super::*;

fn png() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder
            .add_text_chunk("Title".to_owned(), "old".to_owned())
            .unwrap();
        encoder
            .add_ztxt_chunk("comment".to_owned(), "old comment".to_owned())
            .unwrap();
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_chunk(png::chunk::ChunkType(*b"ruSt"), b"unrelated chunk")
            .unwrap();
        writer.write_image_data(&[10, 20, 30, 255]).unwrap();
        writer.finish().unwrap();
    }
    bytes
}

fn non_text_chunks(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut result = Vec::new();
    let mut position = 8;
    while position < bytes.len() {
        let length = u32::from_be_bytes(bytes[position..position + 4].try_into().unwrap()) as usize;
        let end = position + length + 12;
        if !matches!(
            &bytes[position + 4..position + 8],
            b"tEXt" | b"zTXt" | b"iTXt"
        ) {
            result.push(bytes[position..end].to_vec());
        }
        position = end;
    }
    result
}

#[test]
fn unicode_replacements_preserve_pixels_and_every_unrelated_chunk_byte() {
    let original = png();
    let result = write_png_text_chunks(
        &original,
        &[
            PngTextEntry::new("COMMENT", "新しい説明 🐈"),
            PngTextEntry::new("naidata", "{\"test\":1}"),
        ],
    )
    .unwrap();
    assert_eq!(non_text_chunks(&original), non_text_chunks(&result));
    let parsed =
        crate::metadata::png::extract_png_text_chunks(&result, ImageDecodePolicy::default(), true)
            .unwrap();
    assert_eq!(parsed.text_chunks["Title"], ["old"]);
    assert_eq!(parsed.text_chunks["COMMENT"], ["新しい説明 🐈"]);
    assert!(!parsed.text_chunks.contains_key("comment"));
    assert_eq!(parsed.pixels.unwrap().bytes, [10, 20, 30, 255]);
    assert_eq!(write_png_text_chunks(&original, &[]).unwrap(), original);
}

#[test]
fn invalid_keywords_duplicates_and_budgets_fail_without_returning_partial_output() {
    let original = png();
    for keyword in ["", " leading", "trailing ", "two  spaces", "bad\0key", "🐈"] {
        assert!(write_png_text_chunks(&original, &[PngTextEntry::new(keyword, "text")]).is_err());
    }
    assert!(
        write_png_text_chunks(
            &original,
            &[
                PngTextEntry::new("Title", "one"),
                PngTextEntry::new("TITLE", "two")
            ]
        )
        .is_err()
    );
    assert!(write_png_text_chunks(&original, &[PngTextEntry::new("Title", "null\0text")]).is_err());
    let limits = ImageDecodePolicy {
        max_text_chunk_bytes: 32,
        ..ImageDecodePolicy::default()
    };
    assert!(
        write_png_text_chunks_with_policy(
            &original,
            &[PngTextEntry::new("Title", "x".repeat(33))],
            limits
        )
        .is_err()
    );
    let limits = ImageDecodePolicy {
        max_encoded_bytes: original.len() as u64 + 1,
        ..ImageDecodePolicy::default()
    };
    assert!(
        write_png_text_chunks_with_policy(
            &original,
            &[PngTextEntry::new("Extra", "value")],
            limits
        )
        .is_err()
    );
}

#[test]
fn invalid_crc_truncated_and_trailing_data_cannot_be_hidden_by_an_edit() {
    let original = png();
    let mut trailing = original.clone();
    trailing.extend_from_slice(b"garbage");
    let mut corrupt = original.clone();
    corrupt[29] ^= 1;
    for bytes in [trailing, corrupt, original[..original.len() - 1].to_vec()] {
        assert!(write_png_text_chunks(&bytes, &[PngTextEntry::new("Title", "new")]).is_err());
    }
}
