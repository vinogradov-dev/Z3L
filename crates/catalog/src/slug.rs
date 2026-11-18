//! The text a package row is named by: a slug a person reads and a search matches.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::CatalogError;

/// The longest slug a row may carry.
///
/// Sixty-four characters hold a supplier's article number with its family prefix and still fit the
/// narrowest column a card draws.
pub const MAX_SLUG_CHARS: usize = 64;

/// A row's stable, machine-readable name: ASCII, lowercase-independent of its label.
///
/// A slug is what a search, an export and a generated node name use, so it never carries a
/// localised label and never has to be re-derived from one. The package's reader refuses anything
/// else instead of normalising it, because two spellings of one word would become two rows.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Slug(String);

impl Slug {
    /// Validates `text` as a slug.
    ///
    /// A slug is 1 to [`MAX_SLUG_CHARS`] characters of ASCII letters, digits, `-` and `_`. Case is
    /// kept as written, so `white` and `White` are two rows — a reader that folded them would have to
    /// pick one.
    pub fn new(text: impl Into<String>) -> Result<Self, CatalogError> {
        let text = text.into();
        if text.is_empty() {
            return Err(CatalogError::Empty { row: "a slug" });
        }
        if text.chars().count() > MAX_SLUG_CHARS {
            let length = text.chars().count();
            return Err(CatalogError::TooLong { text, length, limit: MAX_SLUG_CHARS });
        }
        let usable =
            text.chars().all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_');
        if !usable {
            return Err(CatalogError::Slug { text });
        }
        Ok(Self(text))
    }

    /// The slug as written.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Slug {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for Slug {
    type Error = CatalogError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::new(text)
    }
}

impl From<Slug> for String {
    fn from(slug: Slug) -> Self {
        slug.0
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn a_slug_keeps_its_case_and_reads_back_as_a_string() {
        let slug = Slug::new("chipboard-18-white").expect("a usable slug");
        assert_eq!(slug.as_str(), "chipboard-18-white");
        assert_eq!(serde_json::to_string(&slug).expect("serialize"), "\"chipboard-18-white\"");
        assert_eq!(serde_json::from_str::<Slug>("\"Shelf_2\"").expect("read back").as_str(), "Shelf_2");
    }

    #[test]
    fn a_slug_refuses_a_space_a_cyrillic_word_and_a_long_word() {
        for text in ["", "a b", "полка", "a/b"] {
            assert!(Slug::new(text).is_err(), "`{text}` is not a slug");
        }
        let long = "a".repeat(MAX_SLUG_CHARS + 1);
        let error = Slug::new(long).expect_err("over the budget");
        assert!(error.to_string().contains(&MAX_SLUG_CHARS.to_string()), "the sentence names the limit: {error}");
    }
}
