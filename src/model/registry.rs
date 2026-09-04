//! Explicit composition: no model descriptor inherits from another model.
use super::{
    CanvasConstraints, CharacterPositionProfile, CharacterPromptProfile, GenerationConstraints,
    ImageTokenizerKind, InpaintingProfile, Model, ModelDefaults, ModelDescriptor, ModelIdentity,
    ModelSupport, NoiseScheduleSupport, OpusAllowance, ParameterRange, PricingProfile,
    PromptProfile, QualitySupport, ReferenceModes, ReferenceProfile, SamplingProfile,
    VarietyBoostProfile, VibeInputPath, VibeProfile, WireProfile,
};
use crate::{ImageSize, NoiseSchedule, QualityPreset, Sampler, SamplingParams, UcPreset};

const CONSTRAINTS: GenerationConstraints = GenerationConstraints {
    canvas: CanvasConstraints {
        dimension: ParameterRange { min: 64, max: 1600 },
        multiple: 64,
        max_pixels: 3_145_728,
    },
    steps: ParameterRange { min: 1, max: 50 },
    samples: ParameterRange { min: 1, max: 4 },
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
        uc_preset: UcPreset::Light,
    }
}

const SAMPLING_LEGACY: SamplingProfile = SamplingProfile {
    noise_schedules: NoiseScheduleSupport::NativeAndExplicit,
    smea: true,
    dynamic_thresholding: true,
    variety: None,
};
const SAMPLING_V4: SamplingProfile = SamplingProfile {
    noise_schedules: NoiseScheduleSupport::ExplicitOnly,
    smea: false,
    dynamic_thresholding: false,
    variety: Some(VarietyBoostProfile {
        sigma_coefficient: 19.0,
    }),
};
const SAMPLING_V45: SamplingProfile = SamplingProfile {
    noise_schedules: NoiseScheduleSupport::ExplicitOnly,
    smea: false,
    dynamic_thresholding: false,
    variety: Some(VarietyBoostProfile {
        sigma_coefficient: 58.0,
    }),
};
const SAMPLING_V5: SamplingProfile = SamplingProfile {
    noise_schedules: NoiseScheduleSupport::ExplicitOnly,
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
        }),
        streaming: true,
        vibe: None,
        character_reference: None,
    },
    constraints: CONSTRAINTS,
    defaults: defaults(7.0),
    prompt: PromptProfile {
        tokenizer: ImageTokenizerKind::Qwen,
        token_limit: 1471,
        characters: Some(CharacterPromptProfile {
            max_characters: 32,
            positioning: CharacterPositionProfile::Freeform,
        }),
        quality: QualitySupport::StandardAndLight,
        transparent_background: true,
        furry_mode: true,
    },
    sampling: SAMPLING_V5,
    wire: WireProfile::StructuredV5,
    pricing: PricingProfile {
        multiplier: 1.5,
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
            model_id: "nai-diffusion-5-curated-inpainting",
        }),
        streaming: true,
        vibe: None,
        character_reference: None,
    },
    constraints: CONSTRAINTS,
    defaults: defaults(7.0),
    prompt: PromptProfile {
        tokenizer: ImageTokenizerKind::Qwen,
        token_limit: 703,
        characters: Some(CharacterPromptProfile {
            max_characters: 32,
            positioning: CharacterPositionProfile::Freeform,
        }),
        quality: QualitySupport::StandardAndLight,
        transparent_background: true,
        furry_mode: true,
    },
    sampling: SAMPLING_V5,
    wire: WireProfile::StructuredV5,
    pricing: PricingProfile {
        multiplier: 1.5,
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
        }),
        streaming: true,
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
        tokenizer: ImageTokenizerKind::T5,
        token_limit: 512,
        characters: Some(CharacterPromptProfile {
            max_characters: 6,
            positioning: CharacterPositionProfile::Grid5x5,
        }),
        quality: QualitySupport::Standard,
        transparent_background: false,
        furry_mode: true,
    },
    sampling: SAMPLING_V45,
    wire: WireProfile::StructuredV4,
    pricing: PricingProfile {
        multiplier: 1.0,
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
        }),
        streaming: true,
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
        tokenizer: ImageTokenizerKind::T5,
        token_limit: 512,
        characters: Some(CharacterPromptProfile {
            max_characters: 6,
            positioning: CharacterPositionProfile::Grid5x5,
        }),
        quality: QualitySupport::Standard,
        transparent_background: false,
        furry_mode: true,
    },
    sampling: SAMPLING_V45,
    wire: WireProfile::StructuredV4,
    pricing: PricingProfile {
        multiplier: 1.0,
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
        }),
        streaming: true,
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
        tokenizer: ImageTokenizerKind::T5,
        token_limit: 512,
        characters: Some(CharacterPromptProfile {
            max_characters: 6,
            positioning: CharacterPositionProfile::Grid5x5,
        }),
        quality: QualitySupport::Standard,
        transparent_background: false,
        furry_mode: true,
    },
    sampling: SAMPLING_V4,
    wire: WireProfile::StructuredV4,
    pricing: PricingProfile {
        multiplier: 1.0,
        opus_allowance: OpusAllowance::Standard,
    },
};

const V4_CURATED: ModelDescriptor = ModelDescriptor {
    identity: ModelIdentity {
        model: Model::NaiDiffusion4Curated,
        api_id: "nai-diffusion-4-curated",
    },
    support: ModelSupport {
        inpainting: Some(InpaintingProfile {
            model_id: "nai-diffusion-4-curated-inpainting",
        }),
        streaming: true,
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
        tokenizer: ImageTokenizerKind::T5,
        token_limit: 512,
        characters: Some(CharacterPromptProfile {
            max_characters: 6,
            positioning: CharacterPositionProfile::Grid5x5,
        }),
        quality: QualitySupport::Standard,
        transparent_background: false,
        furry_mode: true,
    },
    sampling: SAMPLING_V4,
    wire: WireProfile::StructuredV4,
    pricing: PricingProfile {
        multiplier: 1.0,
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
        }),
        streaming: false,
        vibe: Some(VibeProfile {
            metadata_key: "v3",
            input: VibeInputPath::ProviderOnlyLegacy,
            modes: ReferenceModes::Unmasked,
        }),
        character_reference: None,
    },
    constraints: CONSTRAINTS,
    defaults: defaults(5.0),
    prompt: PromptProfile {
        tokenizer: ImageTokenizerKind::Clip,
        token_limit: 225,
        characters: None,
        quality: QualitySupport::Standard,
        transparent_background: false,
        furry_mode: false,
    },
    sampling: SAMPLING_LEGACY,
    wire: WireProfile::LegacyV3,
    pricing: PricingProfile {
        multiplier: 1.0,
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
        }),
        streaming: false,
        vibe: Some(VibeProfile {
            metadata_key: "v3furry",
            input: VibeInputPath::ProviderOnlyLegacy,
            modes: ReferenceModes::Unmasked,
        }),
        character_reference: None,
    },
    constraints: CONSTRAINTS,
    defaults: defaults(6.2),
    prompt: PromptProfile {
        tokenizer: ImageTokenizerKind::Clip,
        token_limit: 225,
        characters: None,
        quality: QualitySupport::Standard,
        transparent_background: false,
        furry_mode: false,
    },
    sampling: SAMPLING_LEGACY,
    wire: WireProfile::LegacyV3,
    pricing: PricingProfile {
        multiplier: 1.0,
        opus_allowance: OpusAllowance::Standard,
    },
};

pub(super) const fn descriptor(model: Model) -> &'static ModelDescriptor {
    match model {
        Model::NaiDiffusion5Full => &V5_FULL,
        Model::NaiDiffusion5Curated => &V5_CURATED,
        Model::NaiDiffusion45Full => &V45_FULL,
        Model::NaiDiffusion45Curated => &V45_CURATED,
        Model::NaiDiffusion4Full => &V4_FULL,
        Model::NaiDiffusion4Curated => &V4_CURATED,
        Model::NaiDiffusion3 => &V3,
        Model::NaiDiffusion3Furry => &V3_FURRY,
    }
}
