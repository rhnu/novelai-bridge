use super::{AnlasEstimate, AnlasEstimateInput, AnlasEstimateStatus, VIBE_ENCODE_COST};

/// Anlas cost per character reference, per generated image.
const CHARACTER_REFERENCE_COST: u32 = 5;
/// Number of active vibe encodings included before the overage charge starts.
const FREE_VIBE_REFERENCE_COUNT: u32 = 4;
/// Opus free-generation pixel ceiling.
const OPUS_FREE_PIXELS: u64 = 1024 * 1024;
/// Opus free-generation step ceiling.
const OPUS_FREE_STEPS: u32 = 28;
/// Highest per-image price accepted by the web client.
const MAX_PER_IMAGE_COST: u32 = 140;
/// Linear area term of the `NovelAI` NAI 3, 4, 4.5, 5 cost function.
const COST_AREA_TERM: f64 = 2.951_823_174_884_865e-6;
/// Area-times-steps term of the `NovelAI` NAI 3, 4, 4.5, 5 cost function.
const COST_AREA_STEP_TERM: f64 = 5.753_298_233_447_344e-7;

/// Estimates the Anlas cost of a NAI 3, 4, 4.5, 5 generation request.
///
/// The formula mirrors the cost function used by the `NovelAI` web client at the pricing-document
/// revision bundled with this crate. It intentionally does not implement V1/V2 or unrelated
/// models. The returned breakdown still contains the mathematical cost when the per-image price
/// exceeds 140; check [`AnlasEstimate::is_too_expensive`] to handle the web client's `-3` state.
#[must_use]
pub fn estimate_anlas_cost(input: AnlasEstimateInput) -> AnlasEstimate {
    let pixels = input.size.pixels();
    #[expect(
        clippy::cast_precision_loss,
        reason = "Pricing follows the provider floating point area formula"
    )]
    let pixels_f = pixels as f64;
    let descriptor = input.model.descriptor();
    // Keep the web client's operation order; an FMA can cross an integer ceil boundary.
    #[expect(
        clippy::suboptimal_flops,
        reason = "Changing operation order can cross an integer price rounding boundary"
    )]
    let base = (COST_AREA_TERM * pixels_f
        + COST_AREA_STEP_TERM
            * pixels_f
            * f64::from(input.steps)
            * descriptor.pricing().step_factor)
        .ceil();
    let smea_multiplier = if input.smea && input.smea_dyn {
        1.4
    } else if input.smea {
        1.2
    } else {
        1.0
    };
    let base_cost = base * smea_multiplier * descriptor.pricing().multiplier;
    let strength = input.mode.effective_strength();
    let per_image = saturating_u32((base_cost * strength).ceil()).max(2);
    let status = if per_image > MAX_PER_IMAGE_COST {
        AnlasEstimateStatus::TooExpensive
    } else {
        AnlasEstimateStatus::Available
    };

    let requested_samples = input.n_samples;
    let sample_range = descriptor.constraints().samples;
    let api_samples = requested_samples.clamp(sample_range.min, sample_range.max);
    let sample_limit = descriptor.sample_limit(input.size);
    let priced_samples = api_samples.min(sample_limit);
    let character_reference_count = chargeable_character_reference_count(&input);
    let character_reference = character_reference_count
        .saturating_mul(CHARACTER_REFERENCE_COST)
        .saturating_mul(priced_samples);
    let vibe_encoding = if descriptor.can_encode_vibe() {
        input.pending_vibe_encodes.saturating_mul(VIBE_ENCODE_COST)
    } else {
        0
    };
    let vibe_reference_overage = if descriptor.can_encode_vibe()
        && descriptor
            .resolve_references(
                input.mode.generation_mode(),
                crate::ReferenceIntent {
                    vibe: input.vibe_reference_count > 0,
                    character: character_reference_count > 0,
                },
            )
            .vibe
            .is_available()
    {
        input
            .vibe_reference_count
            .saturating_sub(FREE_VIBE_REFERENCE_COUNT)
            .saturating_mul(VIBE_ENCODE_COST)
    } else {
        0
    };
    let free_first_image_applied =
        is_free_first_image_eligible(&input, pixels, character_reference_count, priced_samples);
    let billable_samples = priced_samples.saturating_sub(u32::from(free_first_image_applied));
    let generation = per_image.saturating_mul(billable_samples);
    let total = generation
        .saturating_add(character_reference)
        .saturating_add(vibe_encoding)
        .saturating_add(vibe_reference_overage);

    AnlasEstimate {
        status,
        per_image,
        generation,
        character_reference,
        vibe_encoding,
        vibe_reference_overage,
        total,
        requested_samples,
        sample_limit,
        priced_samples,
        billable_samples,
        free_first_image_applied,
    }
}

const fn chargeable_character_reference_count(input: &AnlasEstimateInput) -> u32 {
    let descriptor = input.model.descriptor();
    let mode = input.mode.generation_mode();
    let references_allowed = descriptor
        .resolve_references(
            mode,
            crate::ReferenceIntent {
                vibe: false,
                character: true,
            },
        )
        .character
        .is_available();
    if references_allowed {
        input.character_reference_count
    } else {
        0
    }
}

const fn is_free_first_image_eligible(
    input: &AnlasEstimateInput,
    pixels: u64,
    character_reference_count: u32,
    capped_samples: u32,
) -> bool {
    let descriptor = input.model.descriptor();
    input.context.tier >= 3
        && input.context.subscription_active
        && !input.context.free_first_image_used
        && character_reference_count == 0
        && pixels <= OPUS_FREE_PIXELS
        && input.steps <= OPUS_FREE_STEPS
        && !(matches!(
            descriptor.pricing().opus_allowance,
            crate::OpusAllowance::Metered
        ) && input.context.v5_usage_is_negative)
        && capped_samples > 0
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Explicit bounds checks saturate the value before conversion"
)]
fn saturating_u32(value: f64) -> u32 {
    if value <= 0.0 {
        0
    } else if value >= f64::from(u32::MAX) {
        u32::MAX
    } else {
        value as u32
    }
}
