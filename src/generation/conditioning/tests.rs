use crate::{Model, QualityPreset, UcPreset, generation::GenerateImageRequest};

#[test]
fn conditioning_assembles_every_selected_source_in_order() {
    let plan = GenerateImageRequest::builder("cat", Model::NaiDiffusion5Full)
        .negative_prompt("avoid")
        .furry_mode(true)
        .transparent_background(true)
        .quality(QualityPreset::Light)
        .uc_preset(UcPreset::None)
        .build()
        .normalize()
        .unwrap();
    assert_eq!(
        plan.resolved_prompts().prompt,
        "fur dataset, cat, transparent background, very aesthetic, amazing quality, no text"
    );
    assert_eq!(plan.resolved_prompts().negative_prompt, "avoid");
}

#[test]
fn uc_preset_precedes_user_text_and_none_preserves_it() {
    let input = GenerateImageRequest::builder("cat", Model::NaiDiffusion5Full)
        .negative_prompt("avoid")
        .build();
    let preset = input
        .clone()
        .normalize()
        .unwrap()
        .resolved_prompts()
        .negative_prompt
        .clone();
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
    assert_eq!(UcPreset::default(), UcPreset::Heavy);
    assert!(serde_json::from_str::<UcPreset>("\"strong\"").is_err());
    for (preset, expected) in [
        (
            QualityPreset::Standard,
            "cat, very aesthetic, masterpiece, no text",
        ),
        (
            QualityPreset::Light,
            "cat, very aesthetic, amazing quality, no text",
        ),
        (QualityPreset::None, "cat"),
    ] {
        let input = GenerateImageRequest::builder("cat", Model::NaiDiffusion5Full)
            .quality(preset)
            .build();
        assert_eq!(input.resolve_prompts().unwrap().prompt, expected);
    }
}

#[test]
fn family_presets_apply_their_own_text() {
    for (model, quality_fragment, uc_fragment) in [
        (
            Model::NaiDiffusion3,
            "best quality, amazing quality",
            "{bad}, error",
        ),
        (
            Model::NaiDiffusion3Furry,
            "{best quality}, {amazing quality}",
            "{{worst quality}}",
        ),
        (
            Model::NaiDiffusion4Full,
            "no text, best quality",
            "error, film grain",
        ),
        (
            Model::NaiDiffusion4Curated,
            "rating:general, best quality",
            "logo, dated, signature",
        ),
        (
            Model::NaiDiffusion45Full,
            "masterpiece, no text",
            "screentone",
        ),
        (
            Model::NaiDiffusion45Curated,
            "-0.8::feet::, rating:general",
            "lowres, upscaled",
        ),
    ] {
        let plan = GenerateImageRequest::builder("cat", model)
            .build()
            .normalize()
            .unwrap();
        assert!(plan.resolved_prompts().prompt.contains(quality_fragment));
        assert!(
            plan.resolved_prompts()
                .negative_prompt
                .contains(uc_fragment)
        );
    }
}

#[test]
fn v5_light_uc_differs_from_v45_and_curated_does_not_inject_nsfw() {
    let light = |model| {
        GenerateImageRequest::builder("cat", model)
            .uc_preset(UcPreset::Light)
            .build()
            .resolve_prompts()
            .unwrap()
            .negative_prompt
    };
    assert!(light(Model::NaiDiffusion5Full).ends_with("0::ai-generated::"));
    assert!(!light(Model::NaiDiffusion45Full).contains("ai-generated"));
    assert!(!light(Model::NaiDiffusion5Curated).contains("nsfw"));
    let explicit = GenerateImageRequest::builder("NSFW, cat", Model::NaiDiffusion5Full)
        .uc_preset(UcPreset::Light)
        .build()
        .resolve_prompts()
        .unwrap();
    assert!(!explicit.negative_prompt.contains("nsfw"));
}

#[test]
fn v3_none_retains_only_its_empty_negative_fallback() {
    for model in [Model::NaiDiffusion3, Model::NaiDiffusion3Furry] {
        let request = GenerateImageRequest::builder("cat", model)
            .uc_preset(UcPreset::None)
            .build();
        assert_eq!(request.resolve_prompts().unwrap().negative_prompt, "lowres");
        let mut with_negative = request;
        with_negative.core.negative_prompt = Some("avoid".to_owned());
        assert_eq!(
            with_negative.resolve_prompts().unwrap().negative_prompt,
            "avoid"
        );
    }
}

#[test]
fn unsupported_uc_selections_normalize_or_reject_with_their_field() {
    for (model, preset) in [
        (Model::NaiDiffusion3Furry, UcPreset::HumanFocus),
        (Model::NaiDiffusion4Full, UcPreset::FurryFocus),
        (Model::NaiDiffusion45Curated, UcPreset::FurryFocus),
    ] {
        let input = GenerateImageRequest::builder("cat", model)
            .uc_preset(preset)
            .build();
        let plan = input.clone().normalize().unwrap();
        assert_eq!(plan.request().core.uc_preset, UcPreset::Heavy);
        assert!(
            plan.diagnostics()
                .iter()
                .any(|entry| entry.field == crate::GenerationField::UcPreset)
        );
        assert!(matches!(
            input.normalize_with(crate::ValidationPolicy::Reject),
            Err(crate::BridgeError::InvalidRequest(
                crate::InvalidRequest::UnsupportedModelFeature {
                    feature: crate::RequestField::Generation(crate::GenerationField::UcPreset),
                    ..
                }
            ))
        ));
    }
}

#[test]
fn prepared_hints_and_input_follow_the_normalized_selection() {
    for (model, quality, uc, quality_hint, uc_hint) in [
        (
            Model::NaiDiffusion3,
            QualityPreset::Standard,
            UcPreset::HumanFocus,
            1,
            4,
        ),
        (
            Model::NaiDiffusion5Full,
            QualityPreset::Light,
            UcPreset::FurryFocus,
            3,
            5,
        ),
        (
            Model::NaiDiffusion4Curated,
            QualityPreset::None,
            UcPreset::None,
            0,
            0,
        ),
    ] {
        let plan = GenerateImageRequest::builder("cat", model)
            .quality(quality)
            .uc_preset(uc)
            .seed(42)
            .build()
            .normalize()
            .unwrap();
        let expected = plan.resolved_prompts().clone();
        let body: serde_json::Value = serde_json::from_slice(
            &plan
                .prepare()
                .unwrap()
                .encode(crate::GenerationDelivery::Images)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["input"], expected.prompt);
        assert_eq!(
            body["parameters"]["negative_prompt"],
            expected.negative_prompt
        );
        assert_eq!(body["parameters"]["tag_hint_qt"], quality_hint);
        assert_eq!(body["parameters"]["tag_hint_uc_preset"], uc_hint);
    }
}

#[test]
fn v3_variety_uses_sigma_nineteen_at_the_reference_canvas() {
    for model in [Model::NaiDiffusion3, Model::NaiDiffusion3Furry] {
        let mut input = GenerateImageRequest::builder("cat", model).seed(42).build();
        input.guidance.variety_boost = true;
        let plan = input
            .normalize_with(crate::ValidationPolicy::Reject)
            .unwrap();
        assert!(plan.request().guidance.variety_boost);
        let body: serde_json::Value = serde_json::from_slice(
            &plan
                .prepare()
                .unwrap()
                .encode(crate::GenerationDelivery::Images)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["parameters"]["skip_cfg_above_sigma"], 19.0);
    }
}
