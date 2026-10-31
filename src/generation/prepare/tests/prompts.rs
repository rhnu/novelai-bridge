use super::*;

#[test]
fn blank_base_prompts_can_be_prepared_and_encoded_under_both_policies() {
    for model in [
        Model::NaiDiffusion3,
        Model::NaiDiffusion45Full,
        Model::NaiDiffusion5Full,
    ] {
        for policy in [
            crate::ValidationPolicy::Normalize,
            crate::ValidationPolicy::Reject,
        ] {
            for prompt in ["", " \n\t"] {
                let prepared = GenerateImageRequest::builder(prompt, model)
                    .quality(crate::QualityPreset::None)
                    .negative_prompt(prompt)
                    .uc_preset(crate::UcPreset::None)
                    .seed(42)
                    .policy(policy)
                    .build()
                    .normalize()
                    .unwrap()
                    .prepare()
                    .unwrap();
                let body = payload(&prepared, GenerationDelivery::Images);
                assert_eq!(body["input"], prompt);
                if model == Model::NaiDiffusion3 {
                    assert_eq!(body["parameters"]["prompt"], prompt);
                } else {
                    assert_eq!(body["parameters"]["negative_prompt"], prompt);
                    assert_eq!(
                        body["parameters"]["v4_prompt"]["caption"]["base_caption"],
                        prompt
                    );
                    assert_eq!(
                        body["parameters"]["v4_negative_prompt"]["caption"]["base_caption"],
                        prompt
                    );
                }
            }
        }
    }
}

#[test]
fn blank_character_prompts_preserve_enabled_characters_and_negative_only_text() {
    for model in [Model::NaiDiffusion45Full, Model::NaiDiffusion5Full] {
        for policy in [
            crate::ValidationPolicy::Normalize,
            crate::ValidationPolicy::Reject,
        ] {
            for (positive, negative) in [("", ""), ("", "red"), ("cat", ""), (" \n\t", " \n\t")] {
                let prepared = GenerateImageRequest::builder("", model)
                    .characters(vec![Character {
                        prompt: positive.to_owned(),
                        negative_prompt: Some(negative.to_owned()),
                        position: CharacterPosition::default(),
                        enabled: true,
                    }])
                    .seed(42)
                    .policy(policy)
                    .build()
                    .normalize()
                    .unwrap()
                    .prepare()
                    .unwrap();
                let body = payload(&prepared, GenerationDelivery::Images);
                let parameters = &body["parameters"];
                assert_eq!(parameters["characterPrompts"].as_array().unwrap().len(), 1);
                assert_eq!(parameters["characterPrompts"][0]["prompt"], positive);
                assert_eq!(parameters["characterPrompts"][0]["uc"], negative);
                assert_eq!(
                    parameters["v4_prompt"]["caption"]["char_captions"][0]["char_caption"],
                    positive
                );
                assert_eq!(
                    parameters["v4_negative_prompt"]["caption"]["char_captions"][0]["char_caption"],
                    negative
                );
            }
        }
    }
}
