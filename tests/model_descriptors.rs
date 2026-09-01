#[path = "model_descriptors/references.rs"]
mod references;
mod support;

use novelai_bridge::{
    GenerateImageRequest, GenerationCore, GenerationMode, ImageTokenizerKind, Model, NoiseSchedule,
    NoiseScheduleSupport, NormalizationPolicy, OpusAllowance, QualityPreset, SamplingParams,
    WireProfile,
};

const MODES: [GenerationMode; 3] = [
    GenerationMode::TextToImage,
    GenerationMode::ImageToImage,
    GenerationMode::Inpainting,
];

#[tokio::test]
async fn default_model_and_generation_use_v5_full_profiles() {
    use novelai_bridge::{AnlasEstimateInput, AnlasPricingContext, Client};

    let model = Model::NaiDiffusion5Full;
    assert_eq!(Model::default(), model);
    let defaults = model.descriptor().defaults();
    let raw = GenerateImageRequest::default();
    assert_eq!(raw.core, GenerationCore::default());
    assert_eq!(raw.core.model, model);
    assert_eq!(raw.sampling, SamplingParams::default());
    assert_eq!(raw.sampling, defaults.sampling);
    assert!((raw.sampling.scale - 7.0).abs() < f32::EPSILON);
    assert_eq!(AnlasEstimateInput::default().model, model);

    let request = GenerateImageRequest::builder("cat", Model::default()).build();
    assert_eq!(request.sampling, raw.sampling);
    let estimate = request
        .anlas_estimate_input(AnlasPricingContext::default())
        .unwrap();
    assert_eq!(estimate, AnlasEstimateInput::default());

    let transport = support::MockTransport::default();
    Client::with_transport(transport.clone())
        .generate(request)
        .await
        .unwrap();
    let body = transport.last_body().unwrap();
    assert_eq!(body["model"], "nai-diffusion-5-full");
    assert_eq!(body["parameters"]["params_version"], 4);
    assert_eq!(body["parameters"]["scale"], 7.0);
}

#[test]
fn registry_identity_is_unique_and_agrees_with_serde_and_sibling_queries() {
    let ids: std::collections::BTreeSet<_> = Model::ALL.iter().map(Model::as_str).collect();
    assert_eq!(ids.len(), 8);
    for model in Model::ALL {
        let descriptor = model.descriptor();
        assert_eq!(descriptor.identity().model, model);
        assert_eq!(
            serde_json::to_value(model).unwrap(),
            descriptor.identity().api_id
        );
        assert_eq!(
            serde_json::from_value::<Model>(serde_json::json!(model.as_str())).unwrap(),
            model
        );
        let sibling = format!("{}-inpainting", model.as_str());
        assert_eq!(model.inpainting_id(), Some(sibling.as_str()));
        assert_eq!(descriptor.support().inpainting.unwrap().model_id, sibling);
        for mode in MODES {
            assert_eq!(
                descriptor.model_id_for(mode),
                if mode == GenerationMode::Inpainting {
                    model.inpainting_id()
                } else {
                    Some(model.as_str())
                }
            );
        }
    }
}

#[test]
#[allow(clippy::too_many_lines)] // Keep the explicit registry fixture beside its assertions.
fn every_variant_has_the_expected_independent_profiles() {
    let expected = [
        (
            Model::NaiDiffusion3,
            5.0,
            ImageTokenizerKind::Clip,
            225,
            WireProfile::LegacyV3,
            None,
            Some("v3"),
        ),
        (
            Model::NaiDiffusion3Furry,
            6.2,
            ImageTokenizerKind::Clip,
            225,
            WireProfile::LegacyV3,
            None,
            Some("v3furry"),
        ),
        (
            Model::NaiDiffusion4Full,
            5.5,
            ImageTokenizerKind::T5,
            512,
            WireProfile::StructuredV4,
            Some(19.0),
            Some("v4full"),
        ),
        (
            Model::NaiDiffusion4Curated,
            5.5,
            ImageTokenizerKind::T5,
            512,
            WireProfile::StructuredV4,
            Some(19.0),
            Some("v4curated"),
        ),
        (
            Model::NaiDiffusion45Full,
            5.0,
            ImageTokenizerKind::T5,
            512,
            WireProfile::StructuredV4,
            Some(58.0),
            Some("v4-5full"),
        ),
        (
            Model::NaiDiffusion45Curated,
            5.0,
            ImageTokenizerKind::T5,
            512,
            WireProfile::StructuredV4,
            Some(58.0),
            Some("v4-5curated"),
        ),
        (
            Model::NaiDiffusion5Full,
            7.0,
            ImageTokenizerKind::Qwen,
            1471,
            WireProfile::StructuredV5,
            None,
            None,
        ),
        (
            Model::NaiDiffusion5Curated,
            7.0,
            ImageTokenizerKind::Qwen,
            703,
            WireProfile::StructuredV5,
            None,
            None,
        ),
    ];
    assert_eq!(expected.len(), Model::ALL.len());
    for (model, scale, tokenizer, limit, wire, sigma, key) in expected {
        let descriptor = model.descriptor();
        assert!((descriptor.defaults().sampling.scale - scale).abs() < f32::EPSILON);
        assert_eq!(descriptor.defaults().sampling.steps, 23);
        assert_eq!(descriptor.prompt().tokenizer, tokenizer);
        assert_eq!(descriptor.prompt().token_limit, limit);
        assert_eq!(*descriptor.wire(), wire);
        assert_eq!(descriptor.sampling().variety_sigma(832, 1216), sigma);
        assert_eq!(model.vibe_model_key(), key);
        let legacy = wire == WireProfile::LegacyV3;
        let v5 = wire == WireProfile::StructuredV5;
        assert_eq!(wire.params_version(), if v5 { 4 } else { 3 });
        assert_eq!(
            descriptor.prompt().max_characters(),
            if legacy { 0 } else { 6 }
        );
        assert_eq!(descriptor.sampling().smea, legacy);
        assert_eq!(descriptor.sampling().dynamic_thresholding, legacy);
        assert_eq!(descriptor.support().streaming, !legacy);
        assert_eq!(
            descriptor.prompt().quality.supports(QualityPreset::Light),
            v5
        );
        assert_eq!(descriptor.prompt().transparent_background, v5);
        assert!(
            (descriptor.pricing().multiplier - if v5 { 1.5 } else { 1.0 }).abs() < f64::EPSILON
        );
        assert_eq!(
            descriptor.pricing().opus_allowance,
            if v5 {
                OpusAllowance::Metered
            } else {
                OpusAllowance::Standard
            }
        );
    }
}

#[test]
fn builders_use_descriptor_defaults_and_defaults_satisfy_all_constraints() {
    for model in Model::ALL {
        let descriptor = model.descriptor();
        let defaults = descriptor.defaults();
        let limits = descriptor.constraints();
        let raw = GenerateImageRequest::builder("cat", model).build();
        assert_eq!(raw.core, GenerationCore::new("cat", model));
        assert_eq!(raw.sampling, SamplingParams::for_model(model));
        assert_eq!(raw.sampling, defaults.sampling);
        assert_eq!(raw.core.size, defaults.size);
        assert_eq!(raw.core.n_samples, defaults.n_samples);
        assert_eq!(raw.core.quality, defaults.quality);
        assert_eq!(raw.core.uc_preset, defaults.uc_preset);
        assert!((limits.steps.min..=limits.steps.max).contains(&raw.sampling.steps));
        assert!((limits.scale.min..=limits.scale.max).contains(&raw.sampling.scale));
        assert!(
            (limits.cfg_rescale.min..=limits.cfg_rescale.max).contains(&raw.sampling.cfg_rescale)
        );
        assert!((limits.samples.min..=limits.samples.max).contains(&raw.core.n_samples));
        for size in [defaults.size.width, defaults.size.height] {
            assert!((limits.canvas.dimension.min..=limits.canvas.dimension.max).contains(&size));
            assert!(size.is_multiple_of(limits.canvas.multiple));
        }
        assert!(defaults.size.pixels() <= limits.canvas.max_pixels);
        assert!(
            descriptor
                .prompt()
                .characters
                .is_none_or(|profile| profile.max_characters > 0)
        );
        let normalized = raw.normalize_with(NormalizationPolicy::Reject).unwrap();
        assert!(normalized.diagnostics().is_empty());

        let mut overflow = GenerateImageRequest::builder("cat", model).build();
        overflow.core.size.width = u32::MAX;
        overflow.core.n_samples = u32::MAX;
        overflow.sampling.steps = 0;
        let effective = overflow.normalize().unwrap();
        assert_eq!(
            effective.request().core.size.width,
            limits.canvas.dimension.max
        );
        assert_eq!(effective.request().core.n_samples, limits.samples.max);
        assert_eq!(effective.request().sampling.steps, limits.steps.min);
    }
}

#[test]
fn schedule_support_is_independent_of_wire_format_and_fallback_is_idempotent() {
    let schedules = [
        NoiseSchedule::Native,
        NoiseSchedule::Karras,
        NoiseSchedule::Exponential,
        NoiseSchedule::Polyexponential,
    ];
    for model in Model::ALL {
        let descriptor = model.descriptor();
        for schedule in schedules {
            let expected = if *descriptor.wire() != WireProfile::LegacyV3
                && schedule == NoiseSchedule::Native
            {
                NoiseSchedule::Karras
            } else {
                schedule
            };
            let sampling = descriptor.sampling();
            assert_eq!(
                sampling.noise_schedules.supports(schedule),
                expected == schedule
            );
            assert_eq!(sampling.resolve_noise_schedule(schedule), expected);
            assert_eq!(sampling.resolve_noise_schedule(expected), expected);
            for policy in [NormalizationPolicy::Reject, NormalizationPolicy::Normalize] {
                let mut raw = GenerateImageRequest::builder("cat", model).build();
                raw.sampling.noise_schedule = schedule;
                let effective = raw.normalize_with(policy).unwrap();
                assert_eq!(effective.request().sampling.noise_schedule, expected);
                assert_eq!(effective.diagnostics().is_empty(), schedule == expected);
            }
        }
    }
    // Detached sampling profiles can change without deriving anything from wire/prompt versions.
    let descriptor = Model::NaiDiffusion5Full.descriptor();
    let mut detached = *descriptor.sampling();
    detached.noise_schedules = NoiseScheduleSupport::NativeAndExplicit;
    assert_eq!(
        detached.resolve_noise_schedule(NoiseSchedule::Native),
        NoiseSchedule::Native
    );
    assert_eq!(
        descriptor
            .sampling()
            .resolve_noise_schedule(NoiseSchedule::Native),
        NoiseSchedule::Karras
    );
}

#[tokio::test]
async fn every_model_and_mode_uses_its_registered_wire_identity() {
    use novelai_bridge::{
        Client, EncodedImage, GenerateImageStreamRequest, ImageSize, Img2ImgRequest,
    };
    let image = EncodedImage::from_base64(&support::create_png_base64(2, 2)).unwrap();
    for model in Model::ALL {
        let descriptor = model.descriptor();
        for mode in MODES {
            let mut raw = GenerateImageRequest::builder("cat", model)
                .size(ImageSize {
                    width: 64,
                    height: 64,
                })
                .seed(42)
                .strict_mode(true)
                .build();
            if mode != GenerationMode::TextToImage {
                let mut input = Img2ImgRequest::new(image.clone(), 0.5, 0.0);
                if mode == GenerationMode::Inpainting {
                    input.mask = Some(image.clone());
                }
                raw.img2img = Some(input);
            }
            assert_eq!(raw.mode(), mode);
            for streaming in [false, true] {
                let transport = support::MockTransport::default();
                let client = Client::with_transport(transport.clone());
                if streaming {
                    let result = client
                        .generate_stream(GenerateImageStreamRequest {
                            base: raw.clone(),
                            ..Default::default()
                        })
                        .await;
                    if !descriptor.support().streaming {
                        assert!(result.is_err());
                        assert!(transport.last_body().is_none());
                        continue;
                    }
                    let _stream = result.unwrap();
                } else {
                    client.generate(raw.clone()).await.unwrap();
                }
                let body = transport.last_body().unwrap();
                assert_eq!(body["model"], descriptor.model_id_for(mode).unwrap());
                let action = match mode {
                    GenerationMode::TextToImage => "generate",
                    GenerationMode::ImageToImage => "img2img",
                    GenerationMode::Inpainting => "infill",
                };
                assert_eq!(body["action"], action);
                assert_eq!(
                    body["parameters"]["params_version"],
                    descriptor.wire().params_version()
                );
                let parameters = body["parameters"].as_object().unwrap();
                assert_eq!(
                    parameters.contains_key("v4_prompt"),
                    descriptor.wire().uses_structured_prompt()
                );
                assert_eq!(
                    parameters.contains_key("prompt"),
                    !descriptor.wire().uses_structured_prompt()
                );
                assert_eq!(
                    parameters.contains_key("legacy_uc"),
                    descriptor.wire().uses_v5_extensions()
                );
                assert_eq!(parameters.contains_key("stream"), streaming);
            }
        }
    }
}
