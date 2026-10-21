use crate::{EncodeVibeRequest, Model, generation::test_support::image};

#[test]
fn encoding_preparation_validates_capability_and_numeric_policy() {
    for (model, strength, strict, valid) in [
        (Model::NaiDiffusion45Full, 2.0, false, true),
        (Model::NaiDiffusion45Full, 2.0, true, false),
        (Model::NaiDiffusion45Full, f32::NAN, false, false),
        (Model::NaiDiffusion3, 0.5, false, false),
        (Model::NaiDiffusion5Full, 0.5, false, false),
    ] {
        let prepared = EncodeVibeRequest {
            image: image(),
            information_extracted: strength,
            model,
            strict_mode: strict,
        }
        .prepare();
        assert_eq!(prepared.is_ok(), valid);
        if let Ok(prepared) = prepared {
            let body: serde_json::Value =
                serde_json::from_slice(&prepared.encode().unwrap()).unwrap();
            assert_eq!(prepared.model(), model);
            assert_eq!(body["model"], model.as_str());
            assert_eq!(body["information_extracted"], 1.0);
        }
    }
}
