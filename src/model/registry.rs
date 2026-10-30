//! Explicit composition: no model descriptor inherits from another model.
use super::{
    CanvasConstraints, CharacterPositionProfile, CharacterPromptProfile, GenerationConstraints,
    ImageTokenizerKind, InpaintingProfile, Model, ModelDefaults, ModelDescriptor, ModelIdentity,
    ModelSupport, NoiseScheduleSupport, OpusAllowance, ParameterRange, PricingProfile,
    PromptProfile, ReferenceModes, ReferenceProfile, SamplerCompatibility, SamplingProfile,
    VarietyBoostProfile, VibeInputPath, VibeProfile, WireProfile,
};
use crate::{ImageSize, NoiseSchedule, QualityPreset, Sampler, SamplingParams, UcPreset};

const SYNTAX_V3: crate::PromptSyntaxProfile = crate::PromptSyntaxProfile {
    bracket_weights: true,
    numeric_weights: false,
    negative_weights: false,
    legacy_mixing: true,
    character_separators: false,
    randomizer: true,
    text_rendering: false,
    interaction_tags: false,
};
const SYNTAX_V4: crate::PromptSyntaxProfile = crate::PromptSyntaxProfile {
    bracket_weights: true,
    numeric_weights: true,
    negative_weights: false,
    legacy_mixing: false,
    character_separators: true,
    randomizer: true,
    text_rendering: true,
    interaction_tags: true,
};
const SYNTAX_MODERN: crate::PromptSyntaxProfile = crate::PromptSyntaxProfile {
    negative_weights: true,
    ..SYNTAX_V4
};

const CONSTRAINTS: GenerationConstraints = GenerationConstraints {
    canvas: CanvasConstraints {
        dimension: ParameterRange {
            min: 64,
            max: 16_384,
        },
        multiple: 64,
        max_pixels: 3_145_728,
    },
    steps: ParameterRange { min: 1, max: 50 },
    samples: ParameterRange { min: 1, max: 8 },
    scale: ParameterRange {
        min: 0.0,
        max: 10.0,
    },
    cfg_rescale: ParameterRange { min: 0.0, max: 1.0 },
    img2img_strength: ParameterRange {
        min: 0.01,
        max: 0.99,
    },
    inpaint_strength: ParameterRange {
        min: 0.01,
        max: 1.0,
    },
    img2img_noise: ParameterRange {
        min: 0.0,
        max: 0.99,
    },
};

const fn defaults(scale: f32) -> ModelDefaults {
    ModelDefaults {
        sampling: SamplingParams {
            steps: 23,
            scale,
            sampler: Sampler::KEulerAncestral,
            noise_schedule: NoiseSchedule::Karras,
            cfg_rescale: 0.0,
        },
        size: ImageSize {
            width: 832,
            height: 1216,
        },
        n_samples: 1,
        quality: QualityPreset::Standard,
        uc_preset: UcPreset::Heavy,
    }
}

const SAMPLING_LEGACY: SamplingProfile = SamplingProfile {
    fixed_sampler: None,
    sampler_compatibility: SamplerCompatibility::Legacy,
    noise_schedules: NoiseScheduleSupport::NativeAndExplicit,
    smea: true,
    dynamic_thresholding: true,
    variety: Some(VarietyBoostProfile {
        sigma_coefficient: 19.0,
    }),
};
const SAMPLING_V4: SamplingProfile = SamplingProfile {
    fixed_sampler: None,
    sampler_compatibility: SamplerCompatibility::Structured,
    noise_schedules: NoiseScheduleSupport::ExplicitOnly,
    smea: false,
    dynamic_thresholding: false,
    variety: Some(VarietyBoostProfile {
        sigma_coefficient: 19.0,
    }),
};
const SAMPLING_V45: SamplingProfile = SamplingProfile {
    fixed_sampler: None,
    sampler_compatibility: SamplerCompatibility::Structured,
    noise_schedules: NoiseScheduleSupport::ExplicitOnly,
    smea: false,
    dynamic_thresholding: false,
    variety: Some(VarietyBoostProfile {
        sigma_coefficient: 58.0,
    }),
};
const SAMPLING_V5: SamplingProfile = SamplingProfile {
    fixed_sampler: None,
    sampler_compatibility: SamplerCompatibility::Structured,
    noise_schedules: NoiseScheduleSupport::KarrasOnly,
    smea: false,
    dynamic_thresholding: false,
    variety: None,
};

const V5_FULL: ModelDescriptor = ModelDescriptor {
    identity: ModelIdentity {
        model: Model::NaiDiffusion5Full,
        api_id: "nai-diffusion-5-full",
    },
    support: ModelSupport {
        inpainting: Some(InpaintingProfile {
            model_id: "nai-diffusion-5-full-inpainting",
            image_to_image: true,
        }),
        streaming: true,
        max_enhance: true,
        vibe: None,
        character_reference: None,
    },
    constraints: CONSTRAINTS,
    defaults: defaults(7.0),
    prompt: PromptProfile {
        syntax: SYNTAX_MODERN,
        tokenizer: ImageTokenizerKind::Qwen,
        token_limit: 1471,
        characters: Some(CharacterPromptProfile {
            max_characters: 32,
            positioning: CharacterPositionProfile::Freeform,
        }),
        quality_presets: super::presets::V5_FULL_QUALITY,
        uc_presets: super::presets::V5_FULL_UC,
        suppress_nsfw: true,
        custom_negative_prompts: true,
        legacy_uc: false,
        transparent_background: true,
        furry_mode: true,
    },
    sampling: SAMPLING_V5,
    wire: WireProfile::StructuredV5,
    pricing: PricingProfile {
        multiplier: 1.5,
        step_factor: 1.0,
        opus_allowance: OpusAllowance::Metered,
    },
};

const V5_MEDIUM: ModelDescriptor = ModelDescriptor {
    identity: ModelIdentity {
        model: Model::NaiDiffusion5FullMedium,
        api_id: "nai-diffusion-5-full-medium",
    },
    support: ModelSupport {
        inpainting: Some(InpaintingProfile {
            model_id: "nai-diffusion-5-full-medium-inpainting",
            image_to_image: true,
        }),
        streaming: true,
        max_enhance: true,
        vibe: None,
        character_reference: None,
    },
    constraints: GenerationConstraints {
        steps: ParameterRange { min: 14, max: 14 },
        cfg_rescale: ParameterRange { min: 0.0, max: 0.0 },
        ..CONSTRAINTS
    },
    defaults: ModelDefaults {
        sampling: SamplingParams {
            steps: 14,
            ..defaults(7.0).sampling
        },
        ..defaults(7.0)
    },
    prompt: PromptProfile {
        syntax: SYNTAX_MODERN,
        tokenizer: ImageTokenizerKind::Qwen,
        token_limit: 1471,
        characters: Some(CharacterPromptProfile {
            max_characters: 32,
            positioning: CharacterPositionProfile::Freeform,
        }),
        quality_presets: super::presets::V5_FULL_QUALITY,
        uc_presets: super::presets::V5_MEDIUM_UC,
        suppress_nsfw: true,
        custom_negative_prompts: false,
        legacy_uc: false,
        transparent_background: true,
        furry_mode: true,
    },
    sampling: SamplingProfile {
        fixed_sampler: Some(Sampler::KEulerAncestral),
        ..SAMPLING_V5
    },
    wire: WireProfile::StructuredV5,
    pricing: PricingProfile {
        multiplier: 1.5,
        step_factor: 1.0 / 1.065_217_39,
        opus_allowance: OpusAllowance::Metered,
    },
};

const V5_CURATED: ModelDescriptor = ModelDescriptor {
    identity: ModelIdentity {
        model: Model::NaiDiffusion5Curated,
        api_id: "nai-diffusion-5-curated",
    },
    support: ModelSupport {
        inpainting: Some(InpaintingProfile {
            model_id: "nai-diffusion-4-5-curated-inpainting",
            image_to_image: true,
        }),
        streaming: true,
        max_enhance: true,
        vibe: None,
        character_reference: None,
    },
    constraints: CONSTRAINTS,
    defaults: defaults(7.0),
    prompt: PromptProfile {
        syntax: SYNTAX_MODERN,
        tokenizer: ImageTokenizerKind::Qwen,
        token_limit: 703,
        characters: Some(CharacterPromptProfile {
            max_characters: 32,
            positioning: CharacterPositionProfile::Freeform,
        }),
        quality_presets: super::presets::V5_CURATED_QUALITY,
        uc_presets: super::presets::V5_CURATED_UC,
        suppress_nsfw: false,
        custom_negative_prompts: true,
        legacy_uc: false,
        transparent_background: true,
        furry_mode: true,
    },
    sampling: SAMPLING_V5,
    wire: WireProfile::StructuredV5,
    pricing: PricingProfile {
        multiplier: 1.5,
        step_factor: 1.0,
        opus_allowance: OpusAllowance::Metered,
    },
};

const V45_FULL: ModelDescriptor = ModelDescriptor {
    identity: ModelIdentity {
        model: Model::NaiDiffusion45Full,
        api_id: "nai-diffusion-4-5-full",
    },
    support: ModelSupport {
        inpainting: Some(InpaintingProfile {
            model_id: "nai-diffusion-4-5-full-inpainting",
            image_to_image: true,
        }),
        streaming: true,
        max_enhance: false,
        vibe: Some(VibeProfile {
            metadata_key: "v4-5full",
            input: VibeInputPath::Encoded,
            modes: ReferenceModes::Unmasked,
        }),
        character_reference: Some(ReferenceProfile {
            modes: ReferenceModes::All,
        }),
    },
    constraints: CONSTRAINTS,
    defaults: defaults(5.0),
    prompt: PromptProfile {
        syntax: SYNTAX_MODERN,
        tokenizer: ImageTokenizerKind::T5,
        token_limit: 512,
        characters: Some(CharacterPromptProfile {
            max_characters: 6,
            positioning: CharacterPositionProfile::Grid5x5,
        }),
        quality_presets: super::presets::V45_FULL_QUALITY,
        uc_presets: super::presets::V45_FULL_UC,
        suppress_nsfw: true,
        custom_negative_prompts: true,
        legacy_uc: false,
        transparent_background: false,
        furry_mode: true,
    },
    sampling: SAMPLING_V45,
    wire: WireProfile::StructuredV4,
    pricing: PricingProfile {
        multiplier: 1.0,
        step_factor: 1.0,
        opus_allowance: OpusAllowance::Standard,
    },
};

const V45_CURATED: ModelDescriptor = ModelDescriptor {
    identity: ModelIdentity {
        model: Model::NaiDiffusion45Curated,
        api_id: "nai-diffusion-4-5-curated",
    },
    support: ModelSupport {
        inpainting: Some(InpaintingProfile {
            model_id: "nai-diffusion-4-5-curated-inpainting",
            image_to_image: true,
        }),
        streaming: true,
        max_enhance: false,
        vibe: Some(VibeProfile {
            metadata_key: "v4-5curated",
            input: VibeInputPath::Encoded,
            modes: ReferenceModes::Unmasked,
        }),
        character_reference: Some(ReferenceProfile {
            modes: ReferenceModes::All,
        }),
    },
    constraints: CONSTRAINTS,
    defaults: defaults(5.0),
    prompt: PromptProfile {
        syntax: SYNTAX_MODERN,
        tokenizer: ImageTokenizerKind::T5,
        token_limit: 512,
        characters: Some(CharacterPromptProfile {
            max_characters: 6,
            positioning: CharacterPositionProfile::Grid5x5,
        }),
        quality_presets: super::presets::V45_CURATED_QUALITY,
        uc_presets: super::presets::V45_CURATED_UC,
        suppress_nsfw: false,
        custom_negative_prompts: true,
        legacy_uc: false,
        transparent_background: false,
        furry_mode: true,
    },
    sampling: SAMPLING_V45,
    wire: WireProfile::StructuredV4,
    pricing: PricingProfile {
        multiplier: 1.0,
        step_factor: 1.0,
        opus_allowance: OpusAllowance::Standard,
    },
};

const V4_FULL: ModelDescriptor = ModelDescriptor {
    identity: ModelIdentity {
        model: Model::NaiDiffusion4Full,
        api_id: "nai-diffusion-4-full",
    },
    support: ModelSupport {
        inpainting: Some(InpaintingProfile {
            model_id: "nai-diffusion-4-full-inpainting",
            image_to_image: true,
        }),
        streaming: true,
        max_enhance: false,
        vibe: Some(VibeProfile {
            metadata_key: "v4full",
            input: VibeInputPath::Encoded,
            modes: ReferenceModes::Unmasked,
        }),
        character_reference: None,
    },
    constraints: CONSTRAINTS,
    defaults: defaults(5.5),
    prompt: PromptProfile {
        syntax: SYNTAX_V4,
        tokenizer: ImageTokenizerKind::T5,
        token_limit: 512,
        characters: Some(CharacterPromptProfile {
            max_characters: 6,
            positioning: CharacterPositionProfile::Grid5x5,
        }),
        quality_presets: super::presets::V4_FULL_QUALITY,
        uc_presets: super::presets::V4_FULL_UC,
        suppress_nsfw: true,
        custom_negative_prompts: true,
        legacy_uc: true,
        transparent_background: false,
        furry_mode: true,
    },
    sampling: SAMPLING_V4,
    wire: WireProfile::StructuredV4,
    pricing: PricingProfile {
        multiplier: 1.0,
        step_factor: 1.0,
        opus_allowance: OpusAllowance::Standard,
    },
};

const V4_CURATED: ModelDescriptor = ModelDescriptor {
    identity: ModelIdentity {
        model: Model::NaiDiffusion4Curated,
        api_id: "nai-diffusion-4-curated-preview",
    },
    support: ModelSupport {
        inpainting: Some(InpaintingProfile {
            model_id: "nai-diffusion-4-curated-inpainting",
            image_to_image: true,
        }),
        streaming: true,
        max_enhance: false,
        vibe: Some(VibeProfile {
            metadata_key: "v4curated",
            input: VibeInputPath::Encoded,
            modes: ReferenceModes::Unmasked,
        }),
        character_reference: None,
    },
    constraints: CONSTRAINTS,
    defaults: defaults(5.5),
    prompt: PromptProfile {
        syntax: SYNTAX_V4,
        tokenizer: ImageTokenizerKind::T5,
        token_limit: 512,
        characters: Some(CharacterPromptProfile {
            max_characters: 6,
            positioning: CharacterPositionProfile::Grid5x5,
        }),
        quality_presets: super::presets::V4_CURATED_QUALITY,
        uc_presets: super::presets::V4_CURATED_UC,
        suppress_nsfw: false,
        custom_negative_prompts: true,
        legacy_uc: true,
        transparent_background: false,
        furry_mode: true,
    },
    sampling: SAMPLING_V4,
    wire: WireProfile::StructuredV4,
    pricing: PricingProfile {
        multiplier: 1.0,
        step_factor: 1.0,
        opus_allowance: OpusAllowance::Standard,
    },
};

const V3: ModelDescriptor = ModelDescriptor {
    identity: ModelIdentity {
        model: Model::NaiDiffusion3,
        api_id: "nai-diffusion-3",
    },
    support: ModelSupport {
        inpainting: Some(InpaintingProfile {
            model_id: "nai-diffusion-3-inpainting",
            image_to_image: false,
        }),
        streaming: false,
        max_enhance: false,
        vibe: Some(VibeProfile {
            metadata_key: "v3",
            input: VibeInputPath::RawImage,
            modes: ReferenceModes::All,
        }),
        character_reference: None,
    },
    constraints: CONSTRAINTS,
    defaults: defaults(5.0),
    prompt: PromptProfile {
        syntax: SYNTAX_V3,
        tokenizer: ImageTokenizerKind::Clip,
        token_limit: 225,
        characters: None,
        quality_presets: super::presets::V3_QUALITY,
        uc_presets: super::presets::V3_UC,
        suppress_nsfw: true,
        custom_negative_prompts: true,
        legacy_uc: false,
        transparent_background: false,
        furry_mode: false,
    },
    sampling: SAMPLING_LEGACY,
    wire: WireProfile::LegacyV3,
    pricing: PricingProfile {
        multiplier: 1.0,
        step_factor: 1.0,
        opus_allowance: OpusAllowance::Standard,
    },
};

const V3_FURRY: ModelDescriptor = ModelDescriptor {
    identity: ModelIdentity {
        model: Model::NaiDiffusion3Furry,
        api_id: "nai-diffusion-furry-3",
    },
    support: ModelSupport {
        inpainting: Some(InpaintingProfile {
            model_id: "nai-diffusion-furry-3-inpainting",
            image_to_image: false,
        }),
        streaming: false,
        max_enhance: false,
        vibe: Some(VibeProfile {
            metadata_key: "v3furry",
            input: VibeInputPath::RawImage,
            modes: ReferenceModes::All,
        }),
        character_reference: None,
    },
    constraints: CONSTRAINTS,
    defaults: defaults(6.2),
    prompt: PromptProfile {
        syntax: SYNTAX_V3,
        tokenizer: ImageTokenizerKind::Clip,
        token_limit: 225,
        characters: None,
        quality_presets: super::presets::V3_FURRY_QUALITY,
        uc_presets: super::presets::V3_FURRY_UC,
        suppress_nsfw: true,
        custom_negative_prompts: true,
        legacy_uc: false,
        transparent_background: false,
        furry_mode: false,
    },
    sampling: SAMPLING_LEGACY,
    wire: WireProfile::LegacyV3,
    pricing: PricingProfile {
        multiplier: 1.0,
        step_factor: 1.0,
        opus_allowance: OpusAllowance::Standard,
    },
};

pub(super) const fn descriptor(model: Model) -> &'static ModelDescriptor {
    match model {
        Model::NaiDiffusion5Full => &V5_FULL,
        Model::NaiDiffusion5FullMedium => &V5_MEDIUM,
        Model::NaiDiffusion5Curated => &V5_CURATED,
        Model::NaiDiffusion45Full => &V45_FULL,
        Model::NaiDiffusion45Curated => &V45_CURATED,
        Model::NaiDiffusion4Full => &V4_FULL,
        Model::NaiDiffusion4Curated => &V4_CURATED,
        Model::NaiDiffusion3 => &V3,
        Model::NaiDiffusion3Furry => &V3_FURRY,
    }
}
