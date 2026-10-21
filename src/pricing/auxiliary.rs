use super::{AnlasEstimateInput, AnlasPricingContext, estimate_anlas_cost};
use crate::{DirectorCommand, ImageSize, Model};

/// Anlas cost per `/ai/encode-vibe` call.
pub const VIBE_ENCODE_COST: u32 = 2;

/// Estimates a Director Tools command from its effective processing dimensions.
///
/// Use [`crate::PreparedDirector::image_size`] or [`crate::ImagePreparation::director`] to
/// select the size used by request preparation. Ordinary tools use V3 generation at 28 steps;
/// background removal has three outputs and a fixed surcharge, without the Opus discount.
#[must_use]
pub fn estimate_director_tool_cost(
    size: ImageSize,
    command: &DirectorCommand,
    context: AnlasPricingContext,
) -> u32 {
    let background_removal = matches!(command, DirectorCommand::BgRemoval);
    let base = estimate_anlas_cost(AnlasEstimateInput {
        model: Model::NaiDiffusion3,
        size,
        steps: 28,
        n_samples: 1,
        context: if background_removal {
            AnlasPricingContext::default()
        } else {
            context
        },
        ..AnlasEstimateInput::default()
    })
    .total;
    if background_removal {
        base.saturating_mul(3).saturating_add(5)
    } else {
        base
    }
}
