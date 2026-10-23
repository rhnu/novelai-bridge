use crate::{
    BridgeError, Character, CharacterField, CharacterPosition, GenerateImageRequest,
    GenerationDelivery, GenerationField, GuidanceField, ImageSize, ImageToImageInput,
    InpaintingInput, InvalidRequest, Model, NoiseSchedule, Sampler, UcPreset, ValidationPolicy,
    generation::test_support::image,
};

fn wire(request: GenerateImageRequest) -> serde_json::Value {
    serde_json::from_slice(
        &request
            .normalize()
            .unwrap()
            .prepare()
            .unwrap()
            .encode(GenerationDelivery::Images)
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn curated_inpainting_resolves_the_entire_v45_request_before_preparing_a_stream() {
    let plan = GenerateImageRequest::builder("cat", Model::NaiDiffusion5Curated)
        .negative_prompt("custom-negative")
        .transparent_background(true)
        .image_format(crate::ImageFormat::Webp)
        .size(ImageSize {
            width: 64,
            height: 64,
        })
        .inpainting(InpaintingInput::new(image(), image()))
        .seed(42)
        .build()
        .normalize()
        .unwrap();
    assert_eq!(plan.request().core.model, Model::NaiDiffusion45Curated);
    assert_eq!(plan.model_id(), "nai-diffusion-4-5-curated-inpainting");
    assert!(!plan.request().core.transparent_background);
    assert!(plan.resolved_prompts().prompt.contains("rating:general"));
    assert!(
        !plan
            .resolved_prompts()
            .prompt
            .contains("transparent background")
    );
    assert!(
        plan.resolved_prompts()
            .negative_prompt
            .starts_with("blurry, lowres, upscaled")
    );
    assert!(
        plan.resolved_prompts()
            .negative_prompt
            .ends_with("custom-negative")
    );
    assert_eq!(
        plan.anlas_estimate_input(crate::AnlasPricingContext::default())
            .model,
        Model::NaiDiffusion45Curated
    );
    assert!(
        plan.diagnostics()
            .iter()
            .any(|entry| entry.field == GenerationField::Model
                && matches!(entry.change, crate::NormalizationChange::Fallback))
    );
    assert!(plan.diagnostics().iter().any(|entry| entry.field
        == GenerationField::TransparentBackground
        && matches!(entry.change, crate::NormalizationChange::Unsupported)));
    let body: serde_json::Value = serde_json::from_slice(
        &plan
            .prepare()
            .unwrap()
            .encode(GenerationDelivery::Stream(crate::StreamMode::Sse))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["model"], "nai-diffusion-4-5-curated-inpainting");
    assert_eq!(body["parameters"]["image_format"], "webp");
    assert_eq!(body["parameters"]["stream"], "sse");
    assert!(body["parameters"].get("straight_alpha").is_none());
    assert!(
        body["parameters"]
            .get("tag_hint_transparent_background")
            .is_none()
    );
    assert!(body["parameters"].get("legacy_uc").is_none());
}

#[test]
fn curated_inpainting_rejects_an_implicit_model_change_under_strict_policy() {
    let request = GenerateImageRequest::builder("cat", Model::NaiDiffusion5Curated)
        .inpainting(InpaintingInput::new(image(), image()))
        .build();
    assert!(matches!(request.normalize_with(ValidationPolicy::Reject),
        Err(BridgeError::InvalidRequest(InvalidRequest::UnsupportedModelFeature { feature, .. }))
        if feature == GenerationField::Model.into()));
    assert_eq!(GenerationField::Model.to_string(), "core.model");
    for model in [
        Model::NaiDiffusion5Curated,
        Model::NaiDiffusion5Full,
        Model::NaiDiffusion5FullMedium,
    ] {
        let plan = GenerateImageRequest::builder("cat", model)
            .build()
            .normalize()
            .unwrap();
        assert_eq!(plan.request().core.model, model);
    }
}

#[test]
fn medium_fixed_values_and_negative_prompts_are_normalized_before_encoding() {
    let input = GenerateImageRequest::builder("cat", Model::NaiDiffusion5FullMedium)
        .steps(25)
        .sampler(Sampler::KEuler)
        .uc_preset(UcPreset::Light)
        .negative_prompt("custom-base")
        .characters(vec![Character {
            prompt: "character".to_owned(),
            negative_prompt: Some("custom-character".to_owned()),
            position: CharacterPosition::default(),
            enabled: true,
        }])
        .build();
    let plan = input.normalize().unwrap();
    assert_eq!(plan.request().sampling.steps, 14);
    assert_eq!(plan.request().sampling.sampler, Sampler::KEulerAncestral);
    assert_eq!(plan.request().core.uc_preset, UcPreset::Heavy);
    assert!(plan.request().core.negative_prompt.is_none());
    assert!(
        plan.request().characters.as_ref().unwrap().characters[0]
            .negative_prompt
            .is_none()
    );
    let json: serde_json::Value = serde_json::from_slice(
        &plan
            .prepare()
            .unwrap()
            .encode(GenerationDelivery::Images)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["parameters"]["steps"], 14);
    assert_eq!(json["parameters"]["sampler"], "k_euler_ancestral");
    assert_eq!(json["parameters"]["tag_hint_uc_preset"], 2);
    assert!(json["parameters"].get("cfg_rescale").is_none());
    assert!(
        !json["parameters"]["negative_prompt"]
            .as_str()
            .unwrap()
            .contains("custom-base")
    );
    assert_eq!(json["parameters"]["characterPrompts"][0]["uc"], "");
    assert_eq!(
        json["parameters"]["v4_negative_prompt"]["caption"]["char_captions"][0]["char_caption"],
        ""
    );
}

#[test]
fn medium_rejects_each_fixed_setting_and_reports_its_exact_field() {
    let base = GenerateImageRequest::builder("cat", Model::NaiDiffusion5FullMedium).build();
    let mut steps = base.clone();
    steps.sampling.steps = 15;
    let mut sampler = base.clone();
    sampler.sampling.sampler = Sampler::KEuler;
    let mut uc = base.clone();
    uc.core.uc_preset = UcPreset::None;
    let mut rescale = base.clone();
    rescale.sampling.cfg_rescale = 0.5;
    let mut negative = base;
    negative.core.negative_prompt = Some("custom".to_owned());
    for (request, expected) in [
        (steps, GenerationField::Steps),
        (sampler, GenerationField::Sampler),
        (uc, GenerationField::UcPreset),
        (rescale, GenerationField::CfgRescale),
        (negative, GenerationField::NegativePrompt),
    ] {
        let error = request
            .normalize_with(ValidationPolicy::Reject)
            .unwrap_err();
        assert!(match error {
            BridgeError::InvalidRequest(InvalidRequest::NumericOutOfRange { field, .. }) =>
                field == expected.into(),
            BridgeError::InvalidRequest(InvalidRequest::UnsupportedModelFeature {
                feature,
                ..
            }) => feature == expected.into(),
            _ => false,
        });
    }
    let request = GenerateImageRequest::builder("cat", Model::NaiDiffusion5FullMedium)
        .characters(vec![Character {
            prompt: "character".to_owned(),
            negative_prompt: Some("custom".to_owned()),
            position: CharacterPosition::default(),
            enabled: true,
        }])
        .build();
    assert!(matches!(request.normalize_with(ValidationPolicy::Reject),
        Err(BridgeError::InvalidRequest(InvalidRequest::UnsupportedModelFeature { feature, .. }))
        if feature == GenerationField::Character { index: 0, field: CharacterField::NegativePrompt }.into()));
}

#[test]
fn sampler_replacements_follow_model_and_operation_without_disabling_other_explicit_choices() {
    for (model, inpainting, selected) in [
        (Model::NaiDiffusion3, true, Sampler::Ddim),
        (Model::NaiDiffusion3, true, Sampler::DdimV3),
        (Model::NaiDiffusion4Full, false, Sampler::Ddim),
        (Model::NaiDiffusion5Full, false, Sampler::Ddim),
    ] {
        let mut input = GenerateImageRequest::builder("cat", model)
            .sampler(selected)
            .build();
        if inpainting {
            input.operation =
                crate::GenerationInput::Inpainting(InpaintingInput::new(image(), image()));
        }
        assert!(
            matches!(input.clone().normalize_with(ValidationPolicy::Reject),
            Err(BridgeError::InvalidRequest(InvalidRequest::UnsupportedModelFeature { feature, .. }))
            if feature == GenerationField::Sampler.into())
        );
        assert_eq!(
            input.normalize().unwrap().request().sampling.sampler,
            Sampler::KEulerAncestral
        );
    }
    let accepted = GenerateImageRequest::builder("cat", Model::NaiDiffusion45Full)
        .sampler(Sampler::KDpmpp2mSde)
        .build()
        .normalize_with(ValidationPolicy::Reject)
        .unwrap();
    assert_eq!(accepted.request().sampling.sampler, Sampler::KDpmpp2mSde);
}

#[test]
fn applicable_noise_schedule_and_wire_fields_share_the_model_sampler_rules() {
    let fixed = GenerateImageRequest::builder("cat", Model::NaiDiffusion5Full)
        .noise_schedule(NoiseSchedule::Exponential)
        .build();
    assert!(
        fixed
            .clone()
            .normalize_with(ValidationPolicy::Reject)
            .is_err()
    );
    assert_eq!(wire(fixed)["parameters"]["noise_schedule"], "karras");
    let defaulted = GenerateImageRequest::builder("cat", Model::NaiDiffusion3)
        .sampler(Sampler::KDpm2)
        .build();
    assert_eq!(
        wire(defaulted)["parameters"]["noise_schedule"],
        "exponential"
    );
    let ddim = GenerateImageRequest::builder("cat", Model::NaiDiffusion3)
        .sampler(Sampler::Ddim)
        .noise_schedule(NoiseSchedule::Native)
        .build();
    assert!(
        ddim.clone()
            .normalize_with(ValidationPolicy::Reject)
            .is_ok()
    );
    assert!(wire(ddim)["parameters"].get("noise_schedule").is_none());
}

#[test]
fn image_inputs_and_incompatible_samplers_disable_smea_with_typed_diagnostics() {
    for with_image in [false, true] {
        let mut input = GenerateImageRequest::builder("cat", Model::NaiDiffusion3).build();
        if with_image {
            input.operation =
                crate::GenerationInput::ImageToImage(ImageToImageInput::new(image(), 0.5, 0.0));
        } else {
            input.sampling.sampler = Sampler::Ddim;
            input.sampling.noise_schedule = NoiseSchedule::Native;
        }
        input.guidance.smea = true;
        input.guidance.smea_dyn = true;
        assert!(
            input
                .clone()
                .normalize_with(ValidationPolicy::Reject)
                .is_err()
        );
        let plan = input.normalize().unwrap();
        assert!(!plan.request().guidance.smea);
        assert!(!plan.request().guidance.smea_dyn);
        assert!(
            plan.diagnostics()
                .iter()
                .any(|entry| entry.field == GenerationField::Guidance(GuidanceField::Smea))
        );
    }
    let mut input = GenerateImageRequest::builder("cat", Model::NaiDiffusion3).build();
    input.guidance.smea_dyn = true;
    let plan = input.normalize().unwrap();
    assert!(
        plan.diagnostics()
            .iter()
            .any(|entry| entry.field == GenerationField::Guidance(GuidanceField::SmeaDyn))
    );
}

#[test]
fn sample_limits_use_the_normalized_canvas_and_are_shared_with_quotes_and_payloads() {
    for (width, height, requested, expected) in [
        (704, 512, 8, 8),
        (640, 640, 8, 6),
        (832, 512, 8, 4),
        (720, 512, 8, 8),
        (736, 512, 8, 6),
    ] {
        let input = GenerateImageRequest::builder("cat", Model::NaiDiffusion3)
            .size(ImageSize { width, height })
            .n_samples(requested)
            .seed(42)
            .build();
        let plan = input.normalize().unwrap();
        assert_eq!(plan.request().core.n_samples, expected);
        let quote = plan.estimate_anlas_cost(crate::AnlasPricingContext::default());
        assert_eq!(quote.sample_limit, expected);
        assert_eq!(quote.priced_samples, expected);
        let payload: serde_json::Value = serde_json::from_slice(
            &plan
                .prepare()
                .unwrap()
                .encode(GenerationDelivery::Images)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(payload["parameters"]["n_samples"], expected);
    }
    assert!(
        GenerateImageRequest::builder("cat", Model::NaiDiffusion3)
            .size(ImageSize {
                width: 640,
                height: 640
            })
            .n_samples(8)
            .build()
            .normalize_with(ValidationPolicy::Reject)
            .is_err()
    );
}

#[test]
fn known_ddim_step_failures_fall_back_without_changing_steps_or_accepting_invalid_strict_requests()
{
    for sampler in [Sampler::Ddim, Sampler::DdimV3] {
        for steps in [3, 9, 27, 36, 37] {
            let request = GenerateImageRequest::builder("cat", Model::NaiDiffusion3)
                .sampler(sampler)
                .steps(steps)
                .noise_schedule(NoiseSchedule::Native)
                .build();
            assert!(
                matches!(request.clone().normalize_with(ValidationPolicy::Reject),
                Err(BridgeError::InvalidRequest(InvalidRequest::UnsupportedModelFeature { feature, .. }))
                if feature == GenerationField::Sampler.into())
            );
            let plan = request.normalize().unwrap();
            assert_eq!(plan.request().sampling.sampler, Sampler::KEulerAncestral);
            assert_eq!(plan.request().sampling.steps, steps);
            assert_eq!(
                plan.anlas_estimate_input(crate::AnlasPricingContext::default())
                    .steps,
                steps
            );
            assert!(
                plan.diagnostics()
                    .iter()
                    .any(|entry| entry.field == GenerationField::Sampler)
            );
        }
        for steps in [2, 4, 8, 10, 26, 28, 35, 38] {
            let request = GenerateImageRequest::builder("cat", Model::NaiDiffusion3)
                .sampler(sampler)
                .steps(steps)
                .noise_schedule(NoiseSchedule::Native)
                .build();
            let plan = request.normalize_with(ValidationPolicy::Reject).unwrap();
            assert_eq!(plan.request().sampling.sampler, sampler);
            assert_eq!(plan.request().sampling.steps, steps);
        }
    }
}
