use crate::ImageSize;
use super::AnlasPricingContext;

/// Anlas cost per `/ai/encode-vibe` call.
pub const VIBE_ENCODE_COST: u32 = 2;

/// Estimates the cost of a director tool or upscale call.
///
/// `NovelAI` charges these from a separate pixel-bucket table; Opus covers inputs up to 409600
/// pixels. This helper is independent from the NAI 3, 4, 4.5, 5 image-generation formula.
#[must_use]
pub const fn estimate_director_tool_cost(size: ImageSize, context: AnlasPricingContext) -> u32 {
    let pixels = size.pixels();
    if context.tier >= 3 && context.subscription_active && pixels <= 409_600 {
        return 0;
    }
    if pixels <= 262_144 {
        1
    } else if pixels <= 409_600 {
        2
    } else if pixels <= 524_288 {
        3
    } else if pixels <= 786_432 {
        5
    } else {
        7
    }
}
