use crate::{
    director::{DirectorCommand, RunDirectorToolRequest},
    generation::test_support::image,
};

#[test]
fn operation_normalizes_before_encoding_and_exposes_effective_command() {
    let prepared = RunDirectorToolRequest {
        image: image(),
        command: DirectorCommand::Colorize {
            prompt: Some("  blue  ".to_owned()),
            defry: 9,
        },
        policy: crate::ValidationPolicy::Normalize,
    }
    .prepare()
    .unwrap();
    assert!(
        matches!(prepared.command(), DirectorCommand::Colorize { prompt: Some(prompt), defry: 5 } if prompt == "blue")
    );
    let body: serde_json::Value = serde_json::from_slice(&prepared.encode().unwrap()).unwrap();
    assert_eq!(body["req_type"], "colorize");
    assert_eq!(body["defry"], 5);
    assert_eq!(body["prompt"], "blue");
    assert_eq!(body["width"], prepared.image_size().width);
    assert_eq!(
        prepared.adjustments(),
        &[crate::ValidationAdjustment {
            field: crate::RequestField::DirectorDefry,
            from: 9.0,
            to: 5.0
        }]
    );
    assert!(!format!("{prepared:?}").contains("blue"));
}

#[test]
fn emotion_requires_text_and_strict_defry_rejects_out_of_range_values() {
    for (command, strict) in [
        (
            DirectorCommand::Emotion {
                prompt: "   ".to_owned(),
                defry: 0,
            },
            false,
        ),
        (
            DirectorCommand::Colorize {
                prompt: None,
                defry: 6,
            },
            true,
        ),
    ] {
        assert!(
            RunDirectorToolRequest {
                image: image(),
                command,
                policy: if strict {
                    crate::ValidationPolicy::Reject
                } else {
                    crate::ValidationPolicy::Normalize
                }
            }
            .prepare()
            .is_err()
        );
    }
}

#[test]
fn parameterless_command_omits_prompt_and_defry() {
    let prepared = RunDirectorToolRequest {
        image: image(),
        command: DirectorCommand::BgRemoval,
        policy: crate::ValidationPolicy::Reject,
    }
    .prepare()
    .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&prepared.encode().unwrap()).unwrap();
    assert_eq!(body["req_type"], "bg-removal");
    assert!(body.get("prompt").is_none());
    assert!(body.get("defry").is_none());
}

#[test]
fn removed_policy_and_invalid_command_fields_cannot_be_silently_ignored() {
    let request = RunDirectorToolRequest {
        image: image(),
        command: DirectorCommand::BgRemoval,
        policy: crate::ValidationPolicy::Reject,
    };
    let mut value = serde_json::to_value(request).unwrap();
    value.as_object_mut().unwrap().remove("policy");
    value["strict_mode"] = true.into();
    assert!(serde_json::from_value::<RunDirectorToolRequest>(value).is_err());
    assert!(
        serde_json::from_value::<DirectorCommand>(
            serde_json::json!({"colorize":{"prompt":null,"defry":1,"noise":0.5}})
        )
        .is_err()
    );
}
