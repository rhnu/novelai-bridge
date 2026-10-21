use super::*;

#[test]
fn query_encoding_preserves_unicode_and_reserved_characters_as_one_parameter() {
    let input = "猫 &key=value+#/?";
    let mut request = SuggestTagsRequest::new(input, Model::NaiDiffusion5Full);
    request.language = TagLanguage::Japanese;
    let query = request.query_string();
    assert!(query.contains("prompt=%E7%8C%AB+%26key%3Dvalue%2B%23%2F%3F"));
    let parsed = form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect::<Vec<_>>();
    assert_eq!(
        parsed,
        vec![
            ("model".to_owned(), "nai-diffusion-5-full".to_owned()),
            ("prompt".to_owned(), input.to_owned()),
            ("lang".to_owned(), "jp".to_owned())
        ]
    );
    assert!(!format!("{request:?}").contains(input));
}

#[test]
fn empty_prefixes_and_language_defaults_follow_the_query_schema() {
    let request = SuggestTagsRequest::new("", Model::NaiDiffusion3);
    assert!(request.query_string().contains("prompt=&lang=en"));
    assert_eq!(
        serde_json::from_str::<TagLanguage>("\"jp\"").unwrap(),
        TagLanguage::Japanese
    );
    assert!(serde_json::from_str::<TagLanguage>("\"ja\"").is_err());
}

#[test]
fn response_uses_the_observed_array_shape_and_retains_frequency_and_confidence() {
    let response: SuggestTagsResult = serde_json::from_str(
        r#"{"tags":[{"tag":"cat","count":4500000000,"confidence":0.75,"extension":true}]}"#,
    )
    .unwrap();
    assert_eq!(
        response,
        SuggestTagsResult::English {
            tags: vec![TagSuggestion {
                tag: "cat".to_owned(),
                count: 4_500_000_000,
                confidence: 0.75
            }]
        }
    );
    assert!(
        serde_json::from_str::<SuggestTagsResult>(
            r#"{"tags":{"tag":"cat","count":1,"confidence":1}}"#
        )
        .is_err()
    );
}

#[test]
fn japanese_responses_preserve_the_observed_translation_and_power_fields() {
    // Representative entries from the live JP response, without copying the complete result set.
    let body = r#"[{"jp_tag":"猫","en_tag":"cat","power":10000},
        {"jp_tag":"猫又おかゆ(通常)","en_tag":"nekomata okayu (1st costume)","power":7296,"extension":true}]"#;
    let response: SuggestTagsResult = serde_json::from_str(body).unwrap();
    assert_eq!(response.len(), 2);
    assert!(!response.is_empty());
    let SuggestTagsResult::Japanese(tags) = response else {
        panic!("the JP root array must preserve its language-specific shape");
    };
    assert_eq!(
        tags[0],
        JapaneseTagSuggestion {
            jp_tag: "猫".to_owned(),
            en_tag: "cat".to_owned(),
            power: 10000
        }
    );
    assert_eq!(tags[1].power, 7296);
    assert_eq!(tags[1].en_tag, "nekomata okayu (1st costume)");
    let encoded = serde_json::to_value(SuggestTagsResult::Japanese(tags)).unwrap();
    assert!(encoded.is_array());
    assert!(encoded[0].get("confidence").is_none());
    assert!(encoded[0].get("count").is_none());
}

#[test]
fn empty_result_shapes_remain_distinct_and_report_no_suggestions() {
    for body in [r#"{"tags":[]}"#, "[]"] {
        let response: SuggestTagsResult = serde_json::from_str(body).unwrap();
        assert_eq!(response.len(), 0);
        assert!(response.is_empty());
        assert_eq!(
            serde_json::to_value(&response).unwrap(),
            serde_json::from_str::<serde_json::Value>(body).unwrap()
        );
    }
}
