use super::{
    PngMetadata, PngMetadataWarning,
    png::first_chunk_value_ci,
    value::{
        extract_first_f32, extract_non_empty_string, extract_v4_character_prompts,
        numeric_diagnostic, parse_json_payload_result, value_to_bool, value_to_f32, value_to_i32,
        value_to_i64, value_to_u32,
    },
    vibe::push_reference_vibe_entries,
};
const KEY_COMMENT: &str = "Comment";
const SCALE_KEYS: &[&str] = &[
    "scale",
    "cfg_scale",
    "cfg",
    "guidance",
    "prompt_guidance",
    "cfgScale",
];

pub(super) fn parse_comment_payload(parsed: &mut PngMetadata) {
    let Some(comment) = first_chunk_value_ci(&parsed.text_chunks, KEY_COMMENT) else {
        return;
    };
    let payload = parse_json_payload_result(comment);
    let (payload, failure) = match payload {
        Ok(Some(payload)) => (Some(payload), None),
        Ok(None) => (
            None,
            Some("expected a JSON object or an encoded JSON object".to_owned()),
        ),
        Err(error) => (None, Some(super::format_error_chain(&error))),
    };
    let Some(payload) = payload else {
        if !comment.trim().is_empty() {
            parsed
                .warnings
                .push(PngMetadataWarning::InvalidCommentJson {
                    message: failure.expect("an absent payload has an interpretation diagnostic"),
                });
        }
        return;
    };
    let Some(object) = payload.as_object() else {
        return;
    };

    for &key in SCALE_KEYS {
        if let Some(value) = object.get(key) {
            numeric_diagnostic(value, super::MetadataField::Scale, &mut parsed.warnings);
        }
    }
    if let Some(value) = object.get("cfg_rescale") {
        numeric_diagnostic(
            value,
            super::MetadataField::CfgRescale,
            &mut parsed.warnings,
        );
    }
    parsed.prompt = extract_non_empty_string(object, &["prompt"]).or_else(|| parsed.prompt.take());
    parsed.negative_prompt = extract_non_empty_string(object, &["uc", "negative_prompt"])
        .or_else(|| parsed.negative_prompt.take());
    parsed.seed = object
        .get("seed")
        .and_then(value_to_i64)
        .or_else(|| parsed.seed.take());
    parsed.sampler =
        extract_non_empty_string(object, &["sampler"]).or_else(|| parsed.sampler.take());
    parsed.steps = object
        .get("steps")
        .and_then(value_to_u32)
        .or_else(|| parsed.steps.take());
    parsed.scale = extract_first_f32(object, SCALE_KEYS).or_else(|| parsed.scale.take());
    parsed.width = object
        .get("width")
        .and_then(value_to_u32)
        .or_else(|| parsed.width.take());
    parsed.height = object
        .get("height")
        .and_then(value_to_u32)
        .or_else(|| parsed.height.take());
    parsed.model = extract_non_empty_string(object, &["model"]).or_else(|| parsed.model.take());
    parsed.noise_schedule = extract_non_empty_string(object, &["noise_schedule"])
        .or_else(|| parsed.noise_schedule.take());
    parsed.cfg_rescale = object
        .get("cfg_rescale")
        .and_then(value_to_f32)
        .or_else(|| parsed.cfg_rescale.take());
    parsed.uc_preset = object
        .get("uc_preset")
        .or_else(|| object.get("ucPreset"))
        .and_then(value_to_i32)
        .or_else(|| parsed.uc_preset.take());
    parsed.quality_toggle = object
        .get("quality_toggle")
        .or_else(|| object.get("qualityToggle"))
        .and_then(value_to_bool)
        .or_else(|| parsed.quality_toggle.take());

    let positive = extract_v4_character_prompts(object, "v4_prompt");
    let negative = extract_v4_character_prompts(object, "v4_negative_prompt");
    parsed.characters = (0..positive.len().max(negative.len()))
        .map(|source_index| super::CharacterMetadata {
            source_index,
            prompt: positive.get(source_index).cloned().flatten(),
            negative_prompt: negative.get(source_index).cloned().flatten(),
        })
        .collect();

    push_reference_vibe_entries(object, &mut parsed.vibe_entries, &mut parsed.warnings);
    parsed.raw_comment_json = Some(payload);
}
