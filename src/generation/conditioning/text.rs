use super::ResolvedCharacterPrompt;
use crate::PromptSyntaxProfile;

/// Byte offset of an explicit rendering block, including a caller's mixed-case marker.
pub(super) fn marker(text: &str, syntax: PromptSyntaxProfile) -> Option<usize> {
    text.match_indices(['t', 'T']).find_map(|(offset, _)| {
        crate::is_text_rendering_marker(&text[offset..], text[..offset].chars().next_back(), syntax)
            .then_some(offset)
    })
}

/// End of the base field in the inline character syntax. A randomizer owns its internal bars.
pub(super) fn base_end(text: &str) -> usize {
    let mut randomizer = false;
    let mut bars = text.match_indices('|').peekable();
    while let Some((offset, _)) = bars.next() {
        if bars.next_if(|(next, _)| *next == offset + 1).is_some() {
            randomizer = !randomizer;
        } else if !randomizer {
            return offset;
        }
    }
    text.len()
}

/// Builds the V5 automatic rendering block from the base prompt and active character captions.
pub(super) fn rendering_block(
    prompt: &str,
    characters: &[ResolvedCharacterPrompt],
    use_coords: bool,
    syntax: PromptSyntaxProfile,
) -> Option<String> {
    if marker(prompt, syntax).is_some()
        || characters
            .iter()
            .any(|character| marker(&character.prompt, syntax).is_some())
    {
        return None;
    }
    let mut ordered: Vec<_> = characters.iter().filter(|c| !c.prompt.is_empty()).collect();
    if use_coords {
        ordered.sort_by(|a, b| a.position.y.total_cmp(&b.position.y));
        order_rows(&mut ordered);
    }
    let mut groups: Vec<_> = std::iter::once(&prompt[..base_end(prompt)])
        .chain(ordered.iter().map(|c| c.prompt.as_str()))
        .map(quoted_text)
        .filter(|quotes| !quotes.is_empty())
        .collect();
    if groups.is_empty() {
        return None;
    }
    let (mut cjk, mut units) = (0_usize, 0_usize);
    for ch in groups.iter().flatten().flat_map(|quote| quote.chars()) {
        units += ch.len_utf16();
        cjk += usize::from(
            matches!(ch, '\u{3000}'..='\u{30ff}' | '\u{3400}'..='\u{4dbf}'
            | '\u{4e00}'..='\u{9faf}' | '\u{ff00}'..='\u{ff9f}'),
        );
    }
    // The web client reverses dialogue order within each caption for predominantly CJK text.
    if cjk > units.saturating_mul(3) / 10 {
        for group in &mut groups {
            group.reverse();
        }
    }
    let quotes: Vec<_> = groups.into_iter().flatten().collect();
    Some(format!("teXt: {}", quotes.join("\n\n")))
}

/// Split the vertically sorted captions at the widest gap, then read each row left to right.
fn order_rows(characters: &mut [&ResolvedCharacterPrompt]) {
    if characters.len() < 2 {
        return;
    }
    let mut split = 1;
    let mut widest = -1f32;
    for (i, pair) in characters.windows(2).enumerate() {
        let gap = pair[1].position.y - pair[0].position.y;
        if gap > widest {
            widest = gap;
            split = i + 1;
        }
    }
    let height = characters[characters.len() - 1].position.y - characters[0].position.y;
    if height <= 0.15 && widest <= 0.1 {
        characters.sort_by(|a, b| a.position.x.total_cmp(&b.position.x));
    } else {
        let (upper, lower) = characters.split_at_mut(split);
        order_rows(upper);
        order_rows(lower);
    }
}

fn quoted_text(text: &str) -> Vec<&str> {
    let mut quotes = Vec::new();
    let mut offset = 0;
    // If an unmatched opener has no later valid closer, subsequent openers of its kind cannot match.
    let mut exhausted = [false; 5];
    while let Some(opener) = text[offset..].chars().next() {
        let start = offset;
        offset += opener.len_utf8();
        let (kind, closer) = match opener {
            '"' => (0, '"'),
            '“' => (1, '”'),
            '「' => (2, '」'),
            '\'' if text[..start]
                .chars()
                .next_back()
                .is_none_or(|ch| ch.is_whitespace() || matches!(ch, ',' | '.')) =>
            {
                (3, '\'')
            },
            '‘' => (4, '’'),
            _ => continue,
        };
        if exhausted[kind] {
            continue;
        }
        let remainder = &text[offset..];
        let closing = remainder.match_indices(closer).find(|(index, _)| {
            !matches!(closer, '\'' | '’')
                || !remainder[index + closer.len_utf8()..].starts_with(char::is_alphanumeric)
        });
        if let Some((end, _)) = closing {
            let quote = remainder[..end].trim();
            if !quote.is_empty() {
                quotes.push(quote);
            }
            offset += end + closer.len_utf8();
        } else {
            exhausted[kind] = true;
        }
    }
    quotes
}
