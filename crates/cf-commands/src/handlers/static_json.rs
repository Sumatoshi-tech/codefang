//! Static-analysis JSON report path for raw-file analyzers (`static/composition`).
//!
//! Reproduces the reference static pipeline for the single-analyzer
//! `codefang run --analyzers static/composition --format json` capture:
//!
//!  1. the reference `StaticService.rawFilePhase`
//!     walks `rootPath` with `filepath.WalkDir` — directories are recursed
//!     (except `.git`), every regular file is offered to the requested
//!     `RawFileAnalyzer`s. A file survives when it (a) matches the `--languages`
//!     globs (none here → all match) and (b) is **not** excluded by
//!     `pathpolicy.Exclude(path, nil, opts)` (default opts: drop vendor +
//!     generated paths).
//!  2. Each surviving file is classified by reading its first
//!     `contentHeaderSize` (8192) bytes and running the composition classifier
//!     (`composition.Analyzer.AnalyzeFileContent`), then aggregated
//!     (`composition.Aggregator`): `total_files` and a per-category breakdown.
//!  3. The aggregated report becomes one `renderer.JSONSection`
//!     (`SectionToJSON`) wrapped in a `renderer.JSONReport`
//!     (`SectionsToJSON`), and serialized via
//!     `json.NewEncoder(w).SetIndent("", "  ").Encode(report)` — two-space
//!     indent, one trailing newline.
//!
//! `composition` is informational (`ScoreInfoOnly = -1`), so the overall score
//! and the section score are both `-1` and the labels are `"Info"`
//! (`BaseReportSection.ScoreLabel` / `ExecutiveSummary.OverallScoreLabel`).
//!
//! WalkDir visits directory entries in lexical (byte-sorted) order; we reproduce
//! that with a sorted recursive `read_dir`. The order is load-bearing for
//! analyzers whose section preserves discovery order, and harmless here.

use std::fs;
use std::path::Path;

use cf_composition::{Category, Classifier, ALL_CATEGORIES};
use cf_gojson::{Encoder, GoMap, GoValue, MapOrigin};
use cf_pathpolicy::{exclude, Options};

/// Max bytes read per file in the raw-file pre-pass (the reference `contentHeaderSize`,
/// the reference static service).
const CONTENT_HEADER_SIZE: usize = 8192;

/// Section title / status / score constants (composition report section).
const SECTION_TITLE: &str = "COMPOSITION";
const STATUS_DEFAULT: &str = "File composition analysis completed";
const STATUS_EMPTY: &str = "No files analyzed";
const METRIC_TOTAL_FILES: &str = "Total Files";
const METRIC_SOURCE: &str = "Source Files";
const METRIC_SOURCE_PCT: &str = "Source %";
const SCORE_INFO_ONLY: f64 = -1.0;
const SCORE_LABEL_INFO: &str = "Info";
const SEVERITY_INFO: &str = "info";
const SEVERITY_POOR: &str = "poor";

/// Aggregated composition counts.
#[derive(Default)]
struct Counts {
    total_files: i64,
    /// Per-category counts, indexed by [`ALL_CATEGORIES`] position.
    by_category: std::collections::HashMap<&'static str, i64>,
}

impl Counts {
    fn get(&self, cat: Category) -> i64 {
        self.by_category.get(cat.as_str()).copied().unwrap_or(0)
    }
}

/// Walks `root_path` and returns the aggregated composition [`Counts`], or
/// `None` when the path cannot be read.
fn composition_counts(root_path: &str) -> Option<Counts> {
    composition_counts_opts(root_path, &Options::default())
}

/// [`composition_counts`] with explicit path-policy options (the plot path
/// passes the run flags; the stdout formats keep the defaults).
fn composition_counts_opts(root_path: &str, opts: &Options) -> Option<Counts> {
    let root = Path::new(root_path);
    if !root.exists() {
        return None;
    }

    let classifier = Classifier::new();
    let mut counts = Counts::default();

    walk(root, &classifier, opts, &mut counts);
    Some(counts)
}

/// Builds the AGGREGATED RAW `analyze.Report` GoValue for `static/composition`
/// (`composition.Aggregator.GetResult`) — composition registers NO plot
/// section renderer in the reference implementation, so `--format plot` renders no page for it; this raw
/// value is what `writeReportJSON` serializes into `report.json`.
#[must_use]
pub fn composition_raw_report_value(root_path: &str, opts: &Options) -> Option<GoValue> {
    let counts = composition_counts_opts(root_path, opts)?;
    Some(build_raw_report(&counts))
}

/// Builds the `static/composition --format json` report bytes for `root_path`,
/// or `None` when the path cannot be read (the reference implementation would surface a walk error; the
/// caller then falls through to the blocked-dependency sentinel).
#[must_use]
pub fn composition_report(root_path: &str) -> Option<Vec<u8>> {
    let counts = composition_counts(root_path)?;
    let report = build_json_report(&counts);
    let bytes = Encoder::indented("  ")
        .with_trailing_newline(true)
        .encode_to_vec(&report);
    Some(bytes)
}

/// Builds the `static/composition` `renderer.JSONReport` GoValue (single
/// info-only section), shared by the single-analyzer byte path and the
/// multi-analyzer static-JSON merge. `None` when the path cannot be walked.
#[must_use]
pub fn composition_report_value(root_path: &str) -> Option<GoValue> {
    let counts = composition_counts(root_path)?;
    Some(build_json_report(&counts))
}

/// Builds the `static/composition --format bin` report bytes for `root_path`,
/// or `None` when the path cannot be read.
///
/// The reference per-analyzer binary path (`StaticService.FormatPerAnalyzer` →
/// `composition.Analyzer.FormatReportBinary` →
/// `reportutil.EncodeBinaryEnvelope(report)`) wraps the analyzer's **raw**
/// aggregated `analyze.Report` — NOT the renderer JSON section structure used by
/// the JSON capture. That report is `composition.Aggregator.GetResult`:
/// `{breakdown: map[string]int, percentages: map[string]float64, total_files:
/// int}`, where `breakdown` lists every category (zero counts included) and
/// `percentages` is `count/total*100` (omitted entirely when `total_files == 0`).
/// The payload is the compact `encoding/json` marshal of this `map[string]any`
/// (top-level + nested map keys byte-sorted) inside the CFB1 envelope.
#[must_use]
pub fn composition_bin(root_path: &str) -> Option<Vec<u8>> {
    let counts = composition_counts(root_path)?;
    let report = build_raw_report(&counts);
    let bytes = cf_reportutil::encode_binary_envelope(&report)
        .expect("composition payload within CFB1 limit");
    Some(bytes)
}

/// Builds the `static/composition --format yaml` report bytes for `root_path`,
/// or `None` when the path cannot be read.
///
/// The reference per-analyzer YAML path (`StaticService.FormatPerAnalyzer` →
/// `composition.Analyzer.FormatReportYAML` → `yaml.NewEncoder(w).Encode(report)`)
/// marshals the analyzer's **raw** aggregated `analyze.Report` —
/// `composition.Aggregator.GetResult` (`breakdown` / `percentages` /
/// `total_files`) — exactly the same report value the `bin` capture wraps, only
/// encoded as gopkg.in/yaml.v3 block YAML (4-space indent, byte-sorted map keys)
/// via `cf-goyaml::marshal` instead of the CFB1 JSON envelope.
#[must_use]
pub fn composition_yaml(root_path: &str) -> Option<Vec<u8>> {
    let counts = composition_counts(root_path)?;
    let report = build_raw_report(&counts);
    Some(cf_goyaml::marshal(&report))
}

/// Builds the raw aggregated `analyze.Report` GoValue
/// (`composition.Aggregator.GetResult`) as a map-origin value: top-level keys
/// `breakdown`, `percentages`, `total_files` and every-category nested maps,
/// all byte-sorted by the encoder.
fn build_raw_report(counts: &Counts) -> GoValue {
    let total = counts.total_files;

    // breakdown: map[string]int over every category (zero counts included).
    let mut breakdown = GoMap::new(MapOrigin::Map);
    for cat in ALL_CATEGORIES {
        breakdown.push(cat.as_str(), GoValue::Int(counts.get(cat)));
    }

    // percentages: map[string]float64 over every category, count/total*100;
    // omitted entirely when total_files == 0 (reference: leaves the map empty).
    let mut percentages = GoMap::new(MapOrigin::Map);
    if total > 0 {
        for cat in ALL_CATEGORIES {
            let value = (counts.get(cat) as f64) / (total as f64) * 100.0;
            percentages.push(cat.as_str(), GoValue::Float(value));
        }
    }

    let mut report = GoMap::new(MapOrigin::Map);
    report.push("breakdown", GoValue::Map(breakdown));
    report.push("percentages", GoValue::Map(percentages));
    report.push("total_files", GoValue::Int(total));
    GoValue::Map(report)
}

/// Recursively walks `dir` in lexical order, mirroring `filepath.WalkDir`:
/// directories are recursed (except `.git`), files are filtered through the
/// path policy and classified.
fn walk(dir: &Path, classifier: &Classifier, opts: &Options, counts: &mut Counts) {
    let Ok(read) = fs::read_dir(dir) else {
        // Permission / not-exist errors are skipped.
        return;
    };

    let mut entries: Vec<_> = read.filter_map(Result::ok).collect();
    // filepath.WalkDir sorts entries by name (os.ReadDir guarantees sorted).
    entries.sort_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };

        if file_type.is_dir() {
            if super::should_skip_walk_dir(&entry.path(), &entry.file_name()) {
                continue; // filepath.SkipDir on .git
            }
            walk(&path, classifier, opts, counts);
            continue;
        }

        // Only regular files participate. `--languages` is empty → all match.
        let path_str = path.to_string_lossy();

        // pathpolicy.Exclude(path, nil, opts): the raw-file phase passes nil
        // content, so only path-based vendor/generated heuristics apply.
        if exclude(&path_str, None, opts) {
            continue;
        }

        let header = read_header(&path);
        let category = classifier.classify(&path_str, &header);
        counts.total_files += 1;
        *counts.by_category.entry(category.as_str()).or_insert(0) += 1;
    }
}

/// Reads up to [`CONTENT_HEADER_SIZE`] bytes from `path`; returns empty on error
/// (the reference `readFileHeader` returns nil, which classifies as empty content).
fn read_header(path: &Path) -> Vec<u8> {
    use std::io::Read;
    let Ok(mut f) = fs::File::open(path) else {
        return Vec::new();
    };
    let mut buf = vec![0u8; CONTENT_HEADER_SIZE];
    match f.read(&mut buf) {
        Ok(n) => {
            buf.truncate(n);
            buf
        }
        Err(_) => Vec::new(),
    }
}

/// Builds the `renderer.JSONReport` GoValue for a single composition section.
///
/// Field order mirrors the reference structs exactly (struct-origin maps keep
/// declaration order): `overall_score_label`, `sections`, `overall_score`; and
/// per section `title`, `score_label`, `status`, `metrics`, `distribution`
/// (omitted when empty), `issues`, `score`.
fn build_json_report(counts: &Counts) -> GoValue {
    // Single info-only section → overall score is ScoreInfoOnly (-1), label Info.
    let mut report = GoMap::new(MapOrigin::Struct);
    report.push(
        "overall_score_label",
        GoValue::Str(SCORE_LABEL_INFO.to_string()),
    );
    report.push("sections", GoValue::Array(vec![build_section(counts)]));
    report.push("overall_score", GoValue::Float(SCORE_INFO_ONLY));
    GoValue::Map(report)
}

/// Builds the single composition `renderer.JSONSection` GoValue (reference:
/// `SectionToJSON`). Field order mirrors the reference struct: `title`, `score_label`,
/// `status`, `metrics`, `distribution` (omitted when empty), `issues`, `score`.
fn build_section(counts: &Counts) -> GoValue {
    let total = counts.total_files;
    let source_count = counts.get(Category::Source);

    // ---- metrics ----
    let metrics = GoValue::Array(vec![
        metric(METRIC_TOTAL_FILES, &total.to_string()),
        metric(METRIC_SOURCE, &source_count.to_string()),
        metric(METRIC_SOURCE_PCT, &format_percent(pct(source_count, total))),
    ]);

    // ---- distribution (ALL_CATEGORIES order, non-zero counts) ----
    let mut dist_items = Vec::new();
    if total != 0 {
        for cat in ALL_CATEGORIES {
            let count = counts.get(cat);
            if count == 0 {
                continue;
            }
            let mut d = GoMap::new(MapOrigin::Struct);
            d.push("label", GoValue::Str(cat.as_str().to_string()));
            d.push("percent", GoValue::Float(pct(count, total)));
            d.push("count", GoValue::Int(count));
            dist_items.push(GoValue::Map(d));
        }
    }

    // ---- issues (non-source categories; `[]` when none) ----
    let mut issue_items = Vec::new();
    if total != 0 {
        for cat in ALL_CATEGORIES {
            if cat == Category::Source {
                continue;
            }
            let count = counts.get(cat);
            if count == 0 {
                continue;
            }
            let percent = (count as f64) / (total as f64) * 100.0;
            let mut iss = GoMap::new(MapOrigin::Struct);
            iss.push("name", GoValue::Str(cat.as_str().to_string()));
            iss.push("location", GoValue::Str(String::new()));
            iss.push(
                "value",
                GoValue::Str(format!("{count} files ({percent:.1}%)")),
            );
            iss.push("severity", GoValue::Str(severity_for(cat).to_string()));
            issue_items.push(GoValue::Map(iss));
        }
    }

    let status = if total == 0 {
        STATUS_EMPTY
    } else {
        STATUS_DEFAULT
    };

    // ---- section ----
    let mut section = GoMap::new(MapOrigin::Struct);
    section.push("title", GoValue::Str(SECTION_TITLE.to_string()));
    section.push("score_label", GoValue::Str(SCORE_LABEL_INFO.to_string()));
    section.push("status", GoValue::Str(status.to_string()));
    section.push("metrics", metrics);
    // `distribution` carries `omitempty`: omit the key entirely when empty.
    if !dist_items.is_empty() {
        section.push("distribution", GoValue::Array(dist_items));
    }
    section.push("issues", GoValue::Array(issue_items));
    // `files` is a nil `*[]JSONFileEntry` (omitempty) → omitted (no per-file).
    section.push("score", GoValue::Float(SCORE_INFO_ONLY));

    GoValue::Map(section)
}

fn metric(label: &str, value: &str) -> GoValue {
    let mut m = GoMap::new(MapOrigin::Struct);
    m.push("label", GoValue::Str(label.to_string()));
    m.push("value", GoValue::Str(value.to_string()));
    GoValue::Map(m)
}

/// The reference `reportutil.Pct`: fraction in [0,1] (NOT a percentage).
fn pct(count: i64, total: i64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    (count as f64) / (total as f64)
}

/// The reference `reportutil.FormatPercent`: `%.1f%%` over `v*100`.
fn format_percent(v: f64) -> String {
    format!("{:.1}%", v * 100.0)
}

fn severity_for(cat: Category) -> &'static str {
    match cat {
        Category::Binary => SEVERITY_POOR,
        _ => SEVERITY_INFO,
    }
}
