//! Typed tag suggestions and query encoding.
use serde::{Deserialize, Serialize};

use crate::Model;

/// Query language supported by tag suggestions.
#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum TagLanguage {
    #[default]
    #[serde(rename = "en")]
    English,
    #[serde(rename = "jp")]
    Japanese,
}

impl TagLanguage {
    /// Provider language identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Japanese => "jp",
        }
    }
}

/// A model-specific incomplete tag query.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SuggestTagsRequest {
    pub model: Model,
    pub prompt: String,
    #[serde(default)]
    pub language: TagLanguage,
}

impl SuggestTagsRequest {
    /// Creates an English tag query. An empty prefix is allowed by the query schema.
    #[must_use]
    pub fn new(prompt: impl Into<String>, model: Model) -> Self {
        Self {
            model,
            prompt: prompt.into(),
            language: TagLanguage::English,
        }
    }

    /// Encodes the same query used by the client, for offline inspection.
    #[must_use]
    pub fn query_string(&self) -> String {
        form_urlencoded::Serializer::new(String::new())
            .append_pair("model", self.model.as_str())
            .append_pair("prompt", &self.prompt)
            .append_pair("lang", self.language.as_str())
            .finish()
    }
}

impl std::fmt::Debug for SuggestTagsRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SuggestTagsRequest")
            .field("model", &self.model)
            .field("language", &self.language)
            .field("prompt_len_chars", &self.prompt.chars().count())
            .finish_non_exhaustive()
    }
}

/// One suggested tag and the provider's frequency and confidence values.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TagSuggestion {
    pub tag: String,
    pub count: u64,
    pub confidence: f64,
}

/// One Japanese suggestion preserving the provider's distinct translation and ranking fields.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct JapaneseTagSuggestion {
    pub jp_tag: String,
    pub en_tag: String,
    pub power: u64,
}

/// Language-specific tag suggestions in provider order.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum SuggestTagsResult {
    English { tags: Vec<TagSuggestion> },
    Japanese(Vec<JapaneseTagSuggestion>),
}

impl SuggestTagsResult {
    /// Number of suggestions without converting language-specific ranking fields.
    #[must_use]
    pub const fn len(&self) -> usize {
        match self {
            Self::English { tags } => tags.len(),
            Self::Japanese(tags) => tags.len(),
        }
    }

    /// Whether the provider returned no suggestions.
    #[must_use]
    pub const fn is_empty(&self) -> bool { self.len() == 0 }
}

#[cfg(test)]
mod tests;
