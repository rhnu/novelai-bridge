//! Model-specific preset text from the `NovelAI` frontend preset definitions.
use super::{QualityPresetDefinition, UcPresetDefinition};
use crate::{QualityPreset, UcPreset};

const V5_FULL_QUALITY_STANDARD: QualityPresetDefinition = QualityPresetDefinition {
    preset: QualityPreset::Standard,
    suffix: "very aesthetic, masterpiece, no text",
};
const V5_FULL_QUALITY_LIGHT: QualityPresetDefinition = QualityPresetDefinition {
    preset: QualityPreset::Light,
    suffix: "very aesthetic, amazing quality, no text",
};
const V5_FULL_QUALITY_NONE: QualityPresetDefinition = QualityPresetDefinition {
    preset: QualityPreset::None,
    suffix: "",
};
const V5_FULL_UC_HEAVY: UcPresetDefinition = UcPresetDefinition {
    preset: UcPreset::Heavy,
    prefix: "lowres, artistic error, film grain, scan artifacts, worst quality, bad quality, jpeg artifacts, very displeasing, chromatic aberration, dithering, halftone, screentone, multiple views, logo, too many watermarks, negative space, blank page",
};
const V5_FULL_UC_LIGHT: UcPresetDefinition = UcPresetDefinition {
    preset: UcPreset::Light,
    prefix: "lowres, bad hands, bad anatomy, artistic error, sepia, white haze, worst quality, very displeasing, jpeg artifacts, 0::ai-generated::",
};
const V5_FULL_UC_FURRYFOCUS: UcPresetDefinition = UcPresetDefinition {
    preset: UcPreset::FurryFocus,
    prefix: "{worst quality}, distracting watermark, unfinished, bad quality, {widescreen}, upscale, {sequence}, {{grandfathered content}}, blurred foreground, chromatic aberration, sketch, everyone, [sketch background], simple, [flat colors], ych (character), outline, multiple scenes, [[horror (theme)]], comic",
};
const V5_FULL_UC_HUMANFOCUS: UcPresetDefinition = UcPresetDefinition {
    preset: UcPreset::HumanFocus,
    prefix: "lowres, artistic error, film grain, scan artifacts, worst quality, bad quality, jpeg artifacts, very displeasing, chromatic aberration, dithering, halftone, screentone, multiple views, logo, too many watermarks, negative space, blank page, @_@, mismatched pupils, glowing eyes, bad anatomy",
};
const V5_FULL_UC_NONE: UcPresetDefinition = UcPresetDefinition {
    preset: UcPreset::None,
    prefix: "",
};
const V45_FULL_UC_LIGHT: UcPresetDefinition = UcPresetDefinition {
    preset: UcPreset::Light,
    prefix: "lowres, artistic error, scan artifacts, worst quality, bad quality, jpeg artifacts, multiple views, very displeasing, too many watermarks, negative space, blank page",
};
const V45_CURATED_QUALITY_STANDARD: QualityPresetDefinition = QualityPresetDefinition {
    preset: QualityPreset::Standard,
    suffix: "very aesthetic, masterpiece, no text, -0.8::feet::, rating:general",
};
const V45_CURATED_UC_HEAVY: UcPresetDefinition = UcPresetDefinition {
    preset: UcPreset::Heavy,
    prefix: "blurry, lowres, upscaled, artistic error, film grain, scan artifacts, worst quality, bad quality, jpeg artifacts, very displeasing, chromatic aberration, halftone, multiple views, logo, too many watermarks, negative space, blank page",
};
const V45_CURATED_UC_LIGHT: UcPresetDefinition = UcPresetDefinition {
    preset: UcPreset::Light,
    prefix: "blurry, lowres, upscaled, artistic error, scan artifacts, jpeg artifacts, logo, too many watermarks, negative space, blank page",
};
const V45_CURATED_UC_HUMANFOCUS: UcPresetDefinition = UcPresetDefinition {
    preset: UcPreset::HumanFocus,
    prefix: "blurry, lowres, upscaled, artistic error, film grain, scan artifacts, bad anatomy, bad hands, worst quality, bad quality, jpeg artifacts, very displeasing, chromatic aberration, halftone, multiple views, logo, too many watermarks, @_@, mismatched pupils, glowing eyes, negative space, blank page",
};
const V4_FULL_QUALITY_STANDARD: QualityPresetDefinition = QualityPresetDefinition {
    preset: QualityPreset::Standard,
    suffix: "no text, best quality, very aesthetic, absurdres",
};
const V4_FULL_UC_HEAVY: UcPresetDefinition = UcPresetDefinition {
    preset: UcPreset::Heavy,
    prefix: "blurry, lowres, error, film grain, scan artifacts, worst quality, bad quality, jpeg artifacts, very displeasing, chromatic aberration, multiple views, logo, too many watermarks, white blank page, blank page",
};
const V4_FULL_UC_LIGHT: UcPresetDefinition = UcPresetDefinition {
    preset: UcPreset::Light,
    prefix: "blurry, lowres, error, worst quality, bad quality, jpeg artifacts, very displeasing, white blank page, blank page",
};
const V4_CURATED_QUALITY_STANDARD: QualityPresetDefinition = QualityPresetDefinition {
    preset: QualityPreset::Standard,
    suffix: "rating:general, best quality, very aesthetic, absurdres",
};
const V4_CURATED_UC_HEAVY: UcPresetDefinition = UcPresetDefinition {
    preset: UcPreset::Heavy,
    prefix: "blurry, lowres, error, film grain, scan artifacts, worst quality, bad quality, jpeg artifacts, very displeasing, chromatic aberration, logo, dated, signature, multiple views, gigantic breasts, white blank page, blank page",
};
const V4_CURATED_UC_LIGHT: UcPresetDefinition = UcPresetDefinition {
    preset: UcPreset::Light,
    prefix: "blurry, lowres, error, worst quality, bad quality, jpeg artifacts, very displeasing, logo, dated, signature, white blank page, blank page",
};
const V3_QUALITY_STANDARD: QualityPresetDefinition = QualityPresetDefinition {
    preset: QualityPreset::Standard,
    suffix: "best quality, amazing quality, very aesthetic, absurdres",
};
const V3_UC_HEAVY: UcPresetDefinition = UcPresetDefinition {
    preset: UcPreset::Heavy,
    prefix: "lowres, {bad}, error, fewer, extra, missing, worst quality, jpeg artifacts, bad quality, watermark, unfinished, displeasing, chromatic aberration, signature, extra digits, artistic error, username, scan, [abstract]",
};
const V3_UC_LIGHT: UcPresetDefinition = UcPresetDefinition {
    preset: UcPreset::Light,
    prefix: "lowres, jpeg artifacts, worst quality, watermark, blurry, very displeasing",
};
const V3_UC_HUMANFOCUS: UcPresetDefinition = UcPresetDefinition {
    preset: UcPreset::HumanFocus,
    prefix: "lowres, {bad}, error, fewer, extra, missing, worst quality, jpeg artifacts, bad quality, watermark, unfinished, displeasing, chromatic aberration, signature, extra digits, artistic error, username, scan, [abstract], bad anatomy, bad hands, @_@, mismatched pupils, heart-shaped pupils, glowing eyes",
};
const V3_UC_NONE: UcPresetDefinition = UcPresetDefinition {
    preset: UcPreset::None,
    prefix: "lowres",
};
const V3_FURRY_QUALITY_STANDARD: QualityPresetDefinition = QualityPresetDefinition {
    preset: QualityPreset::Standard,
    suffix: "{best quality}, {amazing quality}",
};
const V3_FURRY_UC_HEAVY: UcPresetDefinition = UcPresetDefinition {
    preset: UcPreset::Heavy,
    prefix: "{{worst quality}}, [displeasing], {unusual pupils}, guide lines, {{unfinished}}, {bad}, url, artist name, {{tall image}}, mosaic, {sketch page}, comic panel, impact (font), [dated], {logo}, ych, {what}, {where is your god now}, {distorted text}, repeated text, {floating head}, {1994}, {widescreen}, absolutely everyone, sequence, {compression artifacts}, hard translated, {cropped}, {commissioner name}, unknown text, high contrast",
};
const V3_FURRY_UC_LIGHT: UcPresetDefinition = UcPresetDefinition {
    preset: UcPreset::Light,
    prefix: "{worst quality}, guide lines, unfinished, bad, url, tall image, widescreen, compression artifacts, unknown text",
};

pub(super) const V5_FULL_QUALITY: &[QualityPresetDefinition] = &[
    V5_FULL_QUALITY_STANDARD,
    V5_FULL_QUALITY_LIGHT,
    V5_FULL_QUALITY_NONE,
];
pub(super) const V5_FULL_UC: &[UcPresetDefinition] = &[
    V5_FULL_UC_HEAVY,
    V5_FULL_UC_LIGHT,
    V5_FULL_UC_FURRYFOCUS,
    V5_FULL_UC_HUMANFOCUS,
    V5_FULL_UC_NONE,
];
pub(super) const V5_CURATED_QUALITY: &[QualityPresetDefinition] = &[
    V5_FULL_QUALITY_STANDARD,
    V5_FULL_QUALITY_LIGHT,
    V5_FULL_QUALITY_NONE,
];
pub(super) const V5_CURATED_UC: &[UcPresetDefinition] = &[
    V5_FULL_UC_HEAVY,
    V5_FULL_UC_LIGHT,
    V5_FULL_UC_FURRYFOCUS,
    V5_FULL_UC_HUMANFOCUS,
    V5_FULL_UC_NONE,
];
pub(super) const V45_FULL_QUALITY: &[QualityPresetDefinition] =
    &[V5_FULL_QUALITY_STANDARD, V5_FULL_QUALITY_NONE];
pub(super) const V45_FULL_UC: &[UcPresetDefinition] = &[
    V5_FULL_UC_HEAVY,
    V45_FULL_UC_LIGHT,
    V5_FULL_UC_FURRYFOCUS,
    V5_FULL_UC_HUMANFOCUS,
    V5_FULL_UC_NONE,
];
pub(super) const V45_CURATED_QUALITY: &[QualityPresetDefinition] =
    &[V45_CURATED_QUALITY_STANDARD, V5_FULL_QUALITY_NONE];
pub(super) const V45_CURATED_UC: &[UcPresetDefinition] = &[
    V45_CURATED_UC_HEAVY,
    V45_CURATED_UC_LIGHT,
    V45_CURATED_UC_HUMANFOCUS,
    V5_FULL_UC_NONE,
];
pub(super) const V4_FULL_QUALITY: &[QualityPresetDefinition] =
    &[V4_FULL_QUALITY_STANDARD, V5_FULL_QUALITY_NONE];
pub(super) const V4_FULL_UC: &[UcPresetDefinition] =
    &[V4_FULL_UC_HEAVY, V4_FULL_UC_LIGHT, V5_FULL_UC_NONE];
pub(super) const V4_CURATED_QUALITY: &[QualityPresetDefinition] =
    &[V4_CURATED_QUALITY_STANDARD, V5_FULL_QUALITY_NONE];
pub(super) const V4_CURATED_UC: &[UcPresetDefinition] =
    &[V4_CURATED_UC_HEAVY, V4_CURATED_UC_LIGHT, V5_FULL_UC_NONE];
pub(super) const V3_QUALITY: &[QualityPresetDefinition] =
    &[V3_QUALITY_STANDARD, V5_FULL_QUALITY_NONE];
pub(super) const V3_UC: &[UcPresetDefinition] =
    &[V3_UC_HEAVY, V3_UC_LIGHT, V3_UC_HUMANFOCUS, V3_UC_NONE];
pub(super) const V3_FURRY_QUALITY: &[QualityPresetDefinition] =
    &[V3_FURRY_QUALITY_STANDARD, V5_FULL_QUALITY_NONE];
pub(super) const V3_FURRY_UC: &[UcPresetDefinition] =
    &[V3_FURRY_UC_HEAVY, V3_FURRY_UC_LIGHT, V3_UC_NONE];

pub(super) const V5_MEDIUM_UC: &[UcPresetDefinition] = &[V5_FULL_UC_HEAVY];
