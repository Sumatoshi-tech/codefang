//! Self-contained VADER (Valence Aware Dictionary and sEntiment Reasoner)
//! scoring engine.
//!
//! # Numeric parity (report compatibility)
//!
//! VADER compound scores are floats that reach machine reports, so this is a
//! bit-faithful implementation of the reference engine, not an approximation
//! (pinned by the differential gate):
//!
//! * The base lexicon and emoji table are **vendored byte-for-byte**
//!   (`data/vaderLexicon.txt`, `data/emojiUTF8Lexicon.txt`) and parsed exactly
//!   as the reference does (line split, tab split, field 0 = word, field 1 =
//!   64-bit float).
//! * Float arithmetic matches operation-for-operation: the valence sum is a
//!   plain left-to-right `f64` sum, and `normalize` uses
//!   `score / sqrt(score*score + alpha)`.
//! * Tokenization splits on single spaces keeping empty tokens, and
//!   punctuation stripping / ALLCAPS detection reproduce the reference's
//!   Python-style string semantics.

use std::collections::HashMap;

// --- scoring constants ---

const B_INCR: f64 = 0.293;
const B_DECR: f64 = -0.293;
const C_INCR: f64 = 0.733;
const N_SCALAR: f64 = -0.74;
const ALPHA_DEFAULT: f64 = 15.0;
const VALENCE_SCALAR_SCALE1: f64 = 0.95;
const VALENCE_SCALAR_SCALE2: f64 = 0.9;
const EP_AMPLIFY_SCALE: f64 = 0.292;
const QM_AMPLIFY_SCALE: f64 = 0.18;
const MAX_EP: i64 = 4;
const MAX_QM: f64 = 0.96;
const NEGATION_SCALE: f64 = 1.25;
const BUT_SCALE: f64 = 0.5;

/// A single sentiment measurement for a statement.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Sentiment {
    /// Negative-valence proportion.
    pub negative: f64,
    /// Neutral-valence proportion.
    pub neutral: f64,
    /// Positive-valence proportion.
    pub positive: f64,
    /// Compound score in `[-1, 1]`.
    pub compound: f64,
}

// --- Python-style string helpers ---

/// String helpers reproducing the reference engine's Python-style semantics
/// (`[a-z]+` / `[A-Z]+` classes and the punctuation trim set) as ASCII
/// character-class scans, so token boundaries and ALLCAPS flags are identical.
struct PythonesqueRegex;

impl PythonesqueRegex {
    /// Punctuation trimmed from word edges:
    /// `"!#$%&'()*+,-./:;<=>?@[\]^_` + backtick + `{|}~`.
    const PUNCTUATION: &'static [char] = &[
        '"', '!', '#', '$', '%', '&', '\'', '(', ')', '*', '+', ',', '-', '.', '/', ':', ';', '<',
        '=', '>', '?', '@', '[', '\\', ']', '^', '_', '`', '{', '|', '}', '~',
    ];

    /// Python `str.isupper`-style check: any lowercase ASCII letter flips the
    /// result to false; otherwise true iff at least one uppercase ASCII letter
    /// is present.
    fn string_is_upper(s: &str) -> bool {
        let has_lower = s.bytes().any(|b| b.is_ascii_lowercase());
        if has_lower {
            return false;
        }
        s.bytes().any(|b| b.is_ascii_uppercase())
    }

    /// Strips leading/trailing punctuation, but only if at least 3 **bytes**
    /// remain (otherwise the token is returned unchanged).
    fn strip_punctuation_if_word(text: &str) -> String {
        let stripped = text.trim_matches(|c| Self::PUNCTUATION.contains(&c));
        if stripped.len() < 3 {
            return text.to_string();
        }
        stripped.to_string()
    }

    /// Whether some-but-not-all words are ALLCAPS.
    fn allcap_differential(words: &[String]) -> bool {
        let allcap = words.iter().filter(|w| Self::string_is_upper(w)).count();
        let cap_diff = words.len() as i64 - allcap as i64;
        0 < cap_diff && cap_diff < words.len() as i64
    }
}

// --- tokenized input text ---

/// Sentiment-relevant string properties of input text.
struct SentiText {
    words_and_emoticons: Vec<String>,
    words_and_emoticons_lower: Vec<String>,
    is_cap_diff: bool,
}

impl SentiText {
    /// Tokenizes on single spaces (keeping empty tokens) and strips edge
    /// punctuation per token.
    fn new(text: &str) -> Self {
        let words_and_emoticons: Vec<String> = text
            .split(' ')
            .map(PythonesqueRegex::strip_punctuation_if_word)
            .collect();
        let words_and_emoticons_lower: Vec<String> =
            words_and_emoticons.iter().map(|w| w.to_lowercase()).collect();
        let is_cap_diff = PythonesqueRegex::allcap_differential(&words_and_emoticons);
        Self {
            words_and_emoticons,
            words_and_emoticons_lower,
            is_cap_diff,
        }
    }
}

// --- term constants ---

/// Words that negate the valence of what follows them.
const NEGATE_LIST: &[&str] = &[
    "aint", "arent", "cannot", "cant", "couldnt", "darent", "didnt", "doesnt", "ain't",
    "aren't", "can't", "couldn't", "daren't", "didn't", "doesn't", "dont", "hadnt", "hasnt",
    "havent", "isnt", "mightnt", "mustnt", "neither", "don't", "hadn't", "hasn't", "haven't",
    "isn't", "mightn't", "mustn't", "neednt", "needn't", "never", "none", "nope", "nor", "not",
    "nothing", "nowhere", "oughtnt", "shant", "shouldnt", "uhuh", "wasnt", "werent", "oughtn't",
    "shan't", "shouldn't", "uh-uh", "wasn't", "weren't", "without", "wont", "wouldnt", "won't",
    "wouldn't", "rarely", "seldom", "despite",
];

fn booster_dict() -> HashMap<String, f64> {
    let inc: &[&str] = &[
        "absolutely", "amazingly", "awfully", "completely", "considerable", "considerably",
        "decidedly", "deeply", "effing", "enormous", "enormously", "entirely", "especially",
        "exceptional", "exceptionally", "extreme", "extremely", "fabulously", "flipping", "flippin",
        "frackin", "fracking", "fricking", "frickin", "frigging", "friggin", "fully", "fuckin",
        "fucking", "fuggin", "fugging", "greatly", "hella", "highly", "hugely", "incredible",
        "incredibly", "intensely", "major", "majorly", "more", "most", "particularly", "purely",
        "quite", "really", "remarkably", "so", "substantially", "thoroughly", "total", "totally",
        "tremendous", "tremendously", "uber", "unbelievably", "unusually", "utter", "utterly",
        "very",
    ];
    let dec: &[&str] = &[
        "almost", "barely", "hardly", "just enough", "kind of", "kinda", "kindof", "kind-of",
        "less", "little", "marginal", "marginally", "occasional", "occasionally", "partly",
        "scarce", "scarcely", "slight", "slightly", "somewhat", "sort of", "sorta", "sortof",
        "sort-of",
    ];
    let mut m = HashMap::new();
    for w in inc {
        m.insert((*w).to_string(), B_INCR);
    }
    for w in dec {
        m.insert((*w).to_string(), B_DECR);
    }
    m
}

fn special_case_idioms() -> HashMap<String, f64> {
    let mut m = HashMap::new();
    for (k, v) in [
        ("the shit", 3.0),
        ("the bomb", 3.0),
        ("bad ass", 1.5),
        ("badass", 1.5),
        ("yeah right", -2.0),
        ("kiss of death", -1.5),
        ("to die for", 3.0),
    ] {
        m.insert(k.to_string(), v);
    }
    m
}

struct TermConstants {
    booster_dict: HashMap<String, f64>,
    special_case_idioms: HashMap<String, f64>,
}

impl TermConstants {
    fn new() -> Self {
        Self {
            booster_dict: booster_dict(),
            special_case_idioms: special_case_idioms(),
        }
    }

    /// Check if preceding words increase/decrease/negate valence.
    fn scalar_inc_dec(&self, word: &str, word_lower: &str, valence: f64, is_cap_diff: bool) -> f64 {
        let mut scalar = 0.0;
        if let Some(&boost) = self.booster_dict.get(word_lower) {
            scalar = boost;
            if valence < 0.0 {
                scalar *= -1.0;
            }
            if PythonesqueRegex::string_is_upper(word) && is_cap_diff {
                if valence > 0.0 {
                    scalar += C_INCR;
                } else {
                    scalar -= C_INCR;
                }
            }
        }
        scalar
    }

    /// Adjusts valence for multi-word idioms around position `i`.
    fn special_idioms_check(&self, valence: f64, wel: &[String], i: usize) -> f64 {
        let mut new_valence = valence;

        let onezero = format!("{} {}", wel[i - 1], wel[i]);
        let twoonezero = format!("{} {} {}", wel[i - 2], wel[i - 1], wel[i]);
        let twoone = format!("{} {}", wel[i - 2], wel[i - 1]);
        let threetwoone = format!("{} {} {}", wel[i - 3], wel[i - 2], wel[i - 1]);
        let threetwo = format!("{} {}", wel[i - 3], wel[i - 2]);

        for seq in [&onezero, &twoonezero, &twoone, &threetwoone, &threetwo] {
            if let Some(&v) = self.special_case_idioms.get(seq) {
                new_valence = v;
                break;
            }
        }

        if wel.len() - 1 > i {
            let zeroone = format!("{} {}", wel[i], wel[i + 1]);
            if let Some(&v) = self.special_case_idioms.get(&zeroone) {
                new_valence = v;
            }
        }

        if wel.len() - 1 > i + 1 {
            let zeroonetwo = format!("{} {} {}", wel[i], wel[i + 1], wel[i + 2]);
            if let Some(&v) = self.special_case_idioms.get(&zeroonetwo) {
                new_valence = v;
            }
        }

        for ngram in [&threetwoone, &threetwo, &twoone] {
            if let Some(&b) = self.booster_dict.get(ngram) {
                new_valence += b;
            }
        }

        new_valence
    }
}

// --- scoring free functions ---

fn negated(input_words: &[&str], include_nt: bool) -> bool {
    for x in input_words {
        if NEGATE_LIST.contains(x) {
            return true;
        }
    }
    if include_nt {
        for w in input_words {
            if w.contains("n't") {
                return true;
            }
        }
    }
    false
}

fn normalize(score: f64, alpha: f64) -> f64 {
    let norm = score / ((score * score) + alpha).sqrt();
    norm.clamp(-1.0, 1.0)
}

fn normalize_default(score: f64) -> f64 {
    normalize(score, ALPHA_DEFAULT)
}

fn sift_sentiment_scores(sentiments: &[f64]) -> (f64, f64, i64) {
    let mut pos_sum = 0.0;
    let mut neg_sum = 0.0;
    let mut neu_count = 0_i64;
    for &v in sentiments {
        if v > 0.0 {
            pos_sum += v + 1.0;
        }
        if v < 0.0 {
            neg_sum += v - 1.0;
        }
        if v == 0.0 {
            neu_count += 1;
        }
    }
    (pos_sum, neg_sum, neu_count)
}

fn amplify_ep(text: &str) -> f64 {
    let mut ep = text.matches('!').count() as i64;
    if ep > MAX_EP {
        ep = MAX_EP;
    }
    ep as f64 * EP_AMPLIFY_SCALE
}

fn amplify_qm(text: &str) -> f64 {
    let qm = text.matches('?').count() as i64;
    if qm > 1 {
        if qm <= 3 {
            return qm as f64 * QM_AMPLIFY_SCALE;
        }
        return MAX_QM;
    }
    0.0
}

fn punctuation_emphasis(text: &str) -> f64 {
    amplify_ep(text) + amplify_qm(text)
}

fn negation_check(valence: f64, wel: &[String], starti: usize, i: usize) -> f64 {
    let mut new_valence = valence;
    if starti == 0 && negated(&[wel[i - 1].as_str()], true) {
        new_valence *= N_SCALAR;
    }
    if starti == 1 {
        if wel[i - 2] == "never" && (wel[i - 1] == "so" || wel[i - 1] == "this") {
            new_valence = valence * NEGATION_SCALE;
        } else if wel[i - 2] == "without" && wel[i - 1] == "doubt" {
            new_valence = valence;
        } else if negated(&[wel[i - 2].as_str()], true) {
            new_valence = valence * N_SCALAR;
        }
    }
    if starti == 2 {
        if wel[i - 3] == "never"
            && ((wel[i - 2] == "so" || wel[i - 2] == "this")
                || (wel[i - 1] == "so" || wel[i - 1] == "this"))
        {
            new_valence = valence * NEGATION_SCALE;
        } else if wel[i - 3] == "without" && (wel[i - 2] == "doubt" || wel[i - 1] == "doubt") {
            new_valence = valence;
        } else if negated(&[wel[i - 3].as_str()], true) {
            new_valence = valence * N_SCALAR;
        }
    }
    new_valence
}

fn but_check(wel: &[String], sentiments: &mut [f64]) {
    let Some(bi) = wel.iter().position(|w| w == "but") else {
        return;
    };
    for (i, s) in sentiments.iter_mut().enumerate() {
        if i < bi {
            *s *= 1.0 - BUT_SCALE;
        }
        if i > bi {
            *s *= 1.0 + BUT_SCALE;
        }
    }
}

fn score_valence(sentiments: &[f64], text: &str) -> Sentiment {
    let mut sentiment = Sentiment::default();
    if !sentiments.is_empty() {
        // Plain left-to-right f64 fold (report contract).
        let mut sum_s: f64 = sentiments.iter().sum();
        let punct = punctuation_emphasis(text);
        if sum_s > 0.0 {
            sum_s += punct;
        } else if sum_s < 0.0 {
            sum_s -= punct;
        }
        sentiment.compound = normalize_default(sum_s);

        let (mut pos_sum, mut neg_sum, neu_count) = sift_sentiment_scores(sentiments);
        if pos_sum > neg_sum.abs() {
            pos_sum += punct;
        } else if pos_sum < neg_sum.abs() {
            neg_sum -= punct;
        }
        let total = pos_sum + neg_sum.abs() + neu_count as f64;
        sentiment.positive = (pos_sum / total).abs();
        sentiment.negative = (neg_sum / total).abs();
        sentiment.neutral = (neu_count as f64 / total).abs();
    }
    sentiment
}

// --- analyzer ---

/// Computes VADER sentiment-intensity scores.
///
/// `lexicon` is public so the sentiment analyzer can inject multilingual
/// entries (see `scorer::inject_multilingual_lexicons`).
pub struct SentimentIntensityAnalyzer {
    /// Word -> valence. Public to allow multilingual injection.
    pub lexicon: HashMap<String, f64>,
    emoji_dict: HashMap<String, String>,
    constants: TermConstants,
}

impl SentimentIntensityAnalyzer {
    /// Constructs and initializes an analyzer from the vendored data tables.
    #[must_use]
    pub fn new() -> Self {
        Self {
            lexicon: make_lex_dict(),
            emoji_dict: make_emoji_dict(),
            constants: TermConstants::new(),
        }
    }

    /// Returns a sentiment score for `text`.
    #[must_use]
    pub fn polarity_scores(&self, text: &str) -> Sentiment {
        // Replace emoji with their textual descriptions.
        let mut buffer = String::with_capacity(text.len() * 2);
        let mut prev_space = true;
        for rune in text.chars() {
            let chr = rune.to_string();
            if let Some(description) = self.emoji_dict.get(&chr) {
                if !prev_space {
                    buffer.push(' ');
                }
                buffer.push_str(description);
                prev_space = false;
            } else {
                buffer.push_str(&chr);
                prev_space = chr == " ";
            }
        }

        let trimmed_text = buffer.trim().to_string();
        let sentitext = SentiText::new(&trimmed_text);

        let wel = &sentitext.words_and_emoticons;
        let well = &sentitext.words_and_emoticons_lower;
        let mut sentiments: Vec<f64> = Vec::with_capacity(wel.len());

        for (i, item) in wel.iter().enumerate() {
            let valence = 0.0;
            let item_lower = &well[i];
            // Boosters and the "kind of" idiom contribute a zero base valence.
            if self.constants.booster_dict.contains_key(item_lower)
                || (i < wel.len() - 1 && item_lower == "kind" && well[i + 1] == "of")
            {
                sentiments.push(valence);
            } else {
                self.sentiment_valence(valence, &sentitext, item, i, &mut sentiments);
            }
        }

        but_check(well, &mut sentiments);
        score_valence(&sentiments, &trimmed_text)
    }

    fn sentiment_valence(
        &self,
        valence: f64,
        sit: &SentiText,
        item: &str,
        i: usize,
        sentiments: &mut Vec<f64>,
    ) {
        let is_cap_diff = sit.is_cap_diff;
        let wel = &sit.words_and_emoticons;
        let well = &sit.words_and_emoticons_lower;
        let item_lower = item.to_lowercase();

        let mut new_valence = valence;

        if let Some(&lex) = self.lexicon.get(&item_lower) {
            new_valence = lex;
            if item_lower == "no" && i + 1 < well.len() && self.lexicon.contains_key(&well[i + 1]) {
                new_valence = 0.0;
            }
            if (i > 0 && well[i - 1] == "no")
                || (i > 1 && well[i - 2] == "no")
                || (i > 2
                    && well[i - 3] == "no"
                    && (well[i - 1] == "or" || well[i - 1] == "nor"))
            {
                new_valence = lex * N_SCALAR;
            }

            if PythonesqueRegex::string_is_upper(item) && is_cap_diff {
                if new_valence > 0.0 {
                    new_valence += C_INCR;
                } else {
                    new_valence -= C_INCR;
                }
            }

            for start_i in 0_usize..3 {
                if i > start_i && !self.lexicon.contains_key(&wel[i - (start_i + 1)]) {
                    let mut s = self.constants.scalar_inc_dec(
                        &wel[i - (start_i + 1)],
                        &well[i - (start_i + 1)],
                        new_valence,
                        is_cap_diff,
                    );
                    if start_i == 1 && s != 0.0 {
                        s *= VALENCE_SCALAR_SCALE1;
                    }
                    if start_i == 2 && s != 0.0 {
                        s *= VALENCE_SCALAR_SCALE2;
                    }
                    new_valence += s;
                    new_valence = negation_check(new_valence, well, start_i, i);
                    if start_i == 2 {
                        new_valence = self.constants.special_idioms_check(new_valence, well, i);
                    }
                }
            }
            new_valence = self.least_check(new_valence, well, i);
        }
        sentiments.push(new_valence);
    }

    fn least_check(&self, valence: f64, well: &[String], i: usize) -> f64 {
        let mut new_valence = valence;
        if i > 1 && !self.lexicon.contains_key(&well[i - 1]) && well[i - 1] == "least" {
            if well[i - 2] != "at" && well[i - 2] != "very" {
                new_valence *= N_SCALAR;
            }
        } else if i > 0 && !self.lexicon.contains_key(&well[i - 1]) && well[i - 1] == "least" {
            new_valence *= N_SCALAR;
        }
        new_valence
    }
}

impl Default for SentimentIntensityAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Parses the vendored VADER lexicon: per line, tab-split, word = field 0,
/// measure = field 1 parsed as `f64` (`0.0` on parse failure).
fn make_lex_dict() -> HashMap<String, f64> {
    const RAW: &str = include_str!("../data/vaderLexicon.txt");
    let mut m = HashMap::new();
    for line in scanner_lines(RAW) {
        let mut parts = line.split('\t');
        let word = parts.next().unwrap_or("");
        let measure: f64 = parts.next().unwrap_or("").parse().unwrap_or(0.0);
        m.insert(word.to_string(), measure);
    }
    m
}

/// Parses the vendored emoji table (tab-split: emoji, description).
fn make_emoji_dict() -> HashMap<String, String> {
    const RAW: &str = include_str!("../data/emojiUTF8Lexicon.txt");
    let mut m = HashMap::new();
    for line in scanner_lines(RAW) {
        let mut parts = line.split('\t');
        let word = parts.next().unwrap_or("");
        let descr = parts.next().unwrap_or("");
        m.insert(word.to_string(), descr.to_string());
    }
    m
}

/// Yields lines split on `\n`, dropping a trailing `\r` and skipping empty
/// lines (matching how the vendored tables are consumed by the reference
/// parser).
fn scanner_lines(raw: &str) -> impl Iterator<Item = &str> {
    raw.split('\n').filter_map(|l| {
        let l = l.strip_suffix('\r').unwrap_or(l);
        if l.is_empty() {
            None
        } else {
            Some(l)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexicon_loads() {
        let sia = SentimentIntensityAnalyzer::new();
        // Base VADER lexicon has ~7500 entries.
        assert!(sia.lexicon.len() > 7000, "lexicon size = {}", sia.lexicon.len());
    }

    #[test]
    fn positive_compound() {
        let sia = SentimentIntensityAnalyzer::new();
        let s = sia.polarity_scores("VADER is smart, handsome, and funny!");
        assert!(s.compound > 0.8, "compound = {}", s.compound);
    }

    #[test]
    fn negative_compound() {
        let sia = SentimentIntensityAnalyzer::new();
        let s = sia.polarity_scores("VADER is not smart, handsome, nor funny.");
        assert!(s.compound < 0.0, "compound = {}", s.compound);
    }

    #[test]
    fn neutral_compound() {
        let sia = SentimentIntensityAnalyzer::new();
        let s = sia.polarity_scores("The book was okay.");
        assert!(s.compound.abs() < 0.6, "compound = {}", s.compound);
    }

    #[test]
    fn normalize_clamps() {
        assert!(normalize_default(0.0).abs() < 1e-12);
        assert!(normalize_default(1000.0) <= 1.0);
        assert!(normalize_default(-1000.0) >= -1.0);
    }
}
