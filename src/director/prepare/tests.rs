use crate::generation::test_support::image;
use crate::director::{DirectorCommand, RunDirectorToolRequest};

#[test]
fn operation_normalizes_before_encoding_and_exposes_effective_command() {
    let prepared = RunDirectorToolRequest { image: image(), command: DirectorCommand::Colorize { prompt: Some("  blue  ".to_owned()), defry: 9 }, strict_mode: false }.prepare().unwrap();
    assert!(matches!(prepared.command(), DirectorCommand::Colorize { prompt: Some(prompt), defry: 5 } if prompt == "blue"));
    let body: serde_json::Value = serde_json::from_slice(&prepared.encode().unwrap()).unwrap();
    assert_eq!(body["req_type"], "colorize");
    assert_eq!(body["defry"], 5);
    assert_eq!(body["prompt"], "blue");
    assert_eq!(body["width"], prepared.image_size().width);
}

#[test]
fn emotion_requires_text_and_strict_defry_rejects_out_of_range_values() {
    for (command, strict) in [
        (DirectorCommand::Emotion { prompt: "   ".to_owned(), defry: 0 }, false),
        (DirectorCommand::Colorize { prompt: None, defry: 6 }, true),
    ] {
        assert!(RunDirectorToolRequest { image: image(), command, strict_mode: strict }.prepare().is_err());
    }
}

#[test]
fn parameterless_command_omits_prompt_and_defry() {
    let prepared = RunDirectorToolRequest { image: image(), command: DirectorCommand::BgRemoval, strict_mode: true }.prepare().unwrap();
    let body: serde_json::Value = serde_json::from_slice(&prepared.encode().unwrap()).unwrap();
    assert_eq!(body["req_type"], "bg-removal");
    assert!(body.get("prompt").is_none());
    assert!(body.get("defry").is_none());
}
