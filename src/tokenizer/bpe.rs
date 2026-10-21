//! Byte-level BPE machinery shared by the CLIP and Qwen encoders.
//!
//! Both encoders differ only in preprocessing and where their tables come from; the reversible
//! byte alphabet and the ranked merge loop below are identical.

use std::collections::HashMap;

/// Builds the reversible byte-to-Unicode alphabet used by byte-level BPE.
///
/// Printable ASCII and Latin-1 ranges map to themselves; every remaining byte is shifted into the
/// range starting at `U+0100`. This keeps every byte representable as a single non-whitespace,
/// non-control character, so the merge loop can operate on characters instead of raw bytes.
///
/// Indexed by byte value.
#[must_use]
pub fn byte_to_unicode() -> [char; 256] {
    let mut table = ['\0'; 256];
    for (byte, symbol) in byte_alphabet_order() {
        table[byte as usize] = symbol;
    }
    table
}

/// Returns the alphabet in the order upstream enumerates it, not in byte order.
///
/// This ordering is load-bearing for CLIP: its vocabulary IDs are assigned by position in this
/// sequence, so enumerating bytes 0..255 instead would shift almost every token ID.
#[must_use]
pub fn byte_alphabet_order() -> Vec<(u8, char)> {
    let mut bytes: Vec<u8> = Vec::with_capacity(256);
    for range in [0x21..=0x7E, 0xA1..=0xAC, 0xAE..=0xFF] {
        bytes.extend(range);
    }

    // Directly-mapped bytes first, in the ranges above; then every remaining byte, ascending.
    let mut alphabet: Vec<(u8, char)> = bytes
        .iter()
        .map(|byte| {
            (
                *byte,
                char::from_u32(u32::from(*byte)).expect("byte is a valid scalar value"),
            )
        })
        .collect();

    let mut next = 0u32;
    for byte in 0..=u8::MAX {
        if !bytes.contains(&byte) {
            alphabet.push((
                byte,
                char::from_u32(256 + next).expect("shifted byte is a valid scalar value"),
            ));
            next += 1;
        }
    }

    alphabet
}

/// One symbol in a word being merged.
///
/// `id` is the interned symbol identifier when the symbol appears anywhere in the merge table.
/// Symbols the table never mentions carry `None` and simply never merge.
#[derive(Clone, Debug)]
pub struct Symbol {
    id: Option<u32>,
    text: String,
}

impl Symbol {
    /// Returns the symbol text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Ranked merge table with interned symbols.
///
/// Symbols are interned so the merge loop can key on `(u32, u32)` pairs. Probing a
/// `HashMap<(String, String), _>` would otherwise force two allocations per adjacent pair per
/// iteration, which dominates encoding cost on long prompts.
#[derive(Debug, Default)]
pub struct MergeTable {
    ids: HashMap<String, u32>,
    texts: Vec<String>,
    /// `(left, right) -> (rank, merged symbol id)`.
    merges: HashMap<(u32, u32), (u32, u32)>,
}

impl MergeTable {
    /// Builds the table from a ranked merge list. Earlier pairs win.
    ///
    /// A pair repeated later in the list keeps its first (lowest) rank, matching how upstream
    /// implementations build their rank dictionaries.
    pub fn from_ranked_pairs<I, L, R>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (L, R)>,
        L: AsRef<str>,
        R: AsRef<str>,
    {
        let mut table = Self::default();
        for (rank, (left, right)) in pairs.into_iter().enumerate() {
            let (left, right) = (left.as_ref(), right.as_ref());
            let merged = format!("{left}{right}");
            let left_id = table.intern(left);
            let right_id = table.intern(right);
            let merged_id = table.intern(&merged);
            let rank = u32::try_from(rank).expect("merge list shorter than u32::MAX");
            table
                .merges
                .entry((left_id, right_id))
                .or_insert((rank, merged_id));
        }
        table
    }

    fn intern(&mut self, symbol: &str) -> u32 {
        if let Some(id) = self.ids.get(symbol) {
            return *id;
        }
        let id = u32::try_from(self.texts.len()).expect("symbol count fits in u32");
        self.texts.push(symbol.to_owned());
        self.ids.insert(symbol.to_owned(), id);
        id
    }

    /// Wraps a symbol text for merging, resolving its interned id when the table knows it.
    #[must_use]
    pub fn symbol(&self, text: impl Into<String>) -> Symbol {
        let text = text.into();
        let id = self.ids.get(&text).copied();
        Symbol { id, text }
    }

    /// Repeatedly merges the lowest-ranked adjacent pair until no ranked pair remains.
    ///
    /// This is the deterministic core of both byte-level encoders: a merge applies only when the
    /// exact adjacent pair exists in the table, so the result never depends on a word dictionary
    /// or on language-specific heuristics. Ties cannot occur because each pair has one rank.
    #[must_use]
    pub fn merge(&self, mut symbols: Vec<Symbol>) -> Vec<Symbol> {
        while symbols.len() > 1 {
            let mut best: Option<(usize, u32, u32)> = None;
            for index in 0..symbols.len() - 1 {
                let (Some(left), Some(right)) = (symbols[index].id, symbols[index + 1].id) else {
                    continue;
                };
                if let Some(&(rank, merged_id)) = self.merges.get(&(left, right))
                    && best.is_none_or(|(_, best_rank, _)| rank < best_rank)
                {
                    best = Some((index, rank, merged_id));
                }
            }

            let Some((index, _, merged_id)) = best else {
                break;
            };

            let right = symbols.remove(index + 1);
            let left = &mut symbols[index];
            left.text.push_str(&right.text);
            left.id = Some(merged_id);
        }

        symbols
    }
}

/// Shared byte-level encoding, vocabulary lookup and bounded piece memoization.
#[derive(Debug)]
pub(super) struct ByteBpe {
    vocab: HashMap<String, super::TokenId>,
    merges: MergeTable,
    byte_table: [char; 256],
    word_suffix: Option<&'static str>,
    cache: std::sync::Mutex<HashMap<String, Vec<super::TokenId>>>,
}

impl ByteBpe {
    pub(super) fn new(
        vocab: HashMap<String, super::TokenId>,
        pairs: Vec<(String, String)>,
        word_suffix: Option<&'static str>,
    ) -> Self {
        Self {
            vocab,
            merges: MergeTable::from_ranked_pairs(pairs),
            byte_table: byte_to_unicode(),
            word_suffix,
            cache: std::sync::Mutex::new(HashMap::new()),
        }
    }

    pub(super) fn vocabulary_size(&self) -> usize {
        self.vocab.len()
    }

    pub(super) fn token_id(&self, token: &str) -> Option<super::TokenId> {
        self.vocab.get(token).copied()
    }

    pub(super) fn encode_piece(&self, piece: &str, output: &mut Vec<super::TokenId>) {
        if let Ok(cache) = self.cache.lock()
            && let Some(cached) = cache.get(piece)
        {
            output.extend_from_slice(cached);
            return;
        }
        let mut symbols: Vec<Symbol> = piece
            .bytes()
            .map(|byte| {
                self.merges
                    .symbol(self.byte_table[byte as usize].to_string())
            })
            .collect();
        if let Some(suffix) = self.word_suffix
            && let Some(last) = symbols.pop()
        {
            symbols.push(self.merges.symbol(format!("{}{suffix}", last.text())));
        }
        let ids: Vec<super::TokenId> = self
            .merges
            .merge(symbols)
            .into_iter()
            .map(|symbol| self.vocab[symbol.text()])
            .collect();
        output.extend_from_slice(&ids);
        if let Ok(mut cache) = self.cache.lock() {
            if cache.len() >= 4096 {
                cache.clear();
            }
            cache.insert(piece.to_owned(), ids);
        }
    }
}
