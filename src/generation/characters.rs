use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CharacterPosition {
    /// Horizontal position in the `[0, 1]` range.
    pub x: f32,
    /// Vertical position in the `[0, 1]` range.
    pub y: f32,
}

impl Default for CharacterPosition {
    fn default() -> Self {
        Self { x: 0.5, y: 0.5 }
    }
}

/// Character prompt input used by V4 and later models.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Character {
    /// Positive prompt for the character.
    pub prompt: String,
    /// Optional negative prompt for the character.
    pub negative_prompt: Option<String>,
    /// Desired character position.
    pub position: CharacterPosition,
    /// Whether this character is enabled.
    pub enabled: bool,
}

/// Character prompts and their positioning behavior.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct CharacterSet {
    /// Character prompts sent with the request.
    pub characters: Vec<Character>,
    /// Optional override for `use_coords`.
    ///
    /// When `None`, coordinates are enabled only if an enabled character sits off-center.
    pub use_coords: Option<bool>,
}

impl CharacterSet {
    /// Creates a set from character prompts, letting the client resolve `use_coords`.
    #[must_use]
    pub const fn new(characters: Vec<Character>) -> Self {
        Self {
            characters,
            use_coords: None,
        }
    }

    /// Returns the enabled characters.
    pub fn enabled(&self) -> impl Iterator<Item = &Character> {
        self.characters.iter().filter(|character| character.enabled)
    }

    /// Returns the number of enabled characters.
    #[must_use]
    pub fn enabled_count(&self) -> usize {
        self.enabled().count()
    }
}

pub(super) fn is_center_position(value: f32) -> bool {
    (value - 0.5).abs() <= 0.0001
}

