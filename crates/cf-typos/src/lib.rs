//! `cf-typos` — the `history/typos` analyzer: typo-fix identifier pairs over
//! commit diffs.
//!
//! A typo fix is ONE identifier that a commit changed into a DIFFERENT,
//! Levenshtein-close identifier on the same line. The crate owns the
//! detection rules ([`detect`]: the diff line-pair alignment and prefilter,
//! then the identifier-multiset diff with the pair-distance bound) and the
//! report metrics ([`metrics`]: `typo_list`, `patterns`, `file_typos`,
//! `aggregate`). The history walk in `cf-commands` supplies the git line
//! diff, the UAST identifiers per line and the commit stamps, and encodes
//! [`metrics_report_value`] through `cf-gojson` / `cf-goyaml` /
//! `cf-reportutil`.
//!
//! ```
//! use cf_alg_levenshtein::Context;
//! use cf_typos::{identifier_pair, metrics_report_value, Hash, ReportData, Typo};
//!
//! let mut lctx = Context::new();
//! let before = vec!["v".to_string(), "recieve".to_string()];
//! let after = vec!["v".to_string(), "receive".to_string()];
//! let change = identifier_pair(&before, &after, 4, &mut lctx).expect("a typo fix");
//!
//! let report = ReportData {
//!     typos: vec![Typo {
//!         wrong: change.wrong,
//!         correct: change.correct,
//!         file: "main.go".to_string(),
//!         commit: Hash::default(),
//!         line: 7,
//!     }],
//! };
//! let json = String::from_utf8(cf_gojson::marshal(&metrics_report_value(&report))).unwrap();
//! assert!(json.contains(r#""typo_list":[{"wrong":"recieve","correct":"receive","file":"main.go","line":7"#));
//! ```

pub mod detect;
pub mod hash;
pub mod metrics;
pub mod typos;

pub use detect::{candidate_line_pairs, identifier_pair, DiffOp, IdentifierChange, LinePair};
pub use hash::Hash;
pub use metrics::{metrics_report_value, ReportData};
pub use typos::{deduplicate_typos, Typo, DEFAULT_MAXIMUM_ALLOWED_TYPO_DISTANCE};
