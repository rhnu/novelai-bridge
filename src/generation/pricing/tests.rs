use crate::{AnlasPricingContext, Model, PricingMode};
use crate::generation::{GenerateImageRequest, Img2ImgRequest};
use crate::generation::test_support::image;

#[test]
fn normalized_quote_basis_contains_one_effective_operation() {
    let mut request = GenerateImageRequest::builder("cat", Model::NaiDiffusion45Full).build();
    request.img2img = Some(Img2ImgRequest::new(image(), 2.0, 0.0));
    let normalized = request.normalize().unwrap();
    let basis = normalized.anlas_estimate_input(AnlasPricingContext::default());
    assert_eq!(basis.mode, PricingMode::ImageToImage { strength: 0.99 });
    assert_eq!(basis.pending_vibe_encodes, 0);
    assert_eq!(normalized.estimate_anlas_cost(AnlasPricingContext::default()), crate::estimate_anlas_cost(basis));
}
