//! Sentiment-relevant string properties. Direct port of `senti_text.go`.

use crate::python_sim::PythonesqueRegex;

/// Holds sentiment-relevant string-level properties of input text.
///
/// Mirrors Go `SentiText`.
#[derive(Debug, Clone, Default)]
pub struct SentiText {
    /// The (trimmed) input text.
    pub text: String,
    /// Tokens with leading/trailing punctuation stripped (per word rules).
    pub words_and_emoticons: Vec<String>,
    /// Lowercased copies of [`Self::words_and_emoticons`].
    pub words_and_emoticons_lower: Vec<String>,
    /// Whether some-but-not-all tokens are ALL CAPS.
    pub is_cap_diff: bool,
}

impl SentiText {
    /// Builds a `SentiText`. Mirrors Go `NewSentiText`.
    ///
    /// Tokenization is `strings.Split(text, " ")` — split on the single ASCII
    /// space, **keeping** empty tokens (so multiple spaces yield empty strings),
    /// exactly as Go does.
    #[must_use]
    pub fn new(text: &str, pr: &PythonesqueRegex) -> Self {
        let words_and_emoticons: Vec<String> = text
            .split(' ')
            .map(|token| pr.strip_punctuation_if_word(token))
            .collect();
        let words_and_emoticons_lower: Vec<String> =
            words_and_emoticons.iter().map(|w| w.to_lowercase()).collect();
        let is_cap_diff = pr.allcap_differential(&words_and_emoticons);
        Self {
            text: text.to_string(),
            words_and_emoticons,
            words_and_emoticons_lower,
            is_cap_diff,
        }
    }
}
