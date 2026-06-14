//! Build-time code generator for `cf-sentiment-lexicons`.
//!
//! Port of the Go generator step. The original Go package embeds the lexicon as
//! a 94k-line generated source file (`lexicon_data.gen.go`, produced by
//! `tools/lexgen/lexgen.go`). Per the rewrite design (DESIGN §1 rule "GENERATED
//! or large embedded-data modules"), we do NOT hand-translate that artifact.
//! Instead we vendor the SAME data tables (`data/lexicon_data.tsv`, extracted
//! verbatim from the Go generated file) and regenerate the equivalent Rust data
//! at build time here, guaranteeing the lexicon entries — and therefore the
//! downstream VADER/govader scores — match the Go output byte-for-byte
//! (DESIGN §2.6).
//!
//! ## Vendored data format (`data/lexicon_data.tsv`)
//!
//! A line beginning with `@` starts a new language section and names the Go
//! loader function it came from (e.g. `@russianLexicon`). Every following line
//! until the next `@` (or EOF) is a tab-separated `word\tvalence` pair, in the
//! exact order it appeared in the Go source. This preserves both per-language
//! grouping and intra-language ordering, so `All()` and `ForLanguage()` yield
//! identical slices to the Go implementation.

use std::env;
use std::fs;
use std::path::Path;

/// Maps a Go loader-function name (the `@section` marker in the vendored TSV) to
/// its ISO 639-1 code and English display name. This is the Rust analogue of the
/// `languageRegistry` table in `lexicons.go`. Declaration order here is
/// irrelevant: the public registry is rebuilt (and ordered) in `lib.rs`.
const LOADER_TO_LANG: &[(&str, &str, &str)] = &[
    ("arabicLexicon", "ar", "Arabic"),
    ("bulgarianLexicon", "bg", "Bulgarian"),
    ("chineseLexicon", "zh", "Chinese"),
    ("croatianLexicon", "hr", "Croatian"),
    ("czechLexicon", "cs", "Czech"),
    ("danishLexicon", "da", "Danish"),
    ("dutchLexicon", "nl", "Dutch"),
    ("finnishLexicon", "fi", "Finnish"),
    ("frenchLexicon", "fr", "French"),
    ("germanLexicon", "de", "German"),
    ("greekLexicon", "el", "Greek"),
    ("hebrewLexicon", "he", "Hebrew"),
    ("hindiLexicon", "hi", "Hindi"),
    ("hungarianLexicon", "hu", "Hungarian"),
    ("indonesianLexicon", "id", "Indonesian"),
    ("italianLexicon", "it", "Italian"),
    ("japaneseLexicon", "ja", "Japanese"),
    ("koreanLexicon", "ko", "Korean"),
    ("malayLexicon", "ms", "Malay"),
    ("norwegianLexicon", "no", "Norwegian"),
    ("persianLexicon", "fa", "Persian"),
    ("polishLexicon", "pl", "Polish"),
    ("portugueseLexicon", "pt", "Portuguese"),
    ("romanianLexicon", "ro", "Romanian"),
    ("russianLexicon", "ru", "Russian"),
    ("slovakLexicon", "sk", "Slovak"),
    ("spanishLexicon", "es", "Spanish"),
    ("swedishLexicon", "sv", "Swedish"),
    ("thaiLexicon", "th", "Thai"),
    ("turkishLexicon", "tr", "Turkish"),
    ("ukrainianLexicon", "uk", "Ukrainian"),
    ("vietnameseLexicon", "vi", "Vietnamese"),
];

fn loader_lang(loader: &str) -> (&'static str, &'static str, &'static str) {
    LOADER_TO_LANG
        .iter()
        .copied()
        .find(|(fnname, _, _)| *fnname == loader)
        .unwrap_or_else(|| panic!("build.rs: unknown lexicon loader section {loader:?}"))
}

/// Escapes a word for embedding as a Rust string literal.
fn rust_str(word: &str) -> String {
    let mut out = String::with_capacity(word.len() + 2);
    out.push('"');
    for ch in word.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let data_path = Path::new(&manifest_dir).join("data/lexicon_data.tsv");
    println!("cargo:rerun-if-changed=data/lexicon_data.tsv");
    println!("cargo:rerun-if-changed=build.rs");

    let raw = fs::read_to_string(&data_path)
        .unwrap_or_else(|e| panic!("build.rs: cannot read {}: {e}", data_path.display()));

    // Parse the vendored TSV into ordered per-language sections.
    struct Section {
        loader: String,
        entries: Vec<(String, f64)>,
    }
    let mut sections: Vec<Section> = Vec::new();
    for line in raw.lines() {
        if line.is_empty() {
            continue;
        }
        if let Some(loader) = line.strip_prefix('@') {
            sections.push(Section {
                loader: loader.trim().to_string(),
                entries: Vec::new(),
            });
            continue;
        }
        let mut parts = line.splitn(2, '\t');
        let word = parts
            .next()
            .expect("build.rs: entry line missing word column");
        let valence_str = parts
            .next()
            .unwrap_or_else(|| panic!("build.rs: entry line missing valence column: {line:?}"));
        let valence: f64 = valence_str
            .trim()
            .parse()
            .unwrap_or_else(|e| panic!("build.rs: bad valence {valence_str:?}: {e}"));
        sections
            .last_mut()
            .expect("build.rs: entry line before any @section header")
            .entries
            .push((word.to_string(), valence));
    }

    assert_eq!(
        sections.len(),
        LOADER_TO_LANG.len(),
        "build.rs: vendored data has {} sections but {} languages are registered",
        sections.len(),
        LOADER_TO_LANG.len()
    );

    // Emit one `&[Entry]` constant per language plus a registry table. The
    // registry table is sorted by ISO code so `lib.rs` can expose a stable,
    // deterministic ordering (the Go map iteration order is nondeterministic,
    // but only entry *contents/counts* are observable through the public API).
    let mut out = String::new();
    out.push_str("// @generated by build.rs from data/lexicon_data.tsv. DO NOT EDIT.\n\n");

    let mut registry: Vec<(&str, &str, String)> = Vec::with_capacity(sections.len());
    for section in &sections {
        // LOADER_TO_LANG tuples are (loader_fn, code, name).
        let (_go_fn, code, name) = loader_lang(&section.loader);
        let const_name = format!("LEX_{}", code.to_uppercase());
        out.push_str(&format!("pub(crate) static {const_name}: &[Entry] = &[\n"));
        for (word, valence) in &section.entries {
            out.push_str(&format!(
                "    Entry {{ word: {}, valence: {} }},\n",
                rust_str(word),
                fmt_valence(*valence)
            ));
        }
        out.push_str("];\n\n");
        registry.push((code, name, const_name));
    }

    // Sort registry by ISO code for deterministic public iteration order.
    registry.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));

    out.push_str(
        "/// (ISO 639-1 code, English display name, entries) for every embedded language.\n",
    );
    out.push_str("pub(crate) static REGISTRY: &[(&str, &str, &[Entry])] = &[\n");
    for (code, name, const_name) in &registry {
        out.push_str(&format!(
            "    ({}, {}, {const_name}),\n",
            rust_str(code),
            rust_str(name)
        ));
    }
    out.push_str("];\n");

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR");
    let dest = Path::new(&out_dir).join("lexicon_data.rs");
    fs::write(&dest, out).expect("build.rs: write generated lexicon_data.rs");
}

/// Renders a valence so the generated source is exact-float-literal stable.
/// All real data is ±1.5, but we render whatever value the vendored data holds.
fn fmt_valence(v: f64) -> String {
    // Use a representation that round-trips and keeps a decimal point so the
    // literal is typed as f64 (e.g. `1.5`, `-1.5`).
    let s = format!("{v}");
    if s.contains('.') || s.contains('e') || s.contains('E') {
        s
    } else {
        format!("{s}.0")
    }
}
