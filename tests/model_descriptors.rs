#![cfg(all(feature = "client", feature = "streaming"))]

#[path = "model_descriptors/references.rs"]
mod references;
mod support;

use novelai_bridge::{
    CharacterPositionProfile, GenerateImageRequest, GenerationCore, GenerationMode, Model,
    NoiseSchedule, NoiseScheduleSupport, NormalizationPolicy, SamplingParams, WireProfile,
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
fn character_position_geometry_is_an_explicit_model_profile() {
    for model in [Model::NaiDiffusion5Full, Model::NaiDiffusion5Curated] {
        assert_eq!(
            model
                .descriptor()
                .prompt()
                .characters
                .expect("V5 character profile")
                .positioning,
            CharacterPositionProfile::Freeform
        );
    }
    for model in [
        Model::NaiDiffusion45Full,
        Model::NaiDiffusion45Curated,
        Model::NaiDiffusion4Full,
        Model::NaiDiffusion4Curated,
    ] {
        assert_eq!(
            model
                .descriptor()
                .prompt()
                .characters
                .expect("V4 character profile")
                .positioning,
            CharacterPositionProfile::Grid5x5
        );
    }
    for model in [Model::NaiDiffusion3, Model::NaiDiffusion3Furry] {
        assert!(model.descriptor().prompt().characters.is_none());
    }
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
        assert_eq!(limits.img2img_strength.max, 0.99);
        assert_eq!(limits.inpaint_strength.max, 1.0);
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
