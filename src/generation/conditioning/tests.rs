use crate::{Model, QualityPreset, UcPreset};
use crate::generation::GenerateImageRequest;

#[test]
fn conditioning_assembles_every_selected_source_in_order() {
    let plan = GenerateImageRequest::builder("cat", Model::NaiDiffusion5Full)
        .negative_prompt("avoid").furry_mode(true).transparent_background(true)
        .quality(QualityPreset::Light).uc_preset(UcPreset::None).build().normalize().unwrap();
    assert_eq!(plan.resolved_prompts().prompt, "fur dataset, cat, transparent background, very aesthetic, amazing quality, no text");
    assert_eq!(plan.resolved_prompts().negative_prompt, "avoid");
}

#[test]
fn uc_preset_precedes_user_text_and_none_preserves_it() {
    let input = GenerateImageRequest::builder("cat", Model::NaiDiffusion5Full).negative_prompt("avoid").build();
    let preset = input.clone().normalize().unwrap().resolved_prompts().negative_prompt.clone();
    assert!(preset.ends_with(", avoid"));
    assert!(preset.starts_with("nsfw, lowres,"));
    let mut without = input;
    without.core.uc_preset = UcPreset::None;
    assert_eq!(without.resolve_prompts().unwrap().negative_prompt, "avoid");
}

#[test]
fn builders_follow_selected_model_defaults() {
    for model in Model::ALL {
        let input = GenerateImageRequest::builder("cat", model).build();
        assert_eq!(input.sampling, model.descriptor().defaults().sampling);
        assert!(input.normalize().is_ok());
    }
}

#[test]
fn quality_and_uc_presets_have_one_effective_serialized_meaning() {
    assert_eq!(UcPreset::default(), UcPreset::Light);
    assert!(serde_json::from_str::<UcPreset>("\"strong\"").is_err());
    for (preset, expected) in [
        (QualityPreset::Standard, "cat, very aesthetic, masterpiece, no text"),
        (QualityPreset::Light, "cat, very aesthetic, amazing quality, no text"),
        (QualityPreset::None, "cat"),
    ] {
        let input = GenerateImageRequest::builder("cat", Model::NaiDiffusion5Full).quality(preset).build();
        assert_eq!(input.resolve_prompts().unwrap().prompt, expected);
    }
}
