use crate::{PromptSyntaxProfile, PromptToken, PromptTokenKind};

/// Tests an unescaped rendering marker at a native prompt boundary.
///
/// `rest` starts at the cursor; `previous` is the preceding Unicode scalar, if any.
/// Field/alternative boundaries act as the beginning of their own prompt.
#[must_use]
pub fn is_text_rendering_marker(
    rest: &str,
    previous: Option<char>,
    profile: PromptSyntaxProfile,
) -> bool {
    profile.text_rendering
        && rest
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("text:"))
        && !rest[5..].starts_with(':')
        && previous.is_none_or(|ch| {
            ch.is_whitespace()
                || matches!(
                    ch,
                    ',' | '.' | ':' | '[' | ']' | '{' | '}' | '、' | '。' | '|'
                )
        })
}

/// Tests an unescaped modern field separator; double bars delimit randomizers.
#[must_use]
pub fn is_character_separator(rest: &str, profile: PromptSyntaxProfile) -> bool {
    profile.character_separators && rest.starts_with('|') && !rest.starts_with("||")
}

/// Tests an unescaped double-bar delimiter before rendering is interpreted.
#[must_use]
pub fn is_randomizer_delimiter(rest: &str, profile: PromptSyntaxProfile) -> bool {
    profile.randomizer && rest.starts_with("||")
}

/// Shared native lexical state for extension scanners that skip their own comments.
///
/// Randomizers are structural before text rendering. Alternatives retain independent
/// rendering state; a mixed result protects subsequent text conservatively.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PromptLexicalState {
    mode: RenderingMode,
    random: Option<RandomRendering>,
}

impl PromptLexicalState {
    /// Consumes one native lexeme and returns its UTF-8 byte length (zero at EOF).
    ///
    /// Skip extension comments before committing the resulting state. The state is Copy,
    /// so an extension scanner may inspect a candidate step first. `previous` refers to
    /// the preceding source scalar; escaped characters are consumed together here.
    pub fn advance(
        &mut self,
        rest: &str,
        previous: Option<char>,
        profile: PromptSyntaxProfile,
    ) -> usize {
        self.next(rest, previous, profile).length
    }

    /// Whether text is certainly or possibly in a terminal text-rendering region.
    #[must_use]
    pub const fn rendering(&self) -> bool { !matches!(self.mode, RenderingMode::Prompt) }

    /// Whether alternatives disagree about rendering subsequent source literally.
    #[must_use]
    pub const fn uncertain_rendering(&self) -> bool {
        matches!(self.mode, RenderingMode::Uncertain)
    }

    /// Whether the cursor is inside a randomizer alternative.
    #[must_use]
    pub const fn in_randomizer(&self) -> bool { self.random.is_some() }

    const fn text_kind(self) -> PromptTokenKind {
        match self.mode {
            RenderingMode::Prompt => PromptTokenKind::Text,
            RenderingMode::Rendered => PromptTokenKind::RenderedText,
            RenderingMode::Uncertain => PromptTokenKind::UncertainText,
        }
    }

    fn next(&mut self, rest: &str, previous: Option<char>, profile: PromptSyntaxProfile) -> Lexeme {
        use PromptTokenKind as K;
        let Some(first) = rest.chars().next() else {
            return Lexeme::new(0, K::Text);
        };
        if let Some(escaped) = rest.strip_prefix('\\') {
            return Lexeme::new(
                1 + escaped.chars().next().map_or(0, char::len_utf8),
                if self.rendering() {
                    self.text_kind()
                } else {
                    K::Escape
                },
            );
        }
        if is_randomizer_delimiter(rest, profile) {
            let kind = if let Some(branch) = self.random.take() {
                self.mode = branch.joined.map_or(self.mode, |mode| mode.join(self.mode));
                K::RandomClose
            } else {
                self.random = Some(RandomRendering {
                    entry: self.mode,
                    joined: None,
                });
                K::RandomOpen
            };
            return Lexeme::new(2, kind);
        }
        if first == '|' && self.random.is_some() {
            if let Some(branch) = &mut self.random {
                branch.joined = Some(branch.joined.map_or(self.mode, |mode| mode.join(self.mode)));
                self.mode = branch.entry;
            }
            return Lexeme::new(1, K::RandomSeparator);
        }
        if is_character_separator(rest, profile) {
            self.mode = RenderingMode::Prompt;
            return Lexeme::new(1, K::CharacterSeparator);
        }
        if self.rendering() {
            return Lexeme::new(first.len_utf8(), self.text_kind());
        }
        if is_text_rendering_marker(rest, previous, profile) {
            self.mode = RenderingMode::Rendered;
            return Lexeme::new(5, K::TextMarker);
        }
        if rest.starts_with("||") {
            return Lexeme::new(2, K::Text);
        }
        if first == '|' && profile.legacy_mixing {
            return Lexeme::new(1, K::MixSeparator);
        }
        if profile.bracket_weights {
            match first {
                '{' | ']' => return Lexeme::new(1, K::Strengthen),
                '}' | '[' => return Lexeme::new(1, K::Weaken),
                _ => {},
            }
        }
        if profile.bracket_weights || profile.numeric_weights {
            if rest.starts_with("::") {
                return Lexeme::new(2, K::WeightReset);
            }
            if previous.is_none_or(|ch| !ch.is_ascii_digit() && ch != '.') {
                let end = number_end(rest);
                if end > 0 && rest[end..].starts_with("::") {
                    return Lexeme {
                        length: end + 2,
                        kind: K::NumericWeight,
                        value: finite_number(&rest[..end]),
                    };
                }
            }
        }
        if profile.legacy_mixing && first == ':' {
            let end = number_end(&rest[1..]);
            if end > 0 {
                let after = rest[1 + end..].trim_start_matches(char::is_whitespace);
                if after.is_empty() || after.starts_with('|') {
                    return Lexeme {
                        length: 1 + end,
                        kind: K::MixWeight,
                        value: finite_number(&rest[1..=end]),
                    };
                }
            }
        }
        if profile.interaction_tags
            && previous.is_none_or(|ch| {
                ch.is_whitespace() || matches!(ch, ',' | '|' | '{' | '[' | '}' | ']' | ':')
            })
        {
            for prefix in ["source#", "target#", "mutual#"] {
                if rest.starts_with(prefix) {
                    return Lexeme::new(prefix.len(), K::Interaction);
                }
            }
        }
        Lexeme::new(first.len_utf8(), K::Text)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum RenderingMode {
    #[default]
    Prompt,
    Rendered,
    Uncertain,
}

impl RenderingMode {
    const fn join(self, other: Self) -> Self {
        match (self, other) {
            (Self::Prompt, Self::Prompt) => Self::Prompt,
            (Self::Rendered, Self::Rendered) => Self::Rendered,
            _ => Self::Uncertain,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RandomRendering {
    entry: RenderingMode,
    joined: Option<RenderingMode>,
}

struct Lexeme {
    length: usize,
    kind: PromptTokenKind,
    value: Option<f64>,
}

impl Lexeme {
    const fn new(length: usize, kind: PromptTokenKind) -> Self {
        Self {
            length,
            kind,
            value: None,
        }
    }
}

/// Groups adjacent text without reparsing prefixes or allocating per-character strings.
pub fn tokenize(source: &str, profile: PromptSyntaxProfile) -> Vec<PromptToken> {
    let mut tokens = Vec::new();
    let mut state = PromptLexicalState::default();
    let mut cursor = 0;
    let mut position = 0u32;
    let mut previous = None;
    let mut pending: Option<(usize, PromptTokenKind)> = None;
    while cursor < source.len() {
        let lexeme = state.next(&source[cursor..], previous, profile);
        let is_text = matches!(
            lexeme.kind,
            PromptTokenKind::Text | PromptTokenKind::RenderedText | PromptTokenKind::UncertainText
        );
        if pending.is_some_and(|(_, kind)| !is_text || kind != lexeme.kind)
            && let Some((start, kind)) = pending.take()
        {
            push(
                &mut tokens,
                &mut position,
                &source[start..cursor],
                kind,
                None,
            );
        }
        if is_text {
            pending.get_or_insert((cursor, lexeme.kind));
        } else {
            push(
                &mut tokens,
                &mut position,
                &source[cursor..cursor + lexeme.length],
                lexeme.kind,
                lexeme.value,
            );
        }
        previous = source[cursor..cursor + lexeme.length].chars().next_back();
        cursor += lexeme.length;
    }
    if let Some((start, kind)) = pending {
        push(&mut tokens, &mut position, &source[start..], kind, None);
    }
    tokens
}

fn push(
    tokens: &mut Vec<PromptToken>,
    position: &mut u32,
    text: &str,
    kind: PromptTokenKind,
    value: Option<f64>,
) {
    let from = *position;
    *position = position.saturating_add(text.encode_utf16().count().try_into().unwrap_or(u32::MAX));
    tokens.push(PromptToken {
        from,
        to: *position,
        text: text.to_owned(),
        kind,
        value,
    });
}

fn number_end(source: &str) -> usize {
    let bytes = source.as_bytes();
    let mut cursor = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let start = cursor;
    while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
        cursor += 1;
    }
    let mut digits = cursor - start;
    if bytes.get(cursor) == Some(&b'.') {
        cursor += 1;
        let start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        digits += cursor - start;
    }
    if digits == 0 { 0 } else { cursor }
}

fn finite_number(source: &str) -> Option<f64> {
    source.parse::<f64>().ok().filter(|value| value.is_finite())
}
