use super::*;
use crate::{PromptLexicalState, is_character_separator, is_text_rendering_marker};

fn modern() -> PromptSyntaxProfile {
    PromptSyntaxProfile {
        bracket_weights: true,
        numeric_weights: true,
        negative_weights: true,
        character_separators: true,
        randomizer: true,
        text_rendering: true,
        interaction_tags: true,
        legacy_mixing: false,
    }
}

fn text_weights(analysis: &PromptAnalysis) -> Vec<PromptWeight> {
    analysis.spans.iter().map(|span| span.weight).collect()
}

#[test]
fn unmatched_operators_continue_instead_of_forming_a_stack() {
    let analysis = analyze_prompt("}a]b[c{d", modern());
    assert_eq!(analysis.diagnostics, []);
    for (actual, expected) in text_weights(&analysis)
        .iter()
        .zip([1.0 / 1.05, 1.0, 1.0 / 1.05, 1.0])
    {
        let PromptWeight::Exact { value } = actual else {
            panic!("exact emphasis expected")
        };
        assert!((value - expected).abs() < 1e-12);
    }
}

#[test]
fn numerical_emphasis_and_bare_reset_clear_all_prior_emphasis() {
    let analysis = analyze_prompt("{{2::a{-3::b::c", modern());
    assert_eq!(
        text_weights(&analysis),
        vec![
            PromptWeight::Exact { value: 2.0 },
            PromptWeight::Exact { value: -3.0 },
            PromptWeight::ONE,
        ]
    );
}

#[test]
fn quotes_are_natural_language_and_escapes_are_lossless() {
    let source = "a \"{rain}\" \\{literal\\} \\| \\Text: 日本語 😀";
    let analysis = analyze_prompt(source, modern());
    assert_eq!(
        analysis
            .tokens
            .iter()
            .map(|token| token.text.as_str())
            .collect::<String>(),
        source
    );
    assert_eq!(
        analysis.tokens.last().unwrap().to as usize,
        source.encode_utf16().count()
    );
    assert_eq!(
        analysis
            .tokens
            .iter()
            .filter(|token| token.kind == PromptTokenKind::Strengthen)
            .count(),
        1
    );
    assert!(
        !analysis
            .tokens
            .iter()
            .any(|token| token.kind == PromptTokenKind::TextMarker)
    );
    for pair in analysis.tokens.windows(2) {
        assert_eq!(pair[0].to, pair[1].from);
    }
    let emoji = analyze_prompt("😀{x", modern());
    assert_eq!((emoji.tokens[1].from, emoji.tokens[1].to), (2, 3));
}

#[test]
fn random_branches_join_weights_for_following_text_and_reset_restores_exactness() {
    let analysis = analyze_prompt("||1.5::|0.5::|::||rain::sun", modern());
    assert_eq!(analysis.diagnostics, []);
    assert_eq!(
        text_weights(&analysis),
        vec![
            PromptWeight::Range { min: 0.5, max: 1.5 },
            PromptWeight::ONE
        ]
    );
}

#[test]
fn every_random_branch_starts_with_the_same_incoming_weight() {
    let analysis = analyze_prompt("{||a{b|c||d", modern());
    let weights = text_weights(&analysis);
    assert_eq!(weights[0], PromptWeight::Exact { value: 1.05 });
    assert_eq!(weights[2], PromptWeight::Exact { value: 1.05 });
    let PromptWeight::Range { min, max } = weights[3] else {
        panic!("branch range expected")
    };
    assert!((min - 1.05).abs() < 1e-12);
    assert!((max - 1.1025).abs() < 1e-12);
}

#[test]
fn incomplete_randomizer_remains_lossless_and_reports_typed_warning() {
    let analysis = analyze_prompt("||cat|", modern());
    assert_eq!(
        analysis
            .tokens
            .iter()
            .map(|token| token.text.as_str())
            .collect::<String>(),
        "||cat|"
    );
    assert_eq!(
        analysis.diagnostics[0].code,
        PromptDiagnosticCode::UnclosedRandomizer
    );
    assert_eq!(
        (analysis.diagnostics[0].from, analysis.diagnostics[0].to),
        (0, 6)
    );
}

#[test]
fn text_rendering_is_literal_until_the_next_character_field() {
    let analysis = analyze_prompt("Text: hi { /* // 2:: ||x|| \\| bye |source#hug", modern());
    assert_eq!(analysis.diagnostics, []);
    assert!(!analysis.tokens.iter().any(|token| matches!(
        token.kind,
        PromptTokenKind::Strengthen | PromptTokenKind::NumericWeight
    )));
    assert_eq!(
        analysis
            .tokens
            .iter()
            .filter(|token| token.kind == PromptTokenKind::CharacterSeparator)
            .count(),
        1
    );
    assert_eq!(analysis.tokens.last().unwrap().kind, PromptTokenKind::Text);
    assert!(
        analysis
            .tokens
            .iter()
            .any(|token| token.kind == PromptTokenKind::Interaction)
    );
    assert!(
        analysis
            .spans
            .iter()
            .any(|span| span.kind == PromptSemanticKind::RenderedText)
    );
}

#[test]
fn shared_marker_helpers_distinguish_boundaries_and_capabilities() {
    assert!(is_text_rendering_marker("Text: hello", None, modern()));
    assert!(is_text_rendering_marker("Text: hello", Some(','), modern()));
    assert!(!is_text_rendering_marker(
        "Text: hello",
        Some('a'),
        modern()
    ));
    assert!(is_text_rendering_marker("text: hello", None, modern()));
    assert!(!is_text_rendering_marker("Text:: hello", None, modern()));
    assert!(!is_text_rendering_marker(
        "Text: hello",
        None,
        PromptSyntaxProfile::default()
    ));
    assert!(is_character_separator("|girl", modern()));
    assert!(!is_character_separator("||red|blue||", modern()));
}

#[test]
fn v3_mixing_suffix_and_modern_character_pipes_have_distinct_roles() {
    let legacy = PromptSyntaxProfile {
        legacy_mixing: true,
        randomizer: true,
        ..Default::default()
    };
    let analysis = analyze_prompt("cat:0.2 |frog:-0.5", legacy);
    assert_eq!(
        analysis
            .tokens
            .iter()
            .filter(|token| token.kind == PromptTokenKind::MixWeight)
            .map(|token| token.value)
            .collect::<Vec<_>>(),
        vec![Some(0.2), Some(-0.5)]
    );
    assert!(
        analysis
            .tokens
            .iter()
            .any(|token| token.kind == PromptTokenKind::MixSeparator)
    );
    let analysis = analyze_prompt("{scene|girl", modern());
    assert_eq!(text_weights(&analysis).last(), Some(&PromptWeight::ONE));
    assert!(
        analysis
            .tokens
            .iter()
            .any(|token| token.kind == PromptTokenKind::CharacterSeparator)
    );
}

#[test]
fn unsupported_numeric_forms_warn_without_inventing_supported_effects() {
    let v4 = PromptSyntaxProfile {
        negative_weights: false,
        ..modern()
    };
    let analysis = analyze_prompt("-2::rain", v4);
    assert_eq!(
        analysis.diagnostics[0].code,
        PromptDiagnosticCode::UnsupportedNegativeWeight
    );
    assert_eq!(text_weights(&analysis), vec![PromptWeight::ONE]);
    let analysis = analyze_prompt(
        "2::rain",
        PromptSyntaxProfile {
            bracket_weights: true,
            ..Default::default()
        },
    );
    assert_eq!(
        analysis.diagnostics[0].code,
        PromptDiagnosticCode::UnsupportedNumericWeight
    );
}

#[test]
fn excessive_numeric_values_are_unknown_and_remain_serializable() {
    let source = format!("{}::rain::sun", "9".repeat(400));
    let analysis = analyze_prompt(&source, modern());
    assert_eq!(
        text_weights(&analysis),
        vec![PromptWeight::Unknown, PromptWeight::ONE]
    );
    assert_eq!(
        analysis.diagnostics[0].code,
        PromptDiagnosticCode::NonFiniteWeight
    );
    let encoded = serde_json::to_string(&analysis).unwrap();
    let decoded: PromptAnalysis = serde_json::from_str(&encoded).unwrap();
    assert_eq!(analysis, decoded);
}

#[test]
fn serde_and_optional_typescript_have_the_same_camel_case_contract() {
    let profile = serde_json::to_value(modern()).unwrap();
    assert_eq!(profile["numericWeights"], true);
    assert!(profile.get("numeric_weights").is_none());
    let weight = serde_json::to_value(PromptWeight::Range { min: 0.5, max: 2.0 }).unwrap();
    assert_eq!(weight["kind"], "range");
    #[cfg(feature = "typegen")]
    {
        use ts_rs::{Config, TS};
        assert!(PromptAnalysis::decl(&Config::default()).contains("PromptToken"));
        assert!(PromptSyntaxProfile::decl(&Config::default()).contains("numericWeights"));
    }
}

#[test]
fn plain_profile_preserves_native_looking_text_without_diagnostics() {
    let source = "{a] 2::b :: ||cat|dog|| Text: hi";
    let analysis = analyze_prompt(source, PromptSyntaxProfile::default());
    assert_eq!(analysis.diagnostics, []);
    assert_eq!(analysis.tokens.len(), 1);
    assert_eq!(analysis.tokens[0].text, source);
    assert_eq!(analysis.tokens[0].kind, PromptTokenKind::Text);
    assert_eq!(analysis.spans[0].weight, PromptWeight::ONE);
}

#[test]
fn randomizer_rendering_branches_close_before_conservative_merging() {
    let analysis = analyze_prompt("||Text: hello|bye||rest", modern());
    assert!(
        !analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == PromptDiagnosticCode::UnclosedRandomizer)
    );
    assert_eq!(
        analysis.tokens.last().unwrap().kind,
        PromptTokenKind::UncertainText
    );
    assert_eq!(analysis.spans.last().unwrap().weight, PromptWeight::Unknown);
    assert_eq!(
        analysis.diagnostics[0].code,
        PromptDiagnosticCode::UncertainTextRendering
    );
    let all_rendered = analyze_prompt("||Text: hello|text: bye||{rest}", modern());
    assert_eq!(all_rendered.diagnostics, []);
    assert_eq!(
        all_rendered.tokens.last().unwrap().kind,
        PromptTokenKind::RenderedText
    );
}

#[test]
fn shared_lexical_state_is_the_same_cursor_contract_used_by_analysis() {
    let source = "||Text: hello|bye||rest|Text: \\|end";
    let mut state = PromptLexicalState::default();
    let mut cursor = 0;
    let mut previous = None;
    while cursor < source.len() {
        let length = state.advance(&source[cursor..], previous, modern());
        assert!(length > 0);
        previous = source[cursor..cursor + length].chars().next_back();
        cursor += length;
        if cursor == "||Text: hello|bye||".len() {
            assert!(state.uncertain_rendering());
            assert!(state.rendering());
            assert!(!state.in_randomizer());
        }
    }
    assert!(state.rendering());
    assert!(!state.uncertain_rendering());
    assert_eq!(state.advance("", previous, modern()), 0);
}
