//! enry content-classifier language detection (`GetLanguagesByClassifier`).
//!
//! Faithful port of `github.com/src-d/enry/v2`'s pure-Go tokenizer
//! (`internal/tokenizer/tokenize.go`, the default `!flex` build) and Naive-Bayes
//! classifier (`classifier.go`), backed by the vendored frequency tables
//! (`data/frequencies.go`, dumped to `data/enry-frequencies-v2.1.0.tsv`).
//!
//! The classifier is the last enry strategy: it scores a set of **candidate**
//! languages (the union of all earlier strategies' results) by
//! `languagesLogProbabilities[lang] + Σ tokenProbability(token, lang)` and
//! returns them sorted by decreasing score (`sort.Stable`, descending). Float
//! addition order is fixed (tokens are a slice), so scores are deterministic;
//! the only Go nondeterminism (map-iteration order when building `scoredLangs`)
//! is erased by the stable score sort.
//!
//! Parity hinges on (1) the tokenizer reproducing Linguist's token stream and
//! (2) the frequency floats round-tripping exactly. The Go data literals are
//! 6-decimal, dumped here with `%f`, and parsed back to `f64` — exact.

use std::collections::HashMap;
use std::sync::OnceLock;

use regex::bytes::Regex;

/// `tokenizer.ByteLimit`: only the first 100000 bytes are tokenized.
const BYTE_LIMIT: usize = 100_000;

/// Parsed Naive-Bayes frequency tables (`data.{LanguagesLogProbabilities,
/// TokensLogProbabilities, TokensTotal}`).
///
/// Token keys are raw bytes (`Vec<u8>`): Go stores them as `map[string]float64`
/// where the string is `string([]byte token)` — raw bytes, possibly invalid
/// UTF-8 (3 such lines exist). Keying by `Vec<u8>` reproduces the exact lookup.
struct Frequencies {
    languages_log_prob: HashMap<String, f64>,
    tokens_log_prob: HashMap<String, HashMap<Vec<u8>, f64>>,
    tokens_total: f64,
    /// `log(1.0 / tokens_total)`, the unknown-token fallback (precomputed).
    unknown_token_log_prob: f64,
}

const FREQ_TSV: &[u8] = include_bytes!("../data/enry-frequencies-v2.1.0.tsv");

fn frequencies() -> &'static Frequencies {
    static F: OnceLock<Frequencies> = OnceLock::new();
    F.get_or_init(|| parse_frequencies(FREQ_TSV))
}

/// Parses the dumped frequency TSV (byte-oriented; tokens may be non-UTF-8):
/// - `T<TAB>tokens_total`
/// - `L<TAB>language<TAB>log_prob`
/// - `K<TAB>language<TAB>token<TAB>log_prob`
fn parse_frequencies(tsv: &[u8]) -> Frequencies {
    let mut languages_log_prob = HashMap::new();
    let mut tokens_log_prob: HashMap<String, HashMap<Vec<u8>, f64>> = HashMap::new();
    let mut tokens_total = 0.0f64;

    for line in tsv.split(|&b| b == b'\n') {
        if line.is_empty() {
            continue;
        }
        // Field 1: tag (single ASCII byte then a tab).
        let tag = line[0];
        let rest = if line.len() > 1 && line[1] == b'\t' { &line[2..] } else { &line[..0] };
        match tag {
            b'T' => {
                if let Ok(s) = std::str::from_utf8(rest) {
                    tokens_total = s.parse().unwrap_or(0.0);
                }
            }
            b'L' => {
                // `language<TAB>log_prob` (language is valid UTF-8).
                if let Some(tab) = rest.iter().rposition(|&b| b == b'\t') {
                    if let (Ok(lang), Ok(prob)) =
                        (std::str::from_utf8(&rest[..tab]), std::str::from_utf8(&rest[tab + 1..]))
                    {
                        if let Ok(p) = prob.parse::<f64>() {
                            languages_log_prob.insert(lang.to_string(), p);
                        }
                    }
                }
            }
            b'K' => {
                // `language<TAB>token<TAB>log_prob`. lang is valid UTF-8 and has
                // no tab; the token may contain non-UTF-8 bytes but no tab/newline
                // (the tokenizer never emits those). Split on the FIRST tab (lang)
                // and the LAST tab (prob); the middle bytes are the raw token.
                let Some(first) = rest.iter().position(|&b| b == b'\t') else { continue };
                let lang_bytes = &rest[..first];
                let after = &rest[first + 1..];
                let Some(last) = after.iter().rposition(|&b| b == b'\t') else { continue };
                let token = after[..last].to_vec();
                let prob_bytes = &after[last + 1..];
                if let (Ok(lang), Ok(prob_s)) =
                    (std::str::from_utf8(lang_bytes), std::str::from_utf8(prob_bytes))
                {
                    if let Ok(p) = prob_s.parse::<f64>() {
                        tokens_log_prob.entry(lang.to_string()).or_default().insert(token, p);
                    }
                }
            }
            _ => {}
        }
    }

    let unknown_token_log_prob = (1.0f64 / tokens_total).ln();
    Frequencies {
        languages_log_prob,
        tokens_log_prob,
        tokens_total,
        unknown_token_log_prob,
    }
}

// ---------------------------------------------------------------------------
// Tokenizer (port of internal/tokenizer/tokenize.go, the !flex build).
// ---------------------------------------------------------------------------

struct TokenizerRegexes {
    literal_string_quotes: Regex,
    single_line_comment: Regex,
    multiline_comment: Regex,
    literal_number: Regex,
    shebang: Regex,
    punctuation: Regex,
    sgml: Regex,
    sgml_comment: Regex,
    sgml_attributes: Regex,
    sgml_lone_attribute: Regex,
    regular_token: Regex,
    operators: Regex,
}

fn tokenizer_regexes() -> &'static TokenizerRegexes {
    static R: OnceLock<TokenizerRegexes> = OnceLock::new();
    R.get_or_init(|| {
        // Patterns copied verbatim from enry's tokenize.go (the already
        // oniguruma-vs-Go-reconciled forms). `regex::bytes` operates on &[u8].
        TokenizerRegexes {
            literal_string_quotes: Regex::new(r#"("(.|\n)*?"|'(.|\n)*?')"#).unwrap(),
            single_line_comment: Regex::new(r#"(?m)(//|--|#|%|")\s([^\n]*$)"#).unwrap(),
            multiline_comment: Regex::new(
                r#"(/\*(.|\n)*?\*/|<!--(.|\n)*?-->|\{-(.|\n)*?-\}|\(\*(.|\n)*?\*\)|"""(.|\n)*?"""|'''(.|\n)*?''')"#,
            )
            .unwrap(),
            literal_number: Regex::new(
                r#"(0x[0-9A-Fa-f]([0-9A-Fa-f]|\.)*|\d(\d|\.)*)([uU][lL]{0,2}|([eE][-+]\d*)?[fFlL]*)"#,
            )
            .unwrap(),
            shebang: Regex::new(
                r#"(?m)^#!(?:/[0-9A-Za-z_]+)*/(?:([0-9A-Za-z_]+)|[0-9A-Za-z_]+(?:\s*[0-9A-Za-z_]+=[0-9A-Za-z_]+\s*)*\s*([0-9A-Za-z_]+))(?:\s*-[0-9A-Za-z_]+\s*)*$"#,
            )
            .unwrap(),
            punctuation: Regex::new(r#";|\{|\}|\(|\)|\[|\]"#).unwrap(),
            sgml: Regex::new(r#"(<\/?[^\s<>=\d"']+)(?:\s(.|\n)*?\/?>|>)"#).unwrap(),
            sgml_comment: Regex::new(r#"(<!--(.|\n)*?-->)"#).unwrap(),
            sgml_attributes: Regex::new(r#"\s+([0-9A-Za-z_]+=)|\s+([^\s>]+)"#).unwrap(),
            sgml_lone_attribute: Regex::new(r#"([0-9A-Za-z_]+)"#).unwrap(),
            regular_token: Regex::new(r#"[0-9A-Za-z_\.@#/\*]+"#).unwrap(),
            operators: Regex::new(r#"<<?|\+|\-|\*|/|%|&&?|\|\|?"#).unwrap(),
        }
    })
}

/// Tokenize `content`, mirroring `tokenizer.Tokenize` (the `!flex` build):
/// the ordered pass list extracts shebang, SGML, then skips comments/literals,
/// then extracts punctuation, regular tokens, operators, and remainders.
fn tokenize(content: &[u8]) -> Vec<Vec<u8>> {
    let content = if content.len() > BYTE_LIMIT { &content[..BYTE_LIMIT] } else { content };
    let mut buf = content.to_vec();
    let mut tokens: Vec<Vec<u8>> = Vec::with_capacity(50);

    // 1. extractAndReplaceShebang.
    let (b1, t1) = extract_shebang(buf);
    buf = b1;
    tokens.extend(t1);
    // 2. extractAndReplaceSGML.
    let (b2, t2) = extract_sgml(buf);
    buf = b2;
    tokens.extend(t2);
    // 3. skipCommentsAndLiterals (no tokens).
    buf = skip_comments_and_literals(buf);
    // 4. extractAndReplacePunctuation.
    let (b4, t4) = common_extract_replace(buf, &tokenizer_regexes().punctuation);
    buf = b4;
    tokens.extend(t4);
    // 5. extractAndReplaceRegular.
    let (b5, t5) = common_extract_replace(buf, &tokenizer_regexes().regular_token);
    buf = b5;
    tokens.extend(t5);
    // 6. extractAndReplaceOperator.
    let (b6, t6) = common_extract_replace(buf, &tokenizer_regexes().operators);
    buf = b6;
    tokens.extend(t6);
    // 7. extractRemainders.
    tokens.extend(extract_remainders(&buf));

    tokens
}

fn common_extract_replace(content: Vec<u8>, re: &Regex) -> (Vec<u8>, Vec<Vec<u8>>) {
    let toks: Vec<Vec<u8>> = re.find_iter(&content).map(|m| m.as_bytes().to_vec()).collect();
    let replaced = re.replace_all(&content, &b" "[..]).into_owned();
    (replaced, toks)
}

fn extract_shebang(content: Vec<u8>) -> (Vec<u8>, Vec<Vec<u8>>) {
    let re = &tokenizer_regexes().shebang;
    let mut shebang_tokens: Vec<Vec<u8>> = Vec::new();
    for caps in re.captures_iter(&content) {
        shebang_tokens.push(get_shebang_token(&caps));
    }
    // NOTE: Go's extractAndReplaceShebang calls reShebang.ReplaceAll but
    // DISCARDS the result (`reShebang.ReplaceAll(content, ...)` with no
    // assignment), so the content is NOT modified. Reproduce that bug exactly.
    (content, shebang_tokens)
}

fn get_shebang_token(caps: &regex::bytes::Captures) -> Vec<u8> {
    const PREFIX: &[u8] = b"SHEBANG#!";
    let mut token: &[u8] = b"";
    // Mirror Go: iterate submatches from index 1, first non-empty wins.
    for i in 1..caps.len() {
        if let Some(m) = caps.get(i) {
            if !m.as_bytes().is_empty() {
                token = m.as_bytes();
                break;
            }
        }
    }
    let mut out = PREFIX.to_vec();
    out.extend_from_slice(token);
    out
}

fn extract_sgml(content: Vec<u8>) -> (Vec<u8>, Vec<Vec<u8>>) {
    let re = &tokenizer_regexes().sgml;
    let re_comment = &tokenizer_regexes().sgml_comment;
    let mut sgml_tokens: Vec<Vec<u8>> = Vec::new();
    let mut any = false;
    for caps in re.captures_iter(&content) {
        any = true;
        let whole = caps.get(0).map(|m| m.as_bytes()).unwrap_or(b"");
        if re_comment.is_match(whole) {
            continue;
        }
        // token = match[1] + '>'.
        let mut token = caps.get(1).map(|m| m.as_bytes().to_vec()).unwrap_or_default();
        token.push(b'>');
        sgml_tokens.push(token);
        sgml_tokens.extend(get_sgml_attributes(whole));
    }
    let replaced = if any {
        re.replace_all(&content, &b" "[..]).into_owned()
    } else {
        content
    };
    (replaced, sgml_tokens)
}

fn get_sgml_attributes(tag: &[u8]) -> Vec<Vec<u8>> {
    let re_attr = &tokenizer_regexes().sgml_attributes;
    let re_lone = &tokenizer_regexes().sgml_lone_attribute;
    let mut attributes: Vec<Vec<u8>> = Vec::new();
    for caps in re_attr.captures_iter(tag) {
        if let Some(m1) = caps.get(1) {
            if !m1.as_bytes().is_empty() {
                attributes.push(m1.as_bytes().to_vec());
            }
        }
        if let Some(m2) = caps.get(2) {
            if !m2.as_bytes().is_empty() {
                for lone in re_lone.find_iter(m2.as_bytes()) {
                    attributes.push(lone.as_bytes().to_vec());
                }
            }
        }
    }
    attributes
}

fn skip_comments_and_literals(mut content: Vec<u8>) -> Vec<u8> {
    let r = tokenizer_regexes();
    // Order: literalStringQuotes, multilineComment, singleLineComment, literalNumber.
    for re in [
        &r.literal_string_quotes,
        &r.multiline_comment,
        &r.single_line_comment,
        &r.literal_number,
    ] {
        content = re.replace_all(&content, &b" "[..]).into_owned();
    }
    content
}

fn extract_remainders(content: &[u8]) -> Vec<Vec<u8>> {
    // Go: bytes.Fields(content) then split each field on nil (== into bytes).
    let mut out: Vec<Vec<u8>> = Vec::new();
    for field in content.split(|b| b.is_ascii_whitespace()) {
        if field.is_empty() {
            continue;
        }
        // bytes.Split(remainder, nil) splits into individual bytes.
        for &byte in field {
            out.push(vec![byte]);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Classifier (port of classifier.go).
// ---------------------------------------------------------------------------

/// Classify `content` among `candidates`, returning the languages sorted by
/// decreasing score, mirroring `(*classifier).Classify` with `DefaultClassifier`.
///
/// `candidates` is the candidate set from earlier strategies (deduplicated; the
/// weights enry assigns are constant `1` and do not affect the ranking).
fn classify(content: &[u8], candidates: &[String]) -> Vec<String> {
    let f = frequencies();

    // GetLanguagesBySpecificClassifier builds a map candidate→weight, but the
    // weight is unused in scoring; the set of languages is what matters. Apply
    // GetLanguageByAlias normalization on each candidate (matches Classify).
    let mut languages: Vec<String> = Vec::with_capacity(candidates.len());
    for cand in candidates {
        let normalized = crate::canonical_language(cand).unwrap_or(cand.clone());
        if !languages.contains(&normalized) {
            languages.push(normalized);
        }
    }

    let empty = content.is_empty();
    // Token keys are raw bytes, matching Go's `string([]byte token)` map keys.
    let tokens: Vec<Vec<u8>> = if empty { Vec::new() } else { tokenize(content) };

    let mut scored: Vec<(String, f64)> = Vec::with_capacity(languages.len());
    for lang in &languages {
        let mut score = *f.languages_log_prob.get(lang).unwrap_or(&0.0);
        if !empty {
            let lang_tokens = f.tokens_log_prob.get(lang);
            for tok in &tokens {
                let p = match lang_tokens.and_then(|m| m.get(tok)) {
                    Some(&v) => v,
                    None => f.unknown_token_log_prob,
                };
                score += p;
            }
        }
        scored.push((lang.clone(), score));
    }

    // sort.Stable(byScore): Less(i,j) == score[j] < score[i] ⇒ descending by
    // score, stable on ties (preserves insertion order, which is `languages`
    // order). Rust's sort_by is stable.
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().map(|(l, _)| l).collect()
}

/// `firstLanguage`: first non-empty language, else `OtherLanguage` ("Other").
fn first_language(languages: &[String]) -> String {
    for l in languages {
        if !l.is_empty() {
            return l.clone();
        }
    }
    "Other".to_string()
}

/// Public entry: resolve a language for `content` among `candidates` via the
/// Naive-Bayes classifier (`GetLanguageByClassifier` → `firstLanguage`).
/// Returns `None` when there are no candidates (enry returns nil ⇒ the caller's
/// `firstLanguage` over the empty list yields "Other", which the devs path maps
/// to the "" / "Other" bucket itself).
#[must_use]
pub fn classify_language(content: &[u8], candidates: &[String]) -> Option<String> {
    if candidates.is_empty() {
        return None;
    }
    let ranked = classify(content, candidates);
    Some(first_language(&ranked))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_basic_words() {
        let toks = tokenize(b"package main\nfunc foo() {}\n");
        let strs: Vec<String> = toks.iter().map(|t| String::from_utf8_lossy(t).into_owned()).collect();
        assert!(strs.contains(&"package".to_string()));
        assert!(strs.contains(&"main".to_string()));
        assert!(strs.contains(&"func".to_string()));
    }

    #[test]
    fn frequencies_load() {
        let f = frequencies();
        assert!(f.tokens_total > 0.0);
        assert!(f.languages_log_prob.contains_key("SaltStack"));
        assert!(f.languages_log_prob.contains_key("Scheme"));
        assert!(f.tokens_log_prob.contains_key("SaltStack"));
    }

    #[test]
    fn classify_returns_a_candidate() {
        let cands = vec!["SaltStack".to_string(), "Scheme".to_string()];
        let r = classify_language(b"include:\n  - base\n{% set x = 1 %}\n", &cands);
        assert!(r.is_some());
        // Result must be one of the candidates.
        let r = r.unwrap();
        assert!(r == "SaltStack" || r == "Scheme");
    }

    #[test]
    fn classify_empty_candidates_none() {
        assert!(classify_language(b"anything", &[]).is_none());
    }
}
