//! Content-based language disambiguation heuristics.
//!
//! Faithful reproduction of enry's `GetLanguagesByContent` strategy
//! (`github.com/src-d/enry/v2@v2.1.0`), comprising:
//!
//! - the rule engine in `data/rule/rule.go` ([`Matcher`] / [`Heuristic`]),
//! - the `Heuristics.Match` algorithm in `data/heuristics.go`
//!   ([`match_heuristics`]),
//! - the generated per-extension table in `data/content.go`
//!   ([`content_heuristics`]),
//! - and the strategy entry point `GetLanguagesByContent` in `common.go`
//!   ([`languages_by_content`]).
//!
//! The heuristics disambiguate languages that collide on a single file
//! extension (e.g. `.h` is C++ vs Objective-C, `.1` is Roff vs Roff Manpage),
//! based on Linguist's content regexps. Regex matching runs over **raw bytes**
//! (content may be non-UTF8) via [`regex::bytes`]. enry's regexp engine and
//! Rust `regex` are both RE2-family, so the pattern strings are reproduced
//! verbatim, including the `(?m)` flag.

use std::collections::HashMap;
use std::sync::OnceLock;

use regex::bytes::Regex;

/// A content matcher, mirroring `rule.Matcher` in enry's `data/rule/rule.go`.
///
/// Variants correspond to the concrete `rule` types used in `content.go`:
/// `or` (a single regex, since enry's `Or` always wraps one
/// `regexp.MustCompile`), `and`, `not`, and `always`. The `languages` field of
/// the upstream rule types is carried separately on [`Heuristic`], not here,
/// because only the **top-level** rule's languages are ever read.
enum Matcher {
    /// A single compiled regex. Matches when the regex matches the data. Covers
    /// both a bare `regexp.Regexp` matcher and the `rule.Or(langs, regexp)`
    /// wrapper (whose `Match` is just the inner regex's `Match`).
    Regex(Regex),
    /// `rule.And`: matches when **every** inner matcher matches.
    And(Vec<Self>),
    /// `rule.Not`: matches when **none** of the inner matchers match.
    Not(Vec<Self>),
    /// `rule.Always`: always matches (Linguist's default fallback rule).
    Always,
}

impl Matcher {
    /// Reproduces `rule.Matcher.Match(data)`.
    fn matches(&self, data: &[u8]) -> bool {
        match self {
            Self::Regex(re) => re.is_match(data),
            Self::And(ms) => ms.iter().all(|m| m.matches(data)),
            Self::Not(ms) => !ms.iter().any(|m| m.matches(data)),
            Self::Always => true,
        }
    }
}

/// One heuristic rule: the language(s) it identifies plus its matcher.
///
/// Mirrors a single `rule.Heuristic` in an `Heuristics` list. `langs` are the
/// top-level `rule.MatchingLanguages(...)` names (language names or aliases);
/// they are resolved through enry's `LanguageByAlias` when the rule matches.
struct Heuristic {
    /// Languages (or aliases) this rule identifies, in source order.
    langs: &'static [&'static str],
    /// The matcher deciding whether this rule fires.
    matcher: Matcher,
}

/// Reproduces `Heuristics.Match(data)` from `data/heuristics.go`.
///
/// Iterates the rules in order; the **first** rule whose matcher matches wins.
/// Its languages are mapped through enry's `LanguageByAlias`
/// ([`crate::canonical_language`]); names that fail to resolve are silently
/// dropped, as upstream does. Returns the matched, resolved languages in
/// order, or an empty vec if no rule matched.
fn match_heuristics(rules: &[Heuristic], data: &[u8]) -> Vec<String> {
    let mut matched = Vec::new();
    for rule in rules {
        if rule.matcher.matches(data) {
            for lang_or_alias in rule.langs {
                if let Some(lang) = crate::canonical_language(lang_or_alias) {
                    matched.push(lang);
                }
            }
            break;
        }
    }
    matched
}

/// Reproduces enry's `GetLanguagesByContent(filename, content, _)` from
/// `common.go`.
///
/// Computes the **last** dotted extension of `filename`, lowercased
/// ([`filepath_ext`]), i.e. the suffix from the final `.` (for `pcons-2.3.1` →
/// `.1`, for `foo.tar.gz` → `.gz`). Looks up
/// the per-extension heuristics; if present, runs [`match_heuristics`] over the
/// content. Returns the matched languages (possibly several, possibly empty).
/// Returns an empty vec when `filename` is empty or its extension has no
/// heuristics.
///
/// Upstream's incoming-candidates argument is ignored (its signature discards
/// it): the result depends only on `(filename, content)`.
///
/// # Examples
///
/// ```
/// use cf_langpath::content_heuristics::languages_by_content;
///
/// // A `.h` header with an Objective-C `@interface` resolves to Objective-C.
/// assert_eq!(
///     languages_by_content("foo.h", b"@interface Foo\n"),
///     vec!["Objective-C".to_string()],
/// );
/// // An empty filename (no extension) yields no candidates.
/// assert!(languages_by_content("", b"anything").is_empty());
/// ```
#[must_use]
pub fn languages_by_content(filename: &str, content: &[u8]) -> Vec<String> {
    if filename.is_empty() {
        return Vec::new();
    }
    let ext = filepath_ext(filename).to_lowercase();
    content_heuristics()
        .get(ext.as_str())
        .map_or_else(Vec::new, |rules| match_heuristics(rules, content))
}

/// Reproduces the extension rule enry applies (its stdlib `filepath.Ext`): the
/// suffix beginning at the final `.` in the last path element, or `""` if
/// there is no dot.
///
/// The scan runs from the end of the whole path back to the last separator;
/// the first `.` it hits (i.e. the last `.` in the basename) begins the
/// extension. Since enry computes this on a filename that may still contain
/// separators, we mirror that: only dots after the last `/` or `\` count.
fn filepath_ext(path: &str) -> &str {
    let bytes = path.as_bytes();
    let mut i = bytes.len();
    while i > 0 {
        let b = bytes[i - 1];
        if b == b'/' || b == b'\\' {
            break;
        }
        if b == b'.' {
            return &path[i - 1..];
        }
        i -= 1;
    }
    ""
}

/// The per-extension heuristics table (`data.ContentHeuristics`), built once.
fn content_heuristics() -> &'static HashMap<&'static str, Vec<Heuristic>> {
    static TABLE: OnceLock<HashMap<&'static str, Vec<Heuristic>>> = OnceLock::new();
    TABLE.get_or_init(build_content_heuristics)
}

/// Compiles a Linguist regex pattern, panicking on failure.
///
/// All `content.go` patterns are RE2-clean (no backreferences or lookaround),
/// so this never fails at runtime; a panic here would mean a transcription bug
/// in a pattern string. The pattern is first passed through
/// [`sanitize_braces`] to reconcile the reference engine's lenient handling of
/// literal `{`/`}` with Rust `regex`'s stricter parser (see that function).
/// Compilation happens once, behind the [`content_heuristics`] `OnceLock`.
fn re(pattern: &str) -> Matcher {
    let sanitized = sanitize_braces(pattern);
    Matcher::Regex(
        Regex::new(&sanitized).expect("Linguist content heuristic regex must compile"),
    )
}

/// Reconciles the reference engine's lenient brace handling with Rust
/// `regex`'s parser.
///
/// enry's RE2 engine treats a `{` that does not begin a valid counted
/// repetition (`{n}`, `{n,}`, `{n,m}`) as a **literal** brace, and likewise a
/// stray `}`. Rust's `regex` crate instead rejects such braces as malformed
/// repetition syntax. Several Linguist content patterns rely on the lenient
/// behaviour (e.g. `\w+\s*{`, `{{[A-Za-z]`, `:{`).
///
/// To keep the patterns verbatim while compiling under Rust, this escapes
/// every brace that is *not* part of a valid counted repetition (`\{` / `\}`),
/// which is semantically identical to the lenient literal treatment. Backslash
/// escapes are passed through untouched (so an already-literal `\}` and the
/// `\\` sequence are preserved), and genuine repetitions like `[A-Za-z]{2}`
/// are left intact.
fn sanitize_braces(pattern: &str) -> String {
    let bytes = pattern.as_bytes();
    let mut out = String::with_capacity(pattern.len() + 4);
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\\' {
            // Copy the escape and the escaped byte verbatim.
            out.push('\\');
            if i + 1 < bytes.len() {
                out.push(bytes[i + 1] as char);
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        if b == b'{' {
            if let Some(end) = valid_repetition_end(bytes, i) {
                // A valid `{n}`/`{n,}`/`{n,m}`: copy through the closing brace.
                out.push_str(&pattern[i..=end]);
                i = end + 1;
            } else {
                out.push_str("\\{");
                i += 1;
            }
            continue;
        }
        if b == b'}' {
            // Any `}` not consumed as part of a repetition above is literal.
            out.push_str("\\}");
            i += 1;
            continue;
        }
        out.push(b as char);
        i += 1;
    }
    out
}

/// If `bytes[start]` is `{` and begins a valid counted repetition
/// (`{n}`, `{n,}`, `{n,m}` with `n`, `m` decimal), returns the index of the
/// closing `}`; otherwise `None`.
fn valid_repetition_end(bytes: &[u8], start: usize) -> Option<usize> {
    debug_assert_eq!(bytes[start], b'{');
    let mut j = start + 1;
    let lo_start = j;
    while j < bytes.len() && bytes[j].is_ascii_digit() {
        j += 1;
    }
    if j == lo_start {
        return None; // need at least one digit for the lower bound.
    }
    if j < bytes.len() && bytes[j] == b',' {
        j += 1;
        // Optional upper bound digits.
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
    }
    if j < bytes.len() && bytes[j] == b'}' {
        Some(j)
    } else {
        None
    }
}

/// Builds the manpage heuristic shared by all Roff `.N`/`.man`/`.mdoc`
/// extensions (`.1`, `.1in`, `.1m`, `.1x`, `.2`, `.3`, `.3in`, `.3m`, `.3p`,
/// `.3pm`, `.3qt`, `.3x`, `.4`–`.9`, `.man`, `.mdoc`).
///
/// These extensions carry a byte-identical three-rule `Heuristics` in
/// `content.go`: an mdoc-style "Roff Manpage" `And`, a man-style "Roff Manpage"
/// `And`, then an `Always` "Roff" fallback. Reproduced verbatim here.
fn roff_manpage_rules() -> Vec<Heuristic> {
    vec![
        Heuristic {
            langs: &["Roff Manpage"],
            matcher: Matcher::And(vec![
                re(r#"(?m)^[.'][ \t]*Dd +(?:[^"\s]+|"[^"]+")"#),
                re(r#"(?m)^[.'][ \t]*Dt +(?:[^"\s]+|"[^"]+") +"?(?:[1-9]|@[^\s@]+@)"#),
                re(r#"(?m)^[.'][ \t]*Sh +(?:[^"\s]|"[^"]+")"#),
            ]),
        },
        Heuristic {
            langs: &["Roff Manpage"],
            matcher: Matcher::And(vec![
                re(r#"(?m)^[.'][ \t]*TH +(?:[^"\s]+|"[^"]+") +"?(?:[1-9]|@[^\s@]+@)"#),
                re(r#"(?m)^[.'][ \t]*SH +(?:[^"\s]+|"[^"\s]+)"#),
            ]),
        },
        Heuristic {
            langs: &["Roff"],
            matcher: Matcher::Always,
        },
    ]
}

/// Builds the full `data.ContentHeuristics` table, translated verbatim from
/// `content.go`. Rule order within each extension is preserved exactly.
#[allow(clippy::too_many_lines)]
fn build_content_heuristics() -> HashMap<&'static str, Vec<Heuristic>> {
    let mut m: HashMap<&'static str, Vec<Heuristic>> = HashMap::new();

    // The 27 Roff manpage extensions share an identical heuristic.
    for ext in [
        ".1", ".1in", ".1m", ".1x", ".2", ".3", ".3in", ".3m", ".3p", ".3pm", ".3qt", ".3x", ".4",
        ".5", ".6", ".7", ".8", ".9", ".man", ".mdoc",
    ] {
        m.insert(ext, roff_manpage_rules());
    }

    m.insert(
        ".as",
        vec![
            Heuristic {
                langs: &["ActionScript"],
                matcher: re(r"(?m)^\s*(package\s+[a-z0-9_\.]+|import\s+[a-zA-Z0-9_\.]+;|class\s+[A-Za-z0-9_]+\s+extends\s+[A-Za-z0-9_]+)"),
            },
            Heuristic { langs: &["AngelScript"], matcher: Matcher::Always },
        ],
    );

    m.insert(
        ".asc",
        vec![
            Heuristic {
                langs: &["Public Key"],
                matcher: re(r"(?m)^(----[- ]BEGIN|ssh-(rsa|dss)) "),
            },
            Heuristic {
                langs: &["AsciiDoc"],
                matcher: re(r"(?m)^[=-]+(\s|\n)|{{[A-Za-z]"),
            },
            Heuristic {
                langs: &["AGS Script"],
                matcher: re(r"(?m)^(\/\/.+|((import|export)\s+)?(function|int|float|char)\s+((room|repeatedly|on|game)_)?([A-Za-z]+[A-Za-z_0-9]+)\s*[;\(])"),
            },
        ],
    );

    m.insert(
        ".asy",
        vec![
            Heuristic { langs: &["LTspice Symbol"], matcher: re(r"(?m)^SymbolType[ \t]") },
            Heuristic { langs: &["Asymptote"], matcher: Matcher::Always },
        ],
    );

    m.insert(
        ".bb",
        vec![
            Heuristic { langs: &["BlitzBasic"], matcher: re(r"(?m)(<^\s*; |End Function)") },
            Heuristic { langs: &["BitBake"], matcher: re(r"(?m)^\s*(# |include|require)\b") },
        ],
    );

    m.insert(
        ".builds",
        vec![
            Heuristic {
                langs: &["XML"],
                matcher: re(r"(?m)^(\s*)(?i:<Project|<Import|<Property|<?xml|xmlns)"),
            },
            Heuristic { langs: &["Text"], matcher: Matcher::Always },
        ],
    );

    m.insert(
        ".ch",
        vec![Heuristic {
            langs: &["xBase"],
            matcher: re(r"(?m)^\s*#\s*(?i:if|ifdef|ifndef|define|command|xcommand|translate|xtranslate|include|pragma|undef)\b"),
        }],
    );

    m.insert(
        ".cl",
        vec![
            Heuristic {
                langs: &["Common Lisp"],
                matcher: re(r"(?m)^\s*\((?i:defun|in-package|defpackage) "),
            },
            Heuristic { langs: &["Cool"], matcher: re(r"(?m)^class") },
            Heuristic { langs: &["OpenCL"], matcher: re(r"(?m)\/\* |\/\/ |^\}") },
        ],
    );

    m.insert(
        ".cls",
        vec![
            Heuristic { langs: &["TeX"], matcher: re(r"(?m)\\\w+{") },
            Heuristic { langs: &["ObjectScript"], matcher: re(r"(?m)^Class\s") },
        ],
    );

    m.insert(
        ".cs",
        vec![
            Heuristic { langs: &["Smalltalk"], matcher: re(r"(?m)![\w\s]+methodsFor: ") },
            Heuristic {
                langs: &["C#"],
                matcher: re(r"(?m)^(\s*namespace\s*[\w\.]+\s*{|\s*\/\/)"),
            },
        ],
    );

    m.insert(
        ".d",
        vec![
            Heuristic {
                langs: &["D"],
                matcher: re(r"(?m)^module\s+[\w.]*\s*;|import\s+[\w\s,.:]*;|\w+\s+\w+\s*\(.*\)(?:\(.*\))?\s*{[^}]*}|unittest\s*(?:\(.*\))?\s*{[^}]*}"),
            },
            Heuristic {
                langs: &["DTrace"],
                matcher: re(r"(?m)^(\w+:\w*:\w*:\w*|BEGIN|END|provider\s+|(tick|profile)-\w+\s+{[^}]*}|#pragma\s+D\s+(option|attributes|depends_on)\s|#pragma\s+ident\s)"),
            },
            Heuristic {
                langs: &["Makefile"],
                matcher: re(r"(?m)([\/\\].*:\s+.*\s\\$|: \\$|^[ %]:|^[\w\s\/\\.]+\w+\.\w+\s*:\s+[\w\s\/\\.]+\w+\.\w+)"),
            },
        ],
    );

    m.insert(
        ".ecl",
        vec![
            Heuristic { langs: &["ECLiPSe"], matcher: re(r"(?m)^[^#]+:-") },
            Heuristic { langs: &["ECL"], matcher: re(r"(?m):=") },
        ],
    );

    m.insert(
        ".es",
        vec![Heuristic {
            langs: &["Erlang"],
            matcher: re(r"(?m)^\s*(?:%%|main\s*\(.*?\)\s*->)"),
        }],
    );

    m.insert(
        ".f",
        vec![
            Heuristic { langs: &["Forth"], matcher: re(r"(?m)^: ") },
            Heuristic { langs: &["Filebench WML"], matcher: re(r"(?m)flowop") },
            Heuristic {
                langs: &["Fortran"],
                matcher: re(r"(?m)^(?i:[c*][^abd-z]|      (subroutine|program|end|data)\s|\s*!)"),
            },
        ],
    );

    m.insert(
        ".for",
        vec![
            Heuristic { langs: &["Forth"], matcher: re(r"(?m)^: ") },
            Heuristic {
                langs: &["Fortran"],
                matcher: re(r"(?m)^(?i:[c*][^abd-z]|      (subroutine|program|end|data)\s|\s*!)"),
            },
        ],
    );

    m.insert(
        ".fr",
        vec![
            Heuristic {
                langs: &["Forth"],
                matcher: re(r"(?m)^(: |also |new-device|previous )"),
            },
            Heuristic {
                langs: &["Frege"],
                matcher: re(r"(?m)^\s*(import|module|package|data|type) "),
            },
            Heuristic { langs: &["Text"], matcher: Matcher::Always },
        ],
    );

    m.insert(
        ".fs",
        vec![
            Heuristic { langs: &["Forth"], matcher: re(r"(?m)^(: |new-device)") },
            Heuristic {
                langs: &["F#"],
                matcher: re(r"(?m)^\s*(#light|import|let|module|namespace|open|type)"),
            },
            Heuristic {
                langs: &["GLSL"],
                matcher: re(r"(?m)^\s*(#version|precision|uniform|varying|vec[234])"),
            },
            Heuristic {
                langs: &["Filterscript"],
                matcher: re(r"(?m)#include|#pragma\s+(rs|version)|__attribute__"),
            },
        ],
    );

    m.insert(
        ".gd",
        vec![
            Heuristic {
                langs: &["GAP"],
                matcher: re(r"(?m)\s*(Declare|BindGlobal|KeyDependentOperation)"),
            },
            Heuristic {
                langs: &["GDScript"],
                matcher: re(r"(?m)\s*(extends|var|const|enum|func|class|signal|tool|yield|assert|onready)"),
            },
        ],
    );

    m.insert(
        ".gml",
        vec![
            Heuristic { langs: &["XML"], matcher: re(r"(?m)(?i:^\s*(\<\?xml|xmlns))") },
            Heuristic {
                langs: &["Graph Modeling Language"],
                matcher: re(r"(?m)(?i:^\s*(graph|node)\s+\[$)"),
            },
            Heuristic { langs: &["Gerber Image"], matcher: re(r"(?m)\*\%$") },
            Heuristic { langs: &["Game Maker Language"], matcher: Matcher::Always },
        ],
    );

    m.insert(
        ".gs",
        vec![Heuristic { langs: &["Gosu"], matcher: re(r"(?m)^uses java\.") }],
    );

    m.insert(
        ".h",
        vec![
            Heuristic {
                langs: &["Objective-C"],
                matcher: re(r#"(?m)^\s*(@(interface|class|protocol|property|end|synchronised|selector|implementation)\b|#import\s+.+\.h[">])"#),
            },
            Heuristic {
                langs: &["C++"],
                // src-d/enry v2.1.0 (the pinned reference version) has ONLY
                // these alternatives and NO trailing `rule.Always(C)` — later
                // go-enry releases add `__has_cpp_attribute|__cplusplus >` and
                // an Always(C) rule; do NOT backport them, the classifier must
                // arbitrate unmatched headers exactly as v2.1.0 does.
                matcher: re(r"(?m)^\s*#\s*include <(cstdint|string|vector|map|list|array|bitset|queue|stack|forward_list|unordered_map|unordered_set|(i|o|io)stream)>|^\s*template\s*<|^[ \t]*(try|constexpr)|^[ \t]*catch\s*\(|^[ \t]*(class|(using[ \t]+)?namespace)\s+\w+|^[ \t]*(private|public|protected):$|std::\w+"),
            },
        ],
    );

    m.insert(
        ".hh",
        vec![Heuristic { langs: &["Hack"], matcher: re(r"(?m)<\?hh") }],
    );

    m.insert(
        ".ice",
        vec![
            Heuristic { langs: &["JSON"], matcher: re(r"(?m)\A\s*[{\[]") },
            Heuristic { langs: &["Slice"], matcher: Matcher::Always },
        ],
    );

    m.insert(
        ".inc",
        vec![
            Heuristic { langs: &["PHP"], matcher: re(r"(?m)^<\?(?:php)?") },
            Heuristic {
                langs: &["SourcePawn"],
                matcher: re(r"(?m)^public\s+(?:SharedPlugin(?:\s+|:)__pl_\w+\s*=(?:\s*{)?|(?:void\s+)?__pl_\w+_SetNTVOptional\(\)(?:\s*{)?)"),
            },
            Heuristic {
                langs: &["POV-Ray SDL"],
                matcher: re(r"(?m)^\s*#(declare|local|macro|while)\s"),
            },
        ],
    );

    m.insert(
        ".l",
        vec![
            Heuristic { langs: &["Common Lisp"], matcher: re(r"(?m)\(def(un|macro)\s") },
            Heuristic { langs: &["Lex"], matcher: re(r"(?m)^(%[%{}]xs|<.*>)") },
            Heuristic { langs: &["Roff"], matcher: re(r"(?m)^\.[A-Za-z]{2}(\s|$)") },
            Heuristic {
                langs: &["PicoLisp"],
                matcher: re(r"(?m)^\((de|class|rel|code|data|must)\s"),
            },
        ],
    );

    m.insert(
        ".lisp",
        vec![
            Heuristic {
                langs: &["Common Lisp"],
                matcher: re(r"(?m)^\s*\((?i:defun|in-package|defpackage) "),
            },
            Heuristic { langs: &["NewLisp"], matcher: re(r"(?m)^\s*\(define ") },
        ],
    );

    m.insert(
        ".ls",
        vec![
            Heuristic {
                langs: &["LoomScript"],
                matcher: re(r"(?m)^\s*package\s*[\w\.\/\*\s]*\s*{"),
            },
            Heuristic { langs: &["LiveScript"], matcher: Matcher::Always },
        ],
    );

    m.insert(
        ".lsp",
        vec![
            Heuristic {
                langs: &["Common Lisp"],
                matcher: re(r"(?m)^\s*\((?i:defun|in-package|defpackage) "),
            },
            Heuristic { langs: &["NewLisp"], matcher: re(r"(?m)^\s*\(define ") },
        ],
    );

    m.insert(
        ".m",
        vec![
            Heuristic {
                langs: &["Objective-C"],
                matcher: re(r#"(?m)^\s*(@(interface|class|protocol|property|end|synchronised|selector|implementation)\b|#import\s+.+\.h[">])"#),
            },
            Heuristic { langs: &["Mercury"], matcher: re(r"(?m):- module") },
            Heuristic { langs: &["MUF"], matcher: re(r"(?m)^: ") },
            Heuristic { langs: &["M"], matcher: re(r"(?m)^\s*;") },
            Heuristic {
                langs: &["Mathematica"],
                matcher: Matcher::And(vec![re(r"(?m)\(\*"), re(r"(?m)\*\)$")]),
            },
            Heuristic { langs: &["MATLAB"], matcher: re(r"(?m)^\s*%") },
            Heuristic { langs: &["Limbo"], matcher: re(r"(?m)^\w+\s*:\s*module\s*{") },
        ],
    );

    m.insert(
        ".md",
        vec![
            Heuristic {
                langs: &["Markdown"],
                matcher: re(r"(?m)(^[-A-Za-z0-9=#!\*\[|>])|<\/|\A\z"),
            },
            Heuristic {
                langs: &["GCC Machine Description"],
                matcher: re(r"(?m)^(;;|\(define_)"),
            },
            Heuristic { langs: &["Markdown"], matcher: Matcher::Always },
        ],
    );

    m.insert(
        ".ml",
        vec![
            Heuristic {
                langs: &["OCaml"],
                matcher: re(r"(?m)(^\s*module)|let rec |match\s+(\S+\s)+with"),
            },
            Heuristic {
                langs: &["Standard ML"],
                matcher: re(r"(?m)=> |case\s+(\S+\s)+of"),
            },
        ],
    );

    m.insert(
        ".mod",
        vec![
            Heuristic { langs: &["XML"], matcher: re(r"(?m)<!ENTITY ") },
            Heuristic {
                langs: &["Modula-2"],
                matcher: re(r"(?m)^\s*(?i:MODULE|END) [\w\.]+;"),
            },
            Heuristic {
                langs: &["Linux Kernel Module", "AMPL"],
                matcher: Matcher::Always,
            },
        ],
    );

    m.insert(
        ".ms",
        vec![
            Heuristic { langs: &["Roff"], matcher: re(r"(?m)^[.'][A-Za-z]{2}(\s|$)") },
            Heuristic {
                langs: &["Unix Assembly"],
                matcher: Matcher::And(vec![
                    Matcher::Not(vec![re(r"(?m)/\*")]),
                    re(r"(?m)^\s*\.(?:include\s|globa?l\s|[A-Za-z][_A-Za-z0-9]*:)"),
                ]),
            },
            Heuristic { langs: &["MAXScript"], matcher: Matcher::Always },
        ],
    );

    m.insert(
        ".n",
        vec![
            Heuristic { langs: &["Roff"], matcher: re(r"(?m)^[.']") },
            Heuristic {
                langs: &["Nemerle"],
                matcher: re(r"(?m)^(module|namespace|using)\s"),
            },
        ],
    );

    m.insert(
        ".ncl",
        vec![
            Heuristic { langs: &["XML"], matcher: re(r"(?m)^\s*<\?xml\s+version") },
            Heuristic { langs: &["Text"], matcher: re(r"(?m)THE_TITLE") },
        ],
    );

    m.insert(
        ".nl",
        vec![
            Heuristic { langs: &["NL"], matcher: re(r"(?m)^(b|g)[0-9]+ ") },
            Heuristic { langs: &["NewLisp"], matcher: Matcher::Always },
        ],
    );

    m.insert(
        ".php",
        vec![
            Heuristic { langs: &["Hack"], matcher: re(r"(?m)<\?hh") },
            Heuristic { langs: &["PHP"], matcher: re(r"(?m)<\?[^h]") },
        ],
    );

    m.insert(
        ".pl",
        vec![
            Heuristic { langs: &["Prolog"], matcher: re(r"(?m)^[^#]*:-") },
            Heuristic {
                langs: &["Perl"],
                matcher: re(r"(?m)\buse\s+(?:strict\b|v?5\.)"),
            },
            Heuristic {
                langs: &["Perl 6"],
                matcher: re(r"(?m)^\s*(?:use\s+v6\b|\bmodule\b|\b(?:my\s+)?class\b)"),
            },
        ],
    );

    m.insert(
        ".pm",
        vec![
            Heuristic {
                langs: &["Perl"],
                matcher: re(r"(?m)\buse\s+(?:strict\b|v?5\.)"),
            },
            Heuristic {
                langs: &["Perl 6"],
                matcher: re(r"(?m)^\s*(?:use\s+v6\b|\bmodule\b|\b(?:my\s+)?class\b)"),
            },
            Heuristic {
                langs: &["X PixMap"],
                matcher: re(r"(?m)^\s*\/\* XPM \*\/"),
            },
        ],
    );

    m.insert(
        ".pod",
        vec![
            Heuristic {
                langs: &["Pod 6"],
                matcher: re(r"(?m)^[\s&&[^\n]]*=(comment|begin pod|begin para|item\d+)"),
            },
            Heuristic { langs: &["Pod"], matcher: Matcher::Always },
        ],
    );

    m.insert(
        ".pp",
        vec![
            Heuristic { langs: &["Pascal"], matcher: re(r"(?m)^\s*end[.;]") },
            Heuristic { langs: &["Puppet"], matcher: re(r"(?m)^\s+\w+\s+=>\s") },
        ],
    );

    m.insert(
        ".pro",
        vec![
            Heuristic { langs: &["Prolog"], matcher: re(r"(?m)^[^\[#]+:-") },
            Heuristic { langs: &["INI"], matcher: re(r"(?m)last_client=") },
            Heuristic {
                langs: &["QMake"],
                matcher: Matcher::And(vec![re(r"(?m)HEADERS"), re(r"(?m)SOURCES")]),
            },
            Heuristic {
                langs: &["IDL"],
                matcher: re(r"(?m)^\s*function[ \w,]+$"),
            },
        ],
    );

    m.insert(
        ".properties",
        vec![
            Heuristic {
                langs: &["INI"],
                matcher: Matcher::And(vec![re(r"(?m)^[^#!;][^=]*="), re(r"(?m)^[;\[]")]),
            },
            Heuristic {
                langs: &["Java Properties"],
                matcher: Matcher::And(vec![re(r"(?m)^[^#!;][^=]*="), re(r"(?m)^[#!]")]),
            },
            Heuristic { langs: &["INI"], matcher: re(r"(?m)^[^#!;][^=]*=") },
            Heuristic { langs: &["Java properties"], matcher: re(r"(?m)^[^#!][^:]*:") },
        ],
    );

    m.insert(
        ".props",
        vec![
            Heuristic {
                langs: &["XML"],
                matcher: re(r"(?m)^(\s*)(?i:<Project|<Import|<Property|<\?xml|xmlns)"),
            },
            Heuristic { langs: &["INI"], matcher: re(r"(?m)(?i:\w+\s*=\s*)") },
        ],
    );

    m.insert(
        ".q",
        vec![
            Heuristic {
                langs: &["q"],
                matcher: re(r"(?m)((?i:[A-Z.][\w.]*:{)|(^|\n)\\(cd?|d|l|p|ts?) )"),
            },
            Heuristic {
                langs: &["HiveQL"],
                matcher: re(r"(?m)(?i:SELECT\s+[\w*,]+\s+FROM|(CREATE|ALTER|DROP)\s(DATABASE|SCHEMA|TABLE))"),
            },
        ],
    );

    m.insert(
        ".r",
        vec![
            Heuristic { langs: &["Rebol"], matcher: re(r"(?m)(?i:\bRebol\b)") },
            Heuristic { langs: &["R"], matcher: re(r"(?m)<-|^\s*#") },
        ],
    );

    m.insert(
        ".rno",
        vec![Heuristic { langs: &["Roff"], matcher: re(r#"(?m)^\.\\" "#) }],
    );

    m.insert(
        ".rpy",
        vec![
            Heuristic {
                langs: &["Python"],
                matcher: re(r"(?m)(?m:^(import|from|class|def)\s)"),
            },
            Heuristic { langs: &["Ren'Py"], matcher: Matcher::Always },
        ],
    );

    m.insert(
        ".rs",
        vec![
            Heuristic {
                langs: &["Rust"],
                matcher: re(r"(?m)^(use |fn |mod |pub |macro_rules|impl|#!?\[)"),
            },
            Heuristic {
                langs: &["RenderScript"],
                matcher: re(r"(?m)#include|#pragma\s+(rs|version)|__attribute__"),
            },
        ],
    );

    m.insert(
        ".sc",
        vec![
            Heuristic {
                langs: &["SuperCollider"],
                matcher: re(r"(?m)(?i:\^(this|super)\.|^\s*~\w+\s*=\.)"),
            },
            Heuristic {
                langs: &["Scala"],
                matcher: re(r"(?m)(^\s*import (scala|java)\.|^\s*class\b)"),
            },
        ],
    );

    m.insert(
        ".sql",
        vec![
            Heuristic {
                langs: &["PLpgSQL"],
                matcher: re(r"(?m)(?i:^\\i\b|AS \$\$|LANGUAGE '?plpgsql'?|SECURITY (DEFINER|INVOKER)|BEGIN( WORK )?;)"),
            },
            Heuristic {
                langs: &["SQLPL"],
                matcher: re(r"(?m)(?i:(alter module)|(language sql)|(begin( NOT)+ atomic)|signal SQLSTATE '[0-9]+')"),
            },
            Heuristic {
                langs: &["PLSQL"],
                matcher: re(r"(?m)(?i:\$\$PLSQL_|XMLTYPE|sysdate|systimestamp|\.nextval|connect by|AUTHID (DEFINER|CURRENT_USER)|constructor\W+function)"),
            },
            Heuristic {
                langs: &["TSQL"],
                matcher: Matcher::And(vec![
                    Matcher::Not(vec![re(r"(?m)(?i:IDENTIFIED|NUMBER|VARCHAR2|REPEAT|UNTIL|IMMEDIATE)")]),
                    re(r"(?m)(?i:(GO)|(@@)|(CREATE PROCEDURE)|BEGIN( TRY| CATCH)|OUTPUT( INSERTED)|IF|ELSE|IIF|CHOOSE|CURSOR|FETCH|DEALLOCATE|DECLARE)"),
                ]),
            },
            Heuristic {
                langs: &["SQL"],
                matcher: Matcher::Not(vec![re(r"(?m)(?i:begin|boolean|package|exception)")]),
            },
        ],
    );

    m.insert(
        ".srt",
        vec![Heuristic {
            langs: &["SubRip Text"],
            matcher: re(r"(?m)^(\d{2}:\d{2}:\d{2},\d{3})\s*(-->)\s*(\d{2}:\d{2}:\d{2},\d{3})$"),
        }],
    );

    m.insert(
        ".t",
        vec![
            Heuristic {
                langs: &["Perl"],
                matcher: re(r"(?m)\buse\s+(?:strict\b|v?5\.)"),
            },
            Heuristic {
                langs: &["Perl 6"],
                matcher: re(r"(?m)^\s*(?:use\s+v6\b|\bmodule\b|\b(?:my\s+)?class\b)"),
            },
            Heuristic {
                langs: &["Turing"],
                matcher: re(r"(?m)^\s*%[ \t]+|^\s*var\s+\w+(\s*:\s*\w+)?\s*:=\s*\w+"),
            },
        ],
    );

    m.insert(
        ".toc",
        vec![
            Heuristic {
                langs: &["World of Warcraft Addon Data"],
                matcher: re(r"(?m)^## |@no-lib-strip@"),
            },
            Heuristic {
                langs: &["TeX"],
                matcher: re(r"(?m)^\\(contentsline|defcounter|beamer|boolfalse)"),
            },
        ],
    );

    m.insert(
        ".ts",
        vec![
            Heuristic { langs: &["XML"], matcher: re(r"(?m)<TS\b") },
            Heuristic { langs: &["TypeScript"], matcher: Matcher::Always },
        ],
    );

    m.insert(
        ".tst",
        vec![
            Heuristic { langs: &["GAP"], matcher: re(r"(?m)gap> ") },
            Heuristic { langs: &["Scilab"], matcher: Matcher::Always },
        ],
    );

    m.insert(
        ".tsx",
        vec![
            Heuristic {
                langs: &["TSX"],
                matcher: re(r#"(?m)^\s*(import.+(from\s+|require\()['"]react|\/\/\/\s*<reference\s)"#),
            },
            Heuristic {
                langs: &["XML"],
                matcher: re(r"(?m)(?i:^\s*<\?xml\s+version)"),
            },
        ],
    );

    m.insert(
        ".vba",
        vec![
            Heuristic { langs: &["Vim script"], matcher: re(r"(?m)^UseVimball") },
            Heuristic { langs: &["Visual Basic"], matcher: Matcher::Always },
        ],
    );

    m.insert(
        ".w",
        vec![
            Heuristic {
                langs: &["OpenEdge ABL"],
                matcher: re(r"(?m)&ANALYZE-SUSPEND _UIB-CODE-BLOCK _CUSTOM _DEFINITIONS"),
            },
            Heuristic { langs: &["CWeb"], matcher: re(r"(?m)^@(<|\w+\.)") },
        ],
    );

    m.insert(
        ".x",
        vec![
            Heuristic {
                langs: &["RPC"],
                matcher: re(r"(?m)\b(program|version)\s+\w+\s*{|\bunion\s+\w+\s+switch\s*\("),
            },
            Heuristic { langs: &["Logos"], matcher: re(r"(?m)^%(end|ctor|hook|group)\b") },
            Heuristic {
                langs: &["Linker Script"],
                matcher: re(r"(?m)OUTPUT_ARCH\(|OUTPUT_FORMAT\(|SECTIONS"),
            },
        ],
    );

    m.insert(
        ".yy",
        vec![
            Heuristic {
                langs: &["JSON"],
                matcher: re(r#"(?m)\"modelName\"\:\s*\"GM"#),
            },
            Heuristic { langs: &["Yacc"], matcher: Matcher::Always },
        ],
    );

    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filepath_ext_last_dotted_suffix() {
        assert_eq!(filepath_ext("pcons-2.3.1"), ".1");
        assert_eq!(filepath_ext("foo.tar.gz"), ".gz");
        assert_eq!(filepath_ext("Makefile"), "");
        assert_eq!(filepath_ext("a/b.c/d"), "");
        assert_eq!(filepath_ext("a/b/c.h"), ".h");
        assert_eq!(filepath_ext(".hidden"), ".hidden");
    }

    #[test]
    fn all_eighty_extensions_present() {
        // content.go defines exactly 80 extension keys.
        assert_eq!(content_heuristics().len(), 80);
    }

    #[test]
    fn perl_shebang_dot_one_is_roff() {
        // A Perl `.1` file with no .TH/.SH/.Dd lines falls through to the
        // Always("Roff") rule. Mirrors the pcons-2.3.1 case.
        let content = b"#!/usr/bin/env perl\nuse strict;\nprint \"hello\\n\";\n";
        assert_eq!(languages_by_content("pcons-2.3.1", content), vec!["Roff"]);
    }

    #[test]
    fn real_manpage_dot_one_is_roff_manpage() {
        // A man-style page with .TH and .SH matches the second And rule.
        let content = b".TH FOO 1 \"2020\"\n.SH NAME\nfoo \\- does things\n";
        assert_eq!(
            languages_by_content("foo.1", content),
            vec!["Roff Manpage"]
        );
    }

    #[test]
    fn mdoc_manpage_is_roff_manpage() {
        // An mdoc-style page (.Dd/.Dt/.Sh) matches the first And rule.
        let content = b".Dd June 9, 2026\n.Dt FOO 1\n.Sh NAME\n";
        assert_eq!(
            languages_by_content("foo.1", content),
            vec!["Roff Manpage"]
        );
    }

    #[test]
    fn h_objective_c_vs_cpp() {
        let objc = b"#import <Foundation/Foundation.h>\n@interface Foo\n@end\n";
        assert_eq!(languages_by_content("foo.h", objc), vec!["Objective-C"]);

        let cpp = b"#include <vector>\ntemplate <class T>\nclass Foo {};\n";
        assert_eq!(languages_by_content("foo.h", cpp), vec!["C++"]);

        // No heuristic matches -> empty (extension strategy/classifier decide).
        let plain = b"int x = 1;\n";
        assert!(languages_by_content("foo.h", plain).is_empty());
    }

    #[test]
    fn m_objective_c_vs_matlab() {
        let objc = b"#import <Foundation/Foundation.h>\n@implementation Foo\n@end\n";
        assert_eq!(languages_by_content("foo.m", objc), vec!["Objective-C"]);

        let matlab = b"% a comment\nx = 1;\n";
        assert_eq!(languages_by_content("foo.m", matlab), vec!["MATLAB"]);
    }

    #[test]
    fn t_perl_vs_turing() {
        let perl = b"use strict;\nuse warnings;\n";
        assert_eq!(languages_by_content("foo.t", perl), vec!["Perl"]);
    }

    #[test]
    fn pl_prolog_vs_perl() {
        let perl = b"#!/usr/bin/perl\nuse strict;\n";
        assert_eq!(languages_by_content("foo.pl", perl), vec!["Perl"]);

        let prolog = b"foo(X) :- bar(X).\n";
        assert_eq!(languages_by_content("foo.pl", prolog), vec!["Prolog"]);
    }

    #[test]
    fn mod_multiple_languages_returned() {
        // The Always rule identifies TWO languages.
        let content = b"some random content\n";
        assert_eq!(
            languages_by_content("foo.mod", content),
            vec!["Linux Kernel Module", "AMPL"]
        );
    }

    #[test]
    fn unknown_extension_is_empty() {
        assert!(languages_by_content("foo.go", b"package main").is_empty());
        assert!(languages_by_content("", b"x").is_empty());
    }
}
