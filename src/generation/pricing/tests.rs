use crate::{
    AnlasPricingContext, Model, PricingMode,
    generation::{
        GenerateImageRequest, GenerationInput, ImageToImageInput, InpaintImageToImage,
        InpaintingInput, test_support::image,
    },
};

#[test]
fn normalized_quote_basis_contains_one_effective_operation() {
    let mut request = GenerateImageRequest::builder("cat", Model::NaiDiffusion45Full).build();
    request.operation = GenerationInput::ImageToImage(ImageToImageInput::new(image(), 2.0, 0.0));
    let normalized = request.normalize().unwrap();
    let basis = normalized.anlas_estimate_input(AnlasPricingContext::default());
    assert_eq!(basis.mode, PricingMode::ImageToImage { strength: 0.99 });
    assert_eq!(basis.pending_vibe_encodes, 0);
    assert_eq!(
        normalized.estimate_anlas_cost(AnlasPricingContext::default()),
        crate::estimate_anlas_cost(basis)
    );
}

#[test]
fn inpainting_quote_and_payload_share_the_effective_nested_strength() {
    for (conditioning, expected) in [
        (None, 1.0),
        (Some(InpaintImageToImage::new(0.5, 0.0)), 0.5),
        (Some(InpaintImageToImage::new(1.2, 0.0)), 1.0),
    ] {
        let mut input = InpaintingInput::new(image(), image());
        input.img2img = conditioning;
        let plan = GenerateImageRequest::builder("cat", Model::NaiDiffusion45Full)
            .seed(42)
            .inpainting(input)
            .build()
            .normalize()
            .unwrap();
        let basis = plan.anlas_estimate_input(AnlasPricingContext::default());
        assert_eq!(basis.mode, PricingMode::Inpainting { strength: expected });
        let prepared = plan.prepare().unwrap();
        let body: serde_json::Value =
            serde_json::from_slice(&prepared.encode(crate::GenerationDelivery::Images).unwrap())
                .unwrap();
        let sent = body["parameters"]
            .get("img2img")
            .map_or(1.0, |parameters| parameters["strength"].as_f64().unwrap());
        assert!((sent - f64::from(expected)).abs() < f64::EPSILON);
        assert_eq!(
            prepared
                .normalized()
                .anlas_estimate_input(AnlasPricingContext::default()),
            basis
        );
    }
}
