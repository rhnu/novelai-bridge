mod support;

use novelai_bridge::{
    AnlasEstimateInput, AnlasEstimateStatus, AnlasPricingContext, CharacterReference,
    CharacterReferenceType, Client, GenerateImageRequest, GuidanceParams, ImageSize, Model,
    RequestCostEstimate, Sampler, VibeReference, VibeTransferConfig, estimate_anlas_cost,
    estimate_director_tool_cost,
};

use support::MockTransport;

fn input(model: Model, size: ImageSize, steps: u32) -> AnlasEstimateInput {
    AnlasEstimateInput {
        model,
        size,
        steps,
        ..AnlasEstimateInput::default()
    }
}

#[test]
fn matches_documented_nai3_to5_samples() {
    let v5_23 = estimate_anlas_cost(input(Model::NaiDiffusion5Curated, ImageSize::square(), 23));
    assert_eq!(v5_23.per_image, 26);

    let v5_28 = estimate_anlas_cost(input(Model::NaiDiffusion5Curated, ImageSize::square(), 28));
    assert_eq!(v5_28.per_image, 30);

    let v4_portrait = estimate_anlas_cost(input(
        Model::NaiDiffusion45Curated,
        ImageSize::portrait(),
        28,
    ));
    assert_eq!(v4_portrait.per_image, 20);

    let v4_large = estimate_anlas_cost(input(
        Model::NaiDiffusion4Full,
        ImageSize {
            width: 1472,
            height: 1472,
        },
        28,
    ));
    assert_eq!(v4_large.per_image, 42);

    let v3_square = estimate_anlas_cost(input(Model::NaiDiffusion3, ImageSize::square(), 28));
    assert_eq!(v3_square.per_image, 20);
}

#[test]
fn applies_resolution_sample_cap_and_smea_multiplier() {
    let mut small = input(
        Model::NaiDiffusion3,
        ImageSize {
            width: 256,
            height: 256,
        },
        28,
    );
    small.n_samples = 12;
    let capped = estimate_anlas_cost(small);
    assert_eq!(capped.requested_samples, 12);
    assert_eq!(capped.sample_limit, 4);
    assert_eq!(capped.priced_samples, 4);
    assert_eq!(capped.billable_samples, 4);
    assert_eq!(capped.total, capped.per_image * 4);

    let mut smea = input(Model::NaiDiffusion3, ImageSize::square(), 28);
    smea.smea = true;
    smea.smea_dyn = true;
    let with_smea = estimate_anlas_cost(smea);
    assert_eq!(with_smea.per_image, 28);
}

#[test]
fn applies_opus_first_image_rule_and_v5_overdrawn_exception() {
    let context = AnlasPricingContext {
        tier: 3,
        subscription_active: true,
        ..AnlasPricingContext::default()
    };
    let mut v4 = input(Model::NaiDiffusion45Curated, ImageSize::square(), 28);
    v4.context = context;
    v4.n_samples = 2;
    let discounted = estimate_anlas_cost(v4);
    assert!(discounted.free_first_image_applied);
    assert_eq!(discounted.billable_samples, 1);
    assert_eq!(discounted.total, 20);

    v4.n_samples = 1;
    let fully_free = estimate_anlas_cost(v4);
    assert!(fully_free.is_free());
    assert_eq!(fully_free.total, 0);

    let mut v5 = v4;
    v5.model = Model::NaiDiffusion5Curated;
    v5.n_samples = 2;
    v5.context.v5_usage_is_negative = true;
    let overdrawn = estimate_anlas_cost(v5);
    assert!(!overdrawn.free_first_image_applied);
    assert_eq!(overdrawn.total, 60);
}

#[test]
fn applies_reference_and_vibe_surcharges() {
    let mut references = input(Model::NaiDiffusion45Curated, ImageSize::portrait(), 28);
    references.n_samples = 2;
    references.character_reference_count = 2;
    references.vibe_reference_count = 6;
    references.pending_vibe_encodes = 2;
    let estimate = estimate_anlas_cost(references);

    assert_eq!(estimate.generation, 40);
    assert_eq!(estimate.character_reference, 20);
    assert_eq!(estimate.vibe_encoding, 4);
    assert_eq!(estimate.vibe_reference_overage, 4);
    assert_eq!(estimate.total, 68);

    let mut v5 = references;
    v5.model = Model::NaiDiffusion5Curated;
    let ignored = estimate_anlas_cost(v5);
    assert_eq!(ignored.character_reference, 0);
    assert_eq!(ignored.vibe_encoding, 0);
    assert_eq!(ignored.vibe_reference_overage, 0);
}

#[test]
fn uses_inpaint_strength_and_reports_too_expensive_requests() {
    let mut inpaint = input(Model::NaiDiffusion45Curated, ImageSize::portrait(), 28);
    inpaint.has_inpaint_mask = true;
    inpaint.img2img_strength = Some(0.2);
    inpaint.inpaint_img2img_strength = Some(0.5);
    assert_eq!(estimate_anlas_cost(inpaint).per_image, 10);

    inpaint.inpaint_img2img_strength = None;
    assert_eq!(estimate_anlas_cost(inpaint).per_image, 20);

    let too_expensive = estimate_anlas_cost(input(
        Model::NaiDiffusion5Curated,
        ImageSize {
            width: 1600,
            height: 1966,
        },
        50,
    ));
    assert_eq!(too_expensive.status, AnlasEstimateStatus::TooExpensive);
    assert!(too_expensive.is_too_expensive());
}

#[test]
fn a_resolution_with_no_priced_samples_is_not_reported_as_free() {
    let estimate = estimate_anlas_cost(input(
        Model::NaiDiffusion3,
        ImageSize {
            width: 2_048,
            height: 2_048,
        },
        1,
    ));

    assert_eq!(estimate.priced_samples, 0);
    assert_eq!(estimate.total, 0);
    assert!(!estimate.is_free());
}

#[test]
fn request_and_client_helpers_share_the_standalone_calculator() {
    let request = GenerateImageRequest::builder("1girl", Model::NaiDiffusion3)
        .size(ImageSize::square())
        .steps(28)
        .n_samples(2)
        .sampler(Sampler::Ddim)
        .guidance(GuidanceParams {
            smea: true,
            smea_dyn: false,
            ..GuidanceParams::default()
        })
        .build();
    let context = AnlasPricingContext::default();

    let from_request = request.estimate_anlas_cost(context);
    let from_input = estimate_anlas_cost(request.anlas_estimate_input(context));
    assert_eq!(from_request, from_input);

    let client = Client::with_transport(MockTransport::default());
    assert_eq!(client.estimate_anlas_cost(&request), from_request);
}

#[test]
fn request_mapping_matches_non_strict_reference_conflict_resolution() {
    let request = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .vibe_transfer(VibeTransferConfig {
            strength: 1.0,
            references: (0..5)
                .map(|_| VibeReference {
                    strength: 1.0,
                    vibe_data_cache: "AQID".to_owned(),
                })
                .collect(),
        })
        .character_references(vec![CharacterReference {
            image: "AQID".to_owned(),
            reference_type: CharacterReferenceType::Character,
            fidelity: 1.0,
            strength: 1.0,
        }])
        .build();

    let estimate = request.estimate_anlas_cost(AnlasPricingContext::default());
    assert_eq!(estimate.character_reference, 5);
    assert_eq!(estimate.vibe_reference_overage, 0);
}

#[test]
fn director_discount_requires_an_active_opus_subscription() {
    let size = ImageSize {
        width: 512,
        height: 512,
    };
    let inactive = AnlasPricingContext {
        tier: 3,
        subscription_active: false,
        ..AnlasPricingContext::default()
    };
    let active = AnlasPricingContext {
        subscription_active: true,
        ..inactive
    };

    assert_eq!(estimate_director_tool_cost(size, inactive), 1);
    assert_eq!(estimate_director_tool_cost(size, active), 0);
}

#[test]
fn observed_cost_shape_exposes_a_uniform_total() {
    let generation =
        estimate_anlas_cost(input(Model::NaiDiffusion5Curated, ImageSize::square(), 23));
    assert_eq!(
        RequestCostEstimate::Generation(generation).total(),
        generation.total
    );
    assert_eq!(RequestCostEstimate::Fixed { anlas: 2 }.total(), 2);
}
