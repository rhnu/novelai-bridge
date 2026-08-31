//! HTML character references, using Python's HTML5 entity data (PSF-2.0, see CREDITS.md).
//! Legacy names may omit `;`; numeric references use HTML replacement/control rules.

use std::{collections::HashMap, sync::LazyLock};

static ENTITIES: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("entities.json")).expect("generated HTML5 entity table")
});

pub(super) fn unescape_html(text: &str) -> String {
    let mut rest = text;
    let mut out = String::with_capacity(text.len());
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start + 1..];
        if let Some(numeric) = rest.strip_prefix('#') {
            let hexadecimal = numeric.starts_with(['x', 'X']);
            let digits = if hexadecimal { &numeric[1..] } else { numeric };
            let len = digits
                .bytes()
                .take_while(|byte| {
                    if hexadecimal {
                        byte.is_ascii_hexdigit()
                    } else {
                        byte.is_ascii_digit()
                    }
                })
                .count();
            if len > 0 {
                let value = u32::from_str_radix(&digits[..len], if hexadecimal { 16 } else { 10 })
                    .unwrap_or(u32::MAX);
                if let Some(ch) = numeric_char(value) {
                    out.push(ch);
                }
                let consumed = 1 + usize::from(hexadecimal) + len;
                rest = &rest[consumed..];
                rest = rest.strip_prefix(';').unwrap_or(rest);
                continue;
            }
        }
        let length = rest
            .bytes()
            .take(32)
            .take_while(u8::is_ascii_alphanumeric)
            .count();
        let mut matched = false;
        for end in (1..=length).rev() {
            let terminated = rest.as_bytes().get(end) == Some(&b';');
            let consumed = end + usize::from(terminated);
            if let Some(value) = ENTITIES.get(&rest[..consumed]) {
                out.push_str(value);
                rest = &rest[end + usize::from(terminated)..];
                matched = true;
                break;
            }
        }
        if !matched {
            out.push('&');
        }
    }
    out.push_str(rest);
    out
}

fn numeric_char(value: u32) -> Option<char> {
    const C1: [u32; 32] = [
        0x20ac, 0x81, 0x201a, 0x192, 0x201e, 0x2026, 0x2020, 0x2021, 0x2c6, 0x2030, 0x160, 0x2039,
        0x152, 0x8d, 0x17d, 0x8f, 0x90, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014,
        0x2dc, 0x2122, 0x161, 0x203a, 0x153, 0x9d, 0x17e, 0x178,
    ];
    if value == 0 || (0xd800..=0xdfff).contains(&value) || value > 0x0010_ffff {
        return Some('\u{fffd}');
    }
    if (0x80..=0x9f).contains(&value) {
        return char::from_u32(C1[(value - 0x80) as usize]);
    }
    if matches!(value, 1..=8 | 11 | 14..=31 | 127 | 0xfdd0..=0xfdef) || value & 0xffff >= 0xfffe {
        return None;
    }
    char::from_u32(value)
}
