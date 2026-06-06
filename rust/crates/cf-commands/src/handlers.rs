//! Analyzer report handlers + the default registry wiring.
//!
//! Each submodule is the crate-owned orchestration for one analyzer family
//! (static folder walk + per-file UAST analysis, or one git revwalk + per-commit
//! analysis), moved verbatim out of the `codefang` binary's `main.rs` where it
//! had previously been reached through a 31-arm per-`(analyzer, format)` `if`
//! ladder. The analyzer MATH stays in the cf-* analyzer crates these call
//! (cf-complexity, cf-halstead, cf-comments, cf-imports, cf-couples,
//! cf-shotness, cf-analyzer-burndown, …); this module owns only the
//! pipeline-tier orchestration + serialization (cf-gojson / cf-goyaml /
//! cf-reportutil), exactly as Go `internal/framework` + `run.go` do.
//!
//! [`default_registry`] builds the single [`crate::pipeline::Registry`] mapping
//! every analyzer id to ONE [`crate::pipeline::RunHandler`]. Each handler owns
//! its own `match format` (mirroring an analyzer's `FormatReport*` family in
//! Go); dispatch in [`crate::pipeline::run_pipeline`] is a keyed lookup by id,
//! NOT a per-format branch ladder.

pub mod burndown_ndjson;
pub mod couples_run;
pub mod go_sort;
pub mod history;
pub mod shotness_run;
pub mod static_comments;
pub mod static_complexity;
pub mod static_complexity_bin;
pub mod static_complexity_yaml;
pub mod static_halstead;
pub mod static_imports;
pub mod static_json;

use crate::pipeline::{AnalyzerEntry, Mode, Registry, RunContext};

// ---------------------------------------------------------------------------
// Shared pipeline helpers (path resolution, tick floor, RFC3339 formatting).
// These mirror the Go run.go / plumbing helpers and are shared by the static
// and history handlers.
// ---------------------------------------------------------------------------

/// Resolves the repository path from `run`'s positional arg or `-p/--path`
/// (Go run.go: the positional wins when present, else `--path`, default `.`).
#[must_use]
pub fn run_repo_path(sub: &clap::ArgMatches) -> String {
    if let Some(p) = sub.get_one::<String>("path-positional") {
        if !p.is_empty() {
            return p.clone();
        }
    }
    sub.get_one::<String>("path").cloned().unwrap_or_else(|| ".".to_string())
}

/// Rounds Unix `secs` down to the start of its 24-hour tick (Go
/// `plumbing.FloorTime(when, 24h)`). `time.Round` rounds half away from zero;
/// the post-round correction yields the floor.
#[must_use]
pub fn floor_tick_secs(secs: i64) -> i64 {
    const PERIOD: i64 = 86_400;
    let rounded = ((secs + PERIOD / 2).div_euclid(PERIOD)) * PERIOD;
    if rounded > secs {
        rounded - PERIOD
    } else {
        rounded
    }
}

/// Formats Unix seconds as Go `time.RFC3339` in the zone given by
/// `offset_minutes`; a zero offset prints the literal `Z`.
#[must_use]
pub fn format_rfc3339_offset(unix_secs: i64, offset_minutes: i32) -> String {
    let local = unix_secs + i64::from(offset_minutes) * 60;
    let days = local.div_euclid(86400);
    let secs_of_day = local.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    let hh = secs_of_day / 3600;
    let mm = (secs_of_day % 3600) / 60;
    let ss = secs_of_day % 60;
    let date = format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}");
    if offset_minutes == 0 {
        format!("{date}Z")
    } else {
        let sign = if offset_minutes < 0 { '-' } else { '+' };
        let abs = offset_minutes.unsigned_abs();
        format!("{date}{sign}{:02}:{:02}", abs / 60, abs % 60)
    }
}

/// Civil date from a day count since the Unix epoch (Howard Hinnant's algorithm).
#[must_use]
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// ---------------------------------------------------------------------------
// Static analyzer glob/bin helpers (registry-ordered multi-analyzer bin output).
// ---------------------------------------------------------------------------

/// The static analyzers in registry order (Go `defaultUASTAnalyzers ++
/// defaultRawFileAnalyzers`). `bin_ported` is true for the analyzers whose
/// `--format bin` payload is reproduced byte-for-byte; clones and cohesion are
/// not (Go-map-order-dependent, nonBinding captures).
pub const STATIC_BIN_ANALYZERS: &[(&str, bool)] = &[
    ("static/clones", false),
    ("static/complexity", true),
    ("static/comments", true),
    ("static/halstead", true),
    ("static/cohesion", false),
    ("static/imports", true),
    ("static/composition", true),
];

/// True when `pat` is a literal static analyzer ID or a glob that could match
/// static IDs (and no history ID).
#[must_use]
pub fn is_static_id_or_glob(pat: &str) -> bool {
    if pat.contains(['*', '?', '[']) {
        let any_static = STATIC_BIN_ANALYZERS.iter().any(|(id, _)| go_path_match(pat, id));
        any_static && !history_glob_matches(pat)
    } else {
        STATIC_BIN_ANALYZERS.iter().any(|(id, _)| *id == pat)
    }
}

/// True when the glob matches any known history analyzer ID.
#[must_use]
pub fn history_glob_matches(pat: &str) -> bool {
    const HISTORY_IDS: &[&str] = &[
        "history/burndown",
        "history/couples",
        "history/devs",
        "history/file-history",
        "history/imports",
        "history/shotness",
        "history/typos",
        "history/sentiment",
        "history/quality",
        "history/anomaly",
    ];
    HISTORY_IDS.iter().any(|id| go_path_match(pat, id))
}

/// Expands the requested patterns over the registry-ordered static analyzers and
/// concatenates each selected analyzer's CFB1 bin envelope (Go
/// `FormatPerAnalyzer(FormatBinary)`). Returns `None` if any selected analyzer
/// is not ported (clones/cohesion) or any folder walk fails.
#[must_use]
pub fn static_multi_bin(patterns: &[&str], path: &str) -> Option<Vec<u8>> {
    let mut selected: Vec<(&str, bool)> = Vec::new();
    for &(id, ported) in STATIC_BIN_ANALYZERS {
        let matched = patterns.iter().any(|pat| {
            if pat.contains(['*', '?', '[']) {
                go_path_match(pat, id)
            } else {
                *pat == id
            }
        });
        if matched {
            selected.push((id, ported));
        }
    }
    if selected.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for (id, ported) in selected {
        if !ported {
            return None;
        }
        let env = static_single_bin(id, path)?;
        out.extend_from_slice(&env);
    }
    Some(out)
}

/// Produces a single static analyzer's CFB1 bin envelope.
#[must_use]
pub fn static_single_bin(id: &str, path: &str) -> Option<Vec<u8>> {
    match id {
        "static/complexity" => static_complexity_bin::complexity_report_bin(path),
        "static/comments" => static_comments::comments_report_bin(path),
        "static/halstead" => static_halstead::halstead_bin_report(path),
        "static/imports" => static_imports::imports_report_bin(path),
        "static/composition" => static_json::composition_bin(path),
        _ => None,
    }
}

/// Go `path.Match` semantics over an analyzer ID (`*`, `?`, `[...]`).
#[must_use]
pub fn go_path_match(pattern: &str, name: &str) -> bool {
    go_path_match_inner(pattern.as_bytes(), name.as_bytes())
}

fn go_path_match_inner(mut pat: &[u8], mut name: &[u8]) -> bool {
    while !pat.is_empty() {
        match pat[0] {
            b'*' => {
                while !pat.is_empty() && pat[0] == b'*' {
                    pat = &pat[1..];
                }
                if pat.is_empty() {
                    return !name.contains(&b'/');
                }
                let mut i = 0;
                loop {
                    if go_path_match_inner(pat, &name[i..]) {
                        return true;
                    }
                    if i >= name.len() || name[i] == b'/' {
                        return false;
                    }
                    i += 1;
                }
            }
            b'?' => {
                if name.is_empty() || name[0] == b'/' {
                    return false;
                }
                pat = &pat[1..];
                name = &name[1..];
            }
            b'[' => {
                if name.is_empty() || name[0] == b'/' {
                    return false;
                }
                let (matched, rest) = match_class(&pat[1..], name[0]);
                if !matched {
                    return false;
                }
                pat = rest;
                name = &name[1..];
            }
            c => {
                if name.is_empty() || name[0] != c {
                    return false;
                }
                pat = &pat[1..];
                name = &name[1..];
            }
        }
    }
    name.is_empty()
}

fn match_class(pat: &[u8], ch: u8) -> (bool, &[u8]) {
    let mut i = 0;
    let mut negate = false;
    if i < pat.len() && (pat[i] == b'^' || pat[i] == b'!') {
        negate = true;
        i += 1;
    }
    let mut matched = false;
    while i < pat.len() && pat[i] != b']' {
        let lo = pat[i];
        i += 1;
        if i + 1 < pat.len() && pat[i] == b'-' && pat[i + 1] != b']' {
            let hi = pat[i + 1];
            i += 2;
            if lo <= ch && ch <= hi {
                matched = true;
            }
        } else if lo == ch {
            matched = true;
        }
    }
    let rest = if i < pat.len() { &pat[i + 1..] } else { &pat[i..] };
    (matched ^ negate, rest)
}

// ---------------------------------------------------------------------------
// Per-analyzer registry handlers. Each owns its own `match format`; this is the
// ONE place that knows how to format a given analyzer (Go: each analyzer's
// FormatReport* family). One handler per analyzer id — NOT one per (id,format).
// ---------------------------------------------------------------------------

fn h_static_complexity(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    let path = &ctx.path;
    match format {
        "json" => static_complexity::complexity_report(path),
        "yaml" => static_complexity_yaml::complexity_report_yaml(path),
        "bin" => static_complexity_bin::complexity_report_bin(path),
        _ => None,
    }
}

fn h_static_composition(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    let path = &ctx.path;
    match format {
        "json" => static_json::composition_report(path),
        "yaml" => static_json::composition_yaml(path),
        "bin" => static_json::composition_bin(path),
        _ => None,
    }
}

fn h_static_halstead(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    let path = &ctx.path;
    match format {
        "json" => static_halstead::halstead_json_report(path),
        "bin" => static_halstead::halstead_bin_report(path),
        _ => None,
    }
}

fn h_static_imports(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    let path = &ctx.path;
    match format {
        "json" => static_imports::imports_report_json(path),
        "yaml" => static_imports::imports_report_yaml(path),
        "bin" => static_imports::imports_report_bin(path),
        _ => None,
    }
}

fn h_static_comments(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    let path = &ctx.path;
    match format {
        "json" => static_comments::comments_report_json(path),
        "yaml" => static_comments::comments_report_yaml(path),
        "bin" => static_comments::comments_report_bin(path),
        _ => None,
    }
}

fn h_history_imports(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    match format {
        "json" => history::imports_run_report(ctx.matches),
        _ => None,
    }
}

fn h_history_typos(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    match format {
        "json" => history::typos_run_report(ctx.matches),
        _ => None,
    }
}

fn h_history_couples(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    match format {
        "json" => couples_run::couples_run_report(ctx.matches),
        _ => None,
    }
}

fn h_history_shotness(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    match format {
        "json" => shotness_run::shotness_run_report(ctx.matches),
        _ => None,
    }
}

fn h_history_devs(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    let sub = ctx.matches;
    let head = ctx.head();
    match (format, head) {
        ("json", true) => history::devs_head_report(sub),
        ("json", false) => history::devs_run_report(sub),
        ("yaml", true) => history::devs_head_report_yaml(sub),
        ("bin", true) => {
            let metrics = history::devs_head_metrics(sub)?;
            let payload = cf_devs::serialize::computed_metrics_to_go(&metrics);
            cf_reportutil::encode_binary_envelope(&payload).ok()
        }
        _ => None,
    }
}

fn h_history_anomaly(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    match (format, ctx.head()) {
        ("json", true) => history::anomaly_head_report(ctx.matches),
        _ => None,
    }
}

fn h_history_quality(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    match (format, ctx.head()) {
        ("json", false) => history::quality_run_report(ctx.matches),
        _ => None,
    }
}

fn h_history_sentiment(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    match (format, ctx.head()) {
        ("json", false) => history::sentiment_run_report(ctx.matches),
        _ => None,
    }
}

fn h_history_file_history(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    match (format, ctx.head()) {
        ("json", false) => history::file_history_run_report(ctx.matches),
        _ => None,
    }
}

fn h_history_burndown(ctx: &RunContext, format: &str) -> Option<Vec<u8>> {
    let sub = ctx.matches;
    let head = ctx.head();
    let ndjson = ctx.ndjson();
    match (format, head, ndjson) {
        ("timeseries", true, false) => history::burndown_head_timeseries(sub),
        ("timeseries+ndjson", false, _) => burndown_ndjson::burndown_timeseries_ndjson(sub),
        ("ndjson", false, _) => burndown_ndjson::burndown_record_ndjson(sub),
        ("json", false, _) => burndown_ndjson::burndown_run_report(sub),
        ("json" | "yaml" | "bin", true, _) => {
            let metrics = history::burndown_head_metrics(sub)?;
            let bytes = match format {
                "json" => cf_gojson::marshal(&metrics.to_go_value()),
                "bin" => cf_reportutil::encode_binary_envelope(&metrics.to_go_value()).ok()?,
                _ => {
                    let mut out = Vec::new();
                    out.extend_from_slice(b"codefang (v2):\n");
                    out.extend_from_slice(
                        format!("  version: {}\n", cf_version::DEFAULT_BINARY).as_bytes(),
                    );
                    out.extend_from_slice(
                        format!("  hash: {}\n", cf_version::BINARY_GIT_HASH).as_bytes(),
                    );
                    out.extend_from_slice(b"history/burndown:\n");
                    out.extend_from_slice(&cf_goyaml::marshal(&metrics.to_go_value_yaml()));
                    out
                }
            };
            Some(bytes)
        }
        _ => None,
    }
}

/// Builds the single default analyzer [`Registry`] — the Rust analogue of Go
/// `defaultRegistry()` (`analyze.NewRegistry(defaultUASTAnalyzers,
/// defaultRawFileAnalyzers, defaultHistoryLeaves)`). One registry insertion per
/// analyzer; dispatch is a keyed lookup by id, not a per-format match ladder.
#[must_use]
pub fn default_registry() -> Registry {
    let mut r = Registry::new();
    let s = |id: &'static str, run: crate::pipeline::RunHandler| AnalyzerEntry {
        id,
        mode: Mode::Static,
        run,
    };
    let h = |id: &'static str, run: crate::pipeline::RunHandler| AnalyzerEntry {
        id,
        mode: Mode::History,
        run,
    };

    r.register(s("static/complexity", h_static_complexity));
    r.register(s("static/composition", h_static_composition));
    r.register(s("static/halstead", h_static_halstead));
    r.register(s("static/imports", h_static_imports));
    r.register(s("static/comments", h_static_comments));

    r.register(h("history/imports", h_history_imports));
    r.register(h("history/typos", h_history_typos));
    r.register(h("history/couples", h_history_couples));
    r.register(h("history/shotness", h_history_shotness));
    r.register(h("history/devs", h_history_devs));
    r.register(h("history/anomaly", h_history_anomaly));
    r.register(h("history/quality", h_history_quality));
    r.register(h("history/sentiment", h_history_sentiment));
    r.register(h("history/file-history", h_history_file_history));
    r.register(h("history/burndown", h_history_burndown));

    r
}
