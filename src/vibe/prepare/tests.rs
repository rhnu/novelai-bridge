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
            policy: if strict {
                crate::ValidationPolicy::Reject
            } else {
                crate::ValidationPolicy::Normalize
            },
        }
        .prepare();
        assert_eq!(prepared.is_ok(), valid);
        if let Ok(prepared) = prepared {
            let body: serde_json::Value =
                serde_json::from_slice(&prepared.encode().unwrap()).unwrap();
            assert_eq!(prepared.model(), model);
            assert_eq!(body["model"], model.as_str());
            assert_eq!(body["information_extracted"], 1.0);
            assert_eq!(prepared.information_extracted(), 1.0);
            assert_eq!(
                prepared.adjustments(),
                &[crate::ValidationAdjustment {
                    field: crate::RequestField::VibeInformationExtracted,
                    from: 2.0,
                    to: 1.0
                }]
            );
        }
    }
}

#[test]
fn removed_strict_policy_cannot_silently_downgrade_vibe_validation() {
    let request = EncodeVibeRequest {
        image: image(),
        model: Model::NaiDiffusion45Full,
        information_extracted: 0.5,
        policy: crate::ValidationPolicy::Reject,
    };
    let mut value = serde_json::to_value(request).unwrap();
    value.as_object_mut().unwrap().remove("policy");
    value["strict_mode"] = true.into();
    assert!(serde_json::from_value::<EncodeVibeRequest>(value).is_err());
}
