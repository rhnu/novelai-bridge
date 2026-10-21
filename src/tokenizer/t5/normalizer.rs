//! `SentencePiece`'s serialized Darts charsmap and whitespace rules (no ICU dependency).
use super::{
    TokenizerError,
    asset::{invalid_data, malformed},
    wire::Reader,
};

#[derive(Debug)]
// Independent flags are specified by the upstream NormalizerSpec/TrainerSpec wire format.
#[expect(
    clippy::struct_excessive_bools,
    reason = "SentencePiece stores independent normalization flags in its serialized model"
)]
pub(super) struct Normalizer {
    units: Vec<u32>,
    replacements: String,
    add_prefix: bool,
    remove_spaces: bool,
    escape_spaces: bool,
    pub(super) suffix: bool,
}

impl Default for Normalizer {
    fn default() -> Self {
        Self {
            units: Vec::new(),
            replacements: String::new(),
            add_prefix: true,
            remove_spaces: true,
            escape_spaces: true,
            suffix: false,
        }
    }
}

const fn offset(unit: u32) -> usize {
    ((unit >> 10) << ((unit & 512) >> 6)) as usize
}

const fn label(unit: u32) -> u32 {
    unit & 0x8000_00FF
}

impl Normalizer {
    pub(super) fn parse(bytes: &[u8]) -> Result<Self, TokenizerError> {
        let mut normalizer = Self::default();
        let mut reader = Reader::new(bytes);
        while !reader.is_done() {
            match reader.tag()? {
                (2, 2) => normalizer.load_map(reader.length_delimited()?)?,
                (3, 0) => normalizer.add_prefix = reader.varint()? != 0,
                (4, 0) => normalizer.remove_spaces = reader.varint()? != 0,
                (5, 0) => normalizer.escape_spaces = reader.varint()? != 0,
                (_, wire) => reader.skip(wire)?,
            }
        }
        Ok(normalizer)
    }

    fn load_map(&mut self, bytes: &[u8]) -> Result<(), TokenizerError> {
        if bytes.is_empty() {
            return Ok(());
        }
        let mut reader = Reader::new(bytes);
        let size = u32::from_le_bytes(reader.take(4)?.try_into().expect("four bytes")) as usize;
        if size < 1024 || !size.is_multiple_of(1024) {
            return Err(malformed("invalid charsmap trie size"));
        }
        self.units = reader
            .take(size)?
            .as_chunks::<4>()
            .0
            .iter()
            .map(|word| u32::from_le_bytes(*word))
            .collect();
        std::str::from_utf8(reader.take(bytes.len() - size - 4)?)
            .map_err(|source| invalid_data("charsmap replacement text", source))?
            .clone_into(&mut self.replacements);
        if !self.replacements.ends_with('\0') {
            return Err(malformed("unterminated charsmap replacements"));
        }
        // Validate all transitions/leaf offsets before encoding: no unchecked asset indexing.
        let root = self.units[0];
        if label(root) != 0 || root & 256 != 0 || offset(root) == 0 {
            return Err(malformed("invalid charsmap root"));
        }
        for (index, &unit) in self.units.iter().enumerate() {
            if label(unit) > 255 {
                continue;
            }
            let base = index ^ offset(unit);
            if (base | 255) >= self.units.len() {
                return Err(malformed("charsmap transition out of bounds"));
            }
            if unit & 256 != 0 {
                let value = (self.units[base] & 0x7FFF_FFFF) as usize;
                if value >= self.replacements.len() || !self.replacements.is_char_boundary(value) {
                    return Err(malformed("charsmap replacement out of bounds"));
                }
            }
        }
        Ok(())
    }

    fn prefix<'a>(&'a self, text: &'a str) -> (&'a str, usize) {
        let mut matched = None;
        if !self.units.is_empty() {
            let mut node = offset(self.units[0]);
            for (index, byte) in text.bytes().enumerate() {
                node ^= usize::from(byte);
                let unit = self.units[node];
                if label(unit) != u32::from(byte) {
                    break;
                }
                node ^= offset(unit);
                if unit & 256 != 0 && text.is_char_boundary(index + 1) {
                    matched = Some((index + 1, (self.units[node] & 0x7FFF_FFFF) as usize));
                }
            }
        }
        if let Some((length, value)) = matched {
            let tail = &self.replacements[value..];
            return (
                &tail[..tail.find('\0').expect("validated charsmap terminator")],
                length,
            );
        }
        let length = text.chars().next().expect("nonempty prefix").len_utf8();
        (&text[..length], length)
    }

    pub(super) fn normalize(&self, text: &str, user_symbols: &[String]) -> String {
        let mut mapped = String::with_capacity(text.len());
        let mut rest = text;
        while !rest.is_empty() {
            // User-defined pieces bypass the model's Unicode rewriting.
            let (piece, length) = user_symbols
                .iter()
                .find(|symbol| rest.starts_with(symbol.as_str()))
                .map_or_else(
                    || self.prefix(rest),
                    |symbol| (&rest[..symbol.len()], symbol.len()),
                );
            mapped.push_str(piece);
            rest = &rest[length..];
        }
        let mut spaced = if self.remove_spaces {
            mapped
                .split(' ')
                .filter(|word| !word.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            mapped
        };
        if spaced.is_empty() {
            return spaced;
        }
        if self.add_prefix {
            if self.suffix {
                spaced.push(' ');
            } else {
                spaced.insert(0, ' ');
            }
        }
        if self.escape_spaces {
            spaced.replace(' ', "▁")
        } else {
            spaced
        }
    }
}
