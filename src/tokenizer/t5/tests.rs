use std::error::Error as _;

use super::*;

fn field(number: u8, data: &[u8]) -> Vec<u8> {
    assert!(data.len() < 128);
    [
        vec![number * 8 + 2, u8::try_from(data.len()).unwrap()],
        data.to_vec(),
    ]
    .concat()
}

fn piece(text: &str, score: f32, kind: u8) -> Vec<u8> {
    field(
        1,
        &[
            field(1, text.as_bytes()),
            vec![21],
            score.to_le_bytes().to_vec(),
            vec![24, kind],
        ]
        .concat(),
    )
}

fn t5_asset() -> Vec<u8> {
    [
        piece("<unk>", 0.0, 2),
        piece("</s>", 0.0, 3),
        piece("▁", -1.0, 1),
        piece("a", -1.0, 1),
        piece("b", -1.0, 1),
        piece("ab", -5.0, 1),
        piece("猫", 0.0, 4),
        piece("unused", 0.0, 5),
    ]
    .concat()
}

#[test]
fn t5_uses_viterbi_unknown_runs_user_symbols_and_normalizer_flags() {
    let bytes = t5_asset();
    let tokenizer = T5Tokenizer::from_sentencepiece_model(&bytes).unwrap();
    assert_eq!(
        tokenizer.encode("ab", FramingPolicy::ContentOnly),
        [2, 3, 4]
    );
    assert_eq!(
        tokenizer.encode("🙂🙂猫", FramingPolicy::ModelInput),
        [2, 0, 6, 1]
    );
    assert_eq!(tokenizer.encode("</s>", FramingPolicy::ContentOnly), [2, 0]);
    let mut no_prefix = bytes;
    no_prefix.extend(field(3, &[24, 0, 32, 0, 40, 0]));
    let tokenizer = T5Tokenizer::from_sentencepiece_model(&no_prefix).unwrap();
    assert_eq!(
        tokenizer.encode("a  b", FramingPolicy::ContentOnly),
        [3, 0, 4]
    );
}

#[test]
fn multibyte_pieces_remain_reachable_across_unknown_runs() {
    let mut bytes = t5_asset();
    bytes.extend(piece("é", -1.0, 1));
    let tokenizer = T5Tokenizer::from_sentencepiece_model(&bytes).unwrap();
    assert_eq!(
        tokenizer.encode("é界🙂é", FramingPolicy::ContentOnly),
        [2, 8, 0, 8]
    );
}

#[test]
fn t5_rejects_corrupt_maps_empty_pieces_nonfinite_scores_and_wrong_models() {
    for suffix in [
        field(3, &field(2, &[0, 0, 0, 0])),
        piece("", -1.0, 1),
        piece("bad", f32::NAN, 1),
        field(2, &[24, 2]),
        piece("duplicate_unknown", 0.0, 2),
        piece("a", 0.0, 1),
        piece("invalid_type", 0.0, 99),
    ] {
        let mut bytes = t5_asset();
        bytes.extend(suffix);
        assert!(T5Tokenizer::from_sentencepiece_model(&bytes).is_err());
    }
    assert!(T5Tokenizer::from_sentencepiece_model(&[0]).is_err());
    assert!(T5Tokenizer::from_sentencepiece_model(&[255; 12]).is_err());
}

#[test]
fn invalid_piece_text_retains_the_utf8_failure() {
    let bytes = field(1, &field(1, &[0xFF]));
    let error = T5Tokenizer::from_sentencepiece_model(&bytes).unwrap_err();
    assert!(matches!(
        error,
        TokenizerError::InvalidAssetData {
            context: "piece text",
            ..
        }
    ));
    assert!(error.source().unwrap().is::<std::str::Utf8Error>());
}

#[test]
fn overlapping_user_symbols_prefer_the_longest_complete_symbol() {
    let mut bytes = t5_asset();
    bytes.extend(piece("猫狗", 0.0, 4));
    bytes.extend(piece("狗", 0.0, 4));
    let tokenizer = T5Tokenizer::from_sentencepiece_model(&bytes).unwrap();
    assert_eq!(tokenizer.encode("猫狗", FramingPolicy::ContentOnly), [2, 8]);
}
