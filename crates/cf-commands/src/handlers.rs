//! Analyzer report handlers + the default registry wiring.
//!
//! Each submodule is the crate-owned orchestration for one analyzer family
//! (static folder walk + per-file UAST analysis, or one git revwalk + per-commit
//! analysis). The analyzer MATH stays in the cf-* analyzer crates these call
//! (cf-complexity, cf-halstead, cf-comments, cf-imports, cf-couples,
//! cf-shotness, cf-analyzer-burndown, …); this module owns only the
//! pipeline-tier orchestration + serialization (cf-gojson / cf-goyaml /
//! cf-reportutil).
//!
//! [`default_registry`] builds the single [`crate::pipeline::Registry`] mapping
//! every analyzer id to ONE [`crate::pipeline::RunHandler`]. Each handler owns
//! its own `match format`; dispatch in [`crate::pipeline::run_pipeline`] is a
//! keyed lookup by id, NOT a per-format branch ladder.
//!
//! The shared run validation ([`preflight`]), the ONE static file walk
//! ([`walk_source_files`] / [`walk_all_files`]) and the two walk filters
//! ([`StaticFilter`], [`HistoryFilter`]) live here because every handler of
//! both phases reads them.

pub mod burndown_ndjson;
pub mod couples_run;
pub mod history;
pub mod history_formats;
pub mod history_plot;
pub mod history_text;
pub mod plot;
pub mod plot_sections;
pub mod section_render;
pub mod shotness_run;
pub(crate) mod spill;
pub mod static_clones;
pub mod static_cohesion;
pub mod static_comments;
pub mod static_complexity;
pub mod static_complexity_bin;
pub mod static_complexity_yaml;
pub mod static_halstead;
pub mod static_imports;
pub mod static_json;
pub(crate) mod uast_walk;

use std::path::{Path, PathBuf};

use cf_uast::Parser;

use crate::pipeline::{AnalyzerEntry, HandlerError, Mode, Registry, RunContext};

/// Concurrency cap for independent per-analyzer tasks in the multi-analyzer
/// dispatch paths (combined render, plot orchestrators, the per-id pipeline).
/// Handlers are self-contained (own `Repository` handles, thread-local
/// parsers); the shared-UAST walk is pre-computed and counts as one task.
pub(crate) const ANALYZER_CONCURRENCY: usize = 4;

/// Runs `f(0..n)` across at most `cap` worker threads and returns the results
/// in index order, so the caller can WRITE its outputs in the existing
/// deterministic order while the independent computations overlap. With one
/// task (or `cap <= 1`) it degenerates to the sequential loop.
pub(crate) fn run_concurrent<T: Send>(
    n: usize,
    cap: usize,
    f: impl Fn(usize) -> T + Sync,
) -> Vec<T> {
    if n <= 1 || cap <= 1 {
        return (0..n).map(f).collect();
    }
    let workers = cap.min(n);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let slots: Vec<std::sync::Mutex<Option<T>>> =
        (0..n).map(|_| std::sync::Mutex::new(None)).collect();
    {
        let f = &f;
        let next = &next;
        let slots = &slots;
        std::thread::scope(|s| {
            for _ in 0..workers {
                s.spawn(move || loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if i >= n {
                        break;
                    }
                    let value = f(i);
                    *slots[i].lock().expect("task slot poisoned") = Some(value);
                });
            }
        });
    }
    slots
        .into_iter()
        .map(|slot| {
            slot.into_inner()
                .expect("task slot poisoned")
                .expect("every task index was dispatched exactly once")
        })
        .collect()
}

/// The process-wide cooperative cancellation flag, set by the SIGINT/SIGTERM
/// handlers installed in [`crate::install_signal_handlers`].
pub(crate) fn cancel_flag() -> &'static std::sync::Arc<std::sync::atomic::AtomicBool> {
    static FLAG: std::sync::OnceLock<std::sync::Arc<std::sync::atomic::AtomicBool>> =
        std::sync::OnceLock::new();
    FLAG.get_or_init(|| std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)))
}

/// True once SIGINT/SIGTERM was received: long-running walks poll this and
/// bail out so the process can exit promptly instead of grinding through the
/// rest of the tree.
pub(crate) fn run_cancelled() -> bool {
    cancel_flag().load(std::sync::atomic::Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// The ONE static file walk. Every `static/*` handler consumes it, so the
// skip set, the read cap, the `--languages` globs, the vendor / generated
// path policy and the generated-content marker are applied in exactly one
// place; a handler owns only what it does with the bytes.
// ---------------------------------------------------------------------------

/// Upper bound for a single source file on the static path (64 MiB). No
/// legitimate source file approaches this; without a bound, a pathological
/// file is fully read, tree-sitter-parsed (tree several times the source
/// size), lowered to a UAST, and — on the complexity path — deep-copied
/// again, so peak RSS is a large multiple of the largest file encountered.
/// Oversized files are skipped exactly like unreadable ones.
pub const DEFAULT_MAX_PARSE_FILE_BYTES: u64 = DEFAULT_MAX_PARSE_FILE_MIB as u64 * MIB_BYTES;

/// Default `--max-parse-file-size`, in MiB.
pub const DEFAULT_MAX_PARSE_FILE_MIB: i64 = 8;

/// Bytes in one MiB.
const MIB_BYTES: u64 = 1024 * 1024;

/// Parses `file` for an analyzer that looks only for the nodes `wanted`
/// accepts. A data file (enry types it as data) whose mapping cannot yield such
/// a node gets an empty file root instead of a parse: the analyzer would find
/// nothing either way, so its report is the same and the tree is never built.
///
/// Both checks are needed. enry's data type alone admits SQL and GraphQL, whose
/// mappings declare functions and operations, so the type alone would change
/// their reports.
///
/// # Errors
///
/// The parser's error for a file that must be parsed.
pub fn parse_unless_data(
    parser: &cf_uast::Parser,
    file: &SourceFile,
    wanted: &dyn Fn(&cf_uast::UastSpec) -> bool,
) -> Result<cf_uast::Node, cf_uast::ParseError> {
    let is_data = cf_langpath::language_by_path_with_content(&file.rel_path, &file.bytes)
        .and_then(|language| cf_langpath::language_type(&language))
        == Some(cf_langpath::LanguageType::Data);
    if is_data && !parser.may_yield(&file.rel_path, wanted) {
        return Ok(cf_uast::Node::with_token(cf_uast_node::UAST_FILE, ""));
    }
    parser.parse(&file.rel_path, &file.bytes)
}

/// [`parse_unless_data`] for the function analyzers.
///
/// # Errors
///
/// The parser's error for a file that must be parsed.
pub fn parse_for_functions(
    parser: &cf_uast::Parser,
    file: &SourceFile,
) -> Result<cf_uast::Node, cf_uast::ParseError> {
    parse_unless_data(parser, file, &cf_uast::spec_yields_function)
}

/// [`parse_unless_data`] for the import analyzer.
///
/// # Errors
///
/// The parser's error for a file that must be parsed.
pub fn parse_for_imports(
    parser: &cf_uast::Parser,
    file: &SourceFile,
) -> Result<cf_uast::Node, cf_uast::ParseError> {
    parse_unless_data(parser, file, &|spec| {
        cf_imports::IMPORT_TRIGGER_TYPES.contains(&spec.r#type.as_str())
    })
}

/// Bytes read per file by [`walk_all_files`]: the classification header
/// (binary sniffing, generated markers, shebangs and modelines all live
/// there), so classifying a tree never reads whole binaries.
pub const ALL_FILES_HEADER_BYTES: usize = 8192;

/// Why the static walk (or a handler's parse step) left a file out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// The file (or its metadata) could not be read.
    Unreadable,
    /// The file is larger than the run's parse size limit.
    Oversized,
    /// The parser has no usable grammar for the file.
    Unparseable,
}

impl SkipReason {
    /// The reason as the stderr warning prints it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unreadable => "unreadable",
            Self::Oversized => "larger than the parse size limit (--max-parse-file-size)",
            Self::Unparseable => "unparseable",
        }
    }
}

/// Every path skipped in this run. Keyed by path so a file is counted ONCE
/// however many analyzers walk it (a `-a 'static/*'` run walks the tree once
/// per analyzer) and the unreadable / oversized warning is printed once.
/// Skips used to be completely silent, so a permissions failure across half
/// a tree produced a plausible-looking but wrong report with no signal.
static SKIPPED_FILES: std::sync::Mutex<std::collections::BTreeSet<PathBuf>> =
    std::sync::Mutex::new(std::collections::BTreeSet::new());

/// Records a skipped file for the end-of-run summary. The first skip of a
/// path for an I/O reason (unreadable, oversized) prints
/// `warning: skipped <path>: <reason>` on stderr; a parse failure is only
/// counted, because the handlers that hit it either list the file as empty
/// or drop it by design. Returns `true` when this call recorded the path
/// (and so printed its warning), `false` for a repeat.
pub(crate) fn note_skipped(path: &Path, reason: SkipReason) -> bool {
    let newly_recorded = SKIPPED_FILES
        .lock()
        .expect("skipped-file registry poisoned")
        .insert(path.to_path_buf());
    if newly_recorded && reason != SkipReason::Unparseable {
        eprintln!("warning: skipped {}: {}", path.display(), reason.as_str());
    }
    newly_recorded
}

/// Number of distinct files skipped so far in this run.
pub(crate) fn skipped_file_count() -> u64 {
    SKIPPED_FILES
        .lock()
        .expect("skipped-file registry poisoned")
        .len() as u64
}

/// True once `path` has been recorded as skipped in this process.
#[cfg(test)]
fn was_skipped(path: &Path) -> bool {
    SKIPPED_FILES
        .lock()
        .expect("skipped-file registry poisoned")
        .contains(path)
}

/// Reads a source file whole, refusing files over `limit` bytes before any
/// byte is read.
fn read_source_capped(path: &Path, limit: u64) -> Result<Vec<u8>, SkipReason> {
    let meta = std::fs::metadata(path).map_err(|_| SkipReason::Unreadable)?;
    if meta.len() > limit {
        return Err(SkipReason::Oversized);
    }
    std::fs::read(path).map_err(|_| SkipReason::Unreadable)
}

/// Reads at most `limit` leading bytes of `path`; empty when the file cannot
/// be read (the all-files walk then classifies on the path alone).
fn read_header(path: &Path, limit: usize) -> Vec<u8> {
    use std::io::Read as _;
    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let mut buf = Vec::new();
    if file.take(limit as u64).read_to_end(&mut buf).is_err() {
        return Vec::new();
    }
    buf
}

/// One file admitted by the static walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    /// The file's path as the walk reached it (the root joined with the
    /// relative components; the root itself for a FILE root).
    pub path: PathBuf,
    /// Slash-joined path relative to the walk root. A FILE root stamps the
    /// file's name, so `codefang run -a static/comments src/a.go` reports
    /// `a.go`, never `.`.
    pub rel_path: String,
    /// The file's bytes: the whole file under [`walk_source_files`], the
    /// first [`ALL_FILES_HEADER_BYTES`] under [`walk_all_files`].
    pub bytes: Vec<u8>,
    /// The parser's language name for the file (empty under
    /// [`walk_all_files`], whose consumer classifies the language itself).
    pub language: String,
}

impl SourceFile {
    /// Directory portion of [`Self::rel_path`] (`.` for a top-level file).
    #[must_use]
    pub fn directory(&self) -> String {
        parent_dir(&self.rel_path)
    }
}

/// Which files a [`StaticWalk`] admits.
enum WalkMode<'a> {
    /// Files the parser supports that pass the whole run filter — the
    /// `--languages` globs, the vendor / generated path policy and the
    /// generated-content marker — read whole.
    Source(&'a Parser),
    /// Every regular file that passes the `--languages` globs and
    /// `--extra-excluded-prefixes`; vendored, generated and binary files are
    /// admitted (composition exists to report them) and only the header is
    /// read.
    AllFiles,
}

/// The lazy static file walk. The candidate paths are gathered up front
/// (lexical, depth-first); each [`Iterator::next`] reads, filters and stamps
/// one file, so one file's bytes are live at a time however large the tree.
pub struct StaticWalk<'a> {
    root: PathBuf,
    mode: WalkMode<'a>,
    filter: StaticFilter,
    pending: std::vec::IntoIter<PathBuf>,
}

/// The source walk every UAST-backed static analyzer runs on. `root` is a
/// directory or a single file. The walk:
///
///  1. visits directories in lexical order, depth-first, skipping `.git`,
///     cache-tagged directories ([`should_skip_walk_dir`]) and symlinked
///     directories (never followed, so a symlink loop cannot recurse and the
///     walk cannot escape the root); symlinked regular files are read;
///  2. admits only files the parser supports and the `--languages` basename
///     globs keep;
///  3. applies the vendor / generated / `--extra-excluded-prefixes` policy to
///     the ROOT-RELATIVE path, so `--extra-excluded-prefixes gen/` works
///     whatever the root spelling;
///  4. reads each file once, refusing files over the filter's parse size limit;
///     an unreadable or oversized file is skipped and reported once on
///     stderr for the whole run ([`note_skipped`]);
///  5. applies the generated-content marker over the first 512 bytes
///     ([`StaticFilter::skips_with_header`]) unless `--include-generated`.
///
/// # Errors
///
/// [`HandlerError::PathNotFound`] when `root` does not exist.
pub fn walk_source_files<'a>(
    root: &str,
    parser: &'a Parser,
    filter: &StaticFilter,
) -> Result<StaticWalk<'a>, HandlerError> {
    StaticWalk::new(root, WalkMode::Source(parser), filter.clone())
}

/// The classification walk `static/composition` runs on: the same traversal
/// as [`walk_source_files`] but EVERY regular file is admitted — vendored,
/// generated, binary, unsupported — with only `--languages` and
/// `--extra-excluded-prefixes` narrowing it. Each file carries its first
/// [`ALL_FILES_HEADER_BYTES`]; an unreadable file carries no bytes and is
/// still admitted (it classifies on its path).
///
/// # Errors
///
/// [`HandlerError::PathNotFound`] when `root` does not exist.
pub fn walk_all_files(
    root: &str,
    filter: &StaticFilter,
) -> Result<StaticWalk<'static>, HandlerError> {
    let relaxed = StaticFilter {
        policy: cf_pathpolicy::Options {
            include_vendored: true,
            include_generated: true,
            extra_excluded_prefixes: filter.policy.extra_excluded_prefixes.clone(),
        },
        language_globs: filter.language_globs.clone(),
        max_parse_bytes: DEFAULT_MAX_PARSE_FILE_BYTES,
    };
    StaticWalk::new(root, WalkMode::AllFiles, relaxed)
}

impl<'a> StaticWalk<'a> {
    fn new(root: &str, mode: WalkMode<'a>, filter: StaticFilter) -> Result<Self, HandlerError> {
        let root_path = PathBuf::from(root);
        if !root_path.exists() {
            return Err(HandlerError::PathNotFound(root.to_string()));
        }
        let mut pending = Vec::new();
        collect_candidates(&root_path, &mut pending);
        Ok(StaticWalk {
            root: root_path,
            mode,
            filter,
            pending: pending.into_iter(),
        })
    }

    /// Filters, reads and stamps one candidate; `None` when the run filter
    /// drops it or it cannot be read.
    fn admit(&self, path: &Path) -> Option<SourceFile> {
        let rel_path = make_relative_path(path, &self.root);
        match self.mode {
            WalkMode::Source(parser) => self.admit_source(parser, path, rel_path),
            WalkMode::AllFiles => self.admit_any(path, rel_path),
        }
    }

    fn admit_source(&self, parser: &Parser, path: &Path, rel_path: String) -> Option<SourceFile> {
        if !parser.is_supported(&rel_path) || self.filter.skips(&rel_path) {
            return None;
        }
        let bytes = match read_source_capped(path, self.filter.max_parse_bytes) {
            Ok(bytes) => bytes,
            Err(reason) => {
                note_skipped(path, reason);
                return None;
            }
        };
        if self.filter.skips_with_header(&rel_path, &bytes) {
            return None;
        }
        Some(SourceFile {
            path: path.to_path_buf(),
            language: parser.get_language(&rel_path),
            rel_path,
            bytes,
        })
    }

    fn admit_any(&self, path: &Path, rel_path: String) -> Option<SourceFile> {
        if self.filter.skips(&rel_path) {
            return None;
        }
        Some(SourceFile {
            path: path.to_path_buf(),
            rel_path,
            bytes: read_header(path, ALL_FILES_HEADER_BYTES),
            language: String::new(),
        })
    }
}

impl Iterator for StaticWalk<'_> {
    type Item = SourceFile;

    /// The next admitted file; the walk ends early (and stays ended) once
    /// SIGINT/SIGTERM was received.
    fn next(&mut self) -> Option<SourceFile> {
        loop {
            if run_cancelled() {
                return None;
            }
            let path = self.pending.next()?;
            if let Some(file) = self.admit(&path) {
                return Some(file);
            }
        }
    }
}

/// What a directory entry contributes to the walk.
enum CandidateKind {
    /// A directory to recurse into (unless skipped).
    Dir,
    /// A regular file (or a symlink resolving to one) to analyze.
    File,
    /// Anything else: a symlinked or dangling directory, a socket, a FIFO…
    Other,
}

/// Classifies a directory entry WITHOUT following directory symlinks: a
/// symlink to a regular file is a file; a symlink to a directory — or a
/// dangling one — contributes nothing.
fn candidate_kind(entry: &std::fs::DirEntry) -> CandidateKind {
    let Ok(file_type) = entry.file_type() else {
        return CandidateKind::Other;
    };
    if file_type.is_dir() {
        return CandidateKind::Dir;
    }
    if file_type.is_file() {
        return CandidateKind::File;
    }
    if file_type.is_symlink() && std::fs::metadata(entry.path()).is_ok_and(|m| m.is_file()) {
        return CandidateKind::File;
    }
    CandidateKind::Other
}

/// Gathers the candidate files under `root` in lexical depth-first order: a
/// FILE root is the single candidate; directories are recursed except the
/// [`should_skip_walk_dir`] set; symlinked directories are never followed.
/// Stops promptly on SIGINT/SIGTERM.
fn collect_candidates(root: &Path, out: &mut Vec<PathBuf>) {
    if root.is_file() {
        out.push(root.to_path_buf());
        return;
    }
    let Ok(read) = std::fs::read_dir(root) else {
        return;
    };
    let mut entries: Vec<_> = read.filter_map(Result::ok).collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        if run_cancelled() {
            return;
        }
        let path = entry.path();
        match candidate_kind(&entry) {
            CandidateKind::Dir => {
                if !should_skip_walk_dir(&path, &entry.file_name()) {
                    collect_candidates(&path, out);
                }
            }
            CandidateKind::File => out.push(path),
            CandidateKind::Other => {}
        }
    }
}

/// The slash-joined path of `path` relative to `root`. A FILE root (the
/// path IS the root) stamps the file's name, as does a path outside the
/// root, so a stamp is never `.` or empty.
#[must_use]
pub fn make_relative_path(path: &Path, root: &Path) -> String {
    if let Ok(rel) = path.strip_prefix(root) {
        if !rel.as_os_str().is_empty() {
            return rel.to_string_lossy().into_owned();
        }
    }
    path.file_name().map_or_else(
        || path.to_string_lossy().into_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// Directory portion of a relative stamp (`.` for a bare filename, `/` for
/// a root-anchored one).
#[must_use]
pub fn parent_dir(relative: &str) -> String {
    match relative.rfind('/') {
        Some(0) => "/".to_string(),
        Some(pos) => relative[..pos].to_string(),
        None => ".".to_string(),
    }
}

/// Directory-level skip predicate for the static folder walks: `.git` and any
/// directory carrying a `CACHEDIR.TAG` — the BSD cache-directory convention
/// Cargo writes into `target/` (and that ripgrep/fd/tar honor). A bare
/// `codefang run .` in a built Rust repo would otherwise descend into a
/// multi-gigabyte `target/` tree and parse its generated sources, which looks
/// like a hang. The tag check has effectively zero false positives (a real
/// source directory never carries the signature).
pub(crate) fn should_skip_walk_dir(path: &std::path::Path, name: &std::ffi::OsStr) -> bool {
    name == ".git" || is_cache_tagged_dir(path)
}

/// True when `dir` holds a `CACHEDIR.TAG` whose leading bytes are the standard
/// cache signature (per <https://bford.info/cachedir/>).
fn is_cache_tagged_dir(dir: &std::path::Path) -> bool {
    use std::io::Read as _;
    const SIG: &[u8] = b"Signature: 8a477f597d28d172789f06886806bc55";
    let Ok(mut f) = std::fs::File::open(dir.join("CACHEDIR.TAG")) else {
        return false;
    };
    let mut buf = [0u8; SIG.len()];
    f.read_exact(&mut buf).is_ok() && buf[..] == *SIG
}

// ---------------------------------------------------------------------------
// Shared pipeline helpers (path resolution, commit window, RFC3339 formatting).
// ---------------------------------------------------------------------------

/// Resolves the repository path from `run`'s positional arg or `-p/--path`
/// (the positional wins when present, else `--path`, default `.`).
#[must_use]
pub fn run_repo_path(sub: &clap::ArgMatches) -> String {
    if let Some(p) = sub.get_one::<String>("path-positional") {
        if !p.is_empty() {
            return p.clone();
        }
    }
    sub.get_one::<String>("path")
        .cloned()
        .unwrap_or_else(|| ".".to_string())
}

/// The `--since` state of a history run: raw spec text, resolved against the
/// repository when the walk is built.
///
/// Accepted forms, in precedence order:
///  1. a duration (`24h`, `1h30m`) subtracted from "now" — units are
///     `h`/`m`/`s` only, so `m` means MINUTES and there is no `d`/`w` unit;
///     "now" honors `CODEFANG_NOW` so duration-relative runs stay reproducible
///     (DESIGN §2.8);
///  2. an RFC3339 timestamp (`2024-01-01T00:00:00Z`, numeric offsets honored);
///  3. a plain UTC date (`2024-01-01`, interpreted as midnight UTC);
///  4. any git revision (`v1.2.0`, `HEAD~100`, a bare SHA) — the referenced
///     commit's AUTHOR time becomes the cutoff.
///
/// Commits whose author time is at/after the cutoff are analyzed. A spec that
/// matches none of the forms aborts the run with empty stdout and exit 1; the
/// `run` command names the flag (`Error: cannot resolve --since "<value>"`)
/// before any walking starts.
///
/// Parsing lives in [`cf_gitlib::helpers::parse_time`] /
/// [`cf_gitlib::Repository::resolve_time`] — ONE parser shared by the CLI and
/// the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SinceSpec {
    /// `--since` absent or empty: the walk is unfiltered.
    Inactive,
    /// Raw `--since` text, resolved against the repository at walk time.
    Active(String),
}

/// Resolves the run's `--since` flag into a [`SinceSpec`] (empty ⇒ inactive).
#[must_use]
pub fn history_since_spec(sub: &clap::ArgMatches) -> SinceSpec {
    match sub.get_one::<String>("since") {
        Some(raw) if !raw.is_empty() => SinceSpec::Active(raw.clone()),
        _ => SinceSpec::Inactive,
    }
}

/// The commit window of a history run, as selected by `--head`, `--limit`,
/// `--since` and the walk's first-parent mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryWindow {
    /// `--head`: the window is exactly `[HEAD]`; the other bounds are ignored.
    pub head: bool,
    /// `--limit N`: the N **newest** reachable commits; `0` = the whole walk.
    pub limit: usize,
    /// Follow only the first parent of merge commits.
    pub first_parent: bool,
    /// `--since`: every commit with author time >= the cutoff.
    pub since: SinceSpec,
}

/// Warns, once per process, that a shallow clone has no walkable history.
///
/// A shallow repository yields an empty window (see
/// [`cf_gitlib::Repository::log`]), so every history analyzer reports empty
/// results. The warning names the cause and the remedy so the empty report is
/// not mistaken for an analysis result.
fn warn_shallow_history_once() {
    static WARNED: std::sync::Once = std::sync::Once::new();
    WARNED.call_once(|| {
        eprintln!(
            "warning: shallow clone: history analyzers see no commits; \
             run `git fetch --unshallow` for a full history"
        );
    });
}

/// The commit window selected by a [`HistoryWindow`].
///
/// Returns hashes in the **oldest-first** order the history analyzers consume
/// (tick assignment, burndown line survival and identity ids are all
/// order-dependent). Window semantics:
///  - `head` → exactly `[HEAD]`, the same single-commit window `--limit 1`
///    selects, so the two are byte-identical for every analyzer and format;
///  - `limit N` → the N **newest** reachable commits (the `git log -n`
///    convention); `0` → the whole walk;
///  - `since C` → every commit with author time >= C. A cutoff after HEAD
///    yields an EMPTY window (analyzers emit their empty report); a cutoff at
///    or before the root commit filters nothing. Author time can be skewed
///    against the committer-time walk order, so the cutoff is a SKIP filter
///    over the whole walk, never a stop condition;
///  - both → the newest `N` commits *inside* the `since` window.
///
/// Returns `None` (⇒ the run aborts) when the repository cannot be walked or
/// `since` cannot be resolved against it — the CLI preflight reports the
/// latter by name, so this is the library-level contract.
#[must_use]
pub fn load_history_commit_hashes(
    repo: &cf_gitlib::Repository,
    window: &HistoryWindow,
) -> Option<Vec<cf_gitlib::Hash>> {
    use cf_gitlib::repository::{time_from_unix_secs, LogOptions};

    if window.head {
        return Some(vec![repo.head().ok()?]);
    }
    if repo.is_shallow() {
        warn_shallow_history_once();
    }

    let since_secs = match &window.since {
        SinceSpec::Inactive => None,
        SinceSpec::Active(spec) => Some(repo.resolve_time(spec).ok()?),
    };

    let log_opts = LogOptions {
        since: since_secs.map(time_from_unix_secs),
        first_parent: window.first_parent,
        reverse: false,
    };
    let mut iter = repo.log(&log_opts).ok()?;
    let mut hashes = Vec::new();
    while window.limit == 0 || hashes.len() < window.limit {
        match iter.next_commit() {
            Some(c) => hashes.push(c.hash()),
            None => break,
        }
    }
    hashes.reverse();
    Some(hashes)
}

/// Formats Unix seconds as RFC3339 in the zone given by `offset_minutes`; a
/// zero offset prints the literal `Z`.
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

/// Serializes a history analyzer's report value across the json/yaml/bin
/// machine formats: the *same* report value is encoded each way, so a handler
/// computes the value once and routes it here rather than re-deriving per
/// format.
///
/// - `json`: `json_value`, no header, no trailing newline (cf-gojson `marshal`).
/// - `binary`: a CFB1 envelope wrapping the same `json_value` bytes.
/// - `yaml`: the `codefang (v2):` / version / hash header, then
///   `<analyzer_name>:\n`, then the yaml body of `yaml_value`.
///
/// `analyzer_name` is the history analyzer's display name (the YAML section
/// header); `analyzer_id` names the analyzer in the error.
///
/// # Errors
///
/// [`HandlerError::FormatNotSupported`] for any other format.
pub fn serialize_history_metrics(
    format: &str,
    analyzer_id: &str,
    analyzer_name: &str,
    json_value: &cf_gojson::GoValue,
    yaml_value: &cf_gojson::GoValue,
) -> Result<Vec<u8>, HandlerError> {
    match format {
        "json" => Ok(cf_gojson::marshal(json_value)),
        "binary" | "bin" => {
            cf_reportutil::encode_binary_envelope(json_value).map_err(|_| HandlerError::NoReport {
                analyzer: analyzer_id.to_string(),
                format: format.to_string(),
            })
        }
        "yaml" => {
            let mut out = Vec::new();
            out.extend_from_slice(b"codefang (v2):\n");
            out.extend_from_slice(
                format!("  version: {}\n", cf_version::DEFAULT_BINARY).as_bytes(),
            );
            out.extend_from_slice(format!("  hash: {}\n", cf_version::BINARY_GIT_HASH).as_bytes());
            out.extend_from_slice(format!("{analyzer_name}:\n").as_bytes());
            out.extend_from_slice(&cf_goyaml::marshal(yaml_value));
            Ok(out)
        }
        _ => Err(format_not_supported(format, analyzer_id)),
    }
}

/// The [`HandlerError::FormatNotSupported`] for `(format, analyzer)`.
fn format_not_supported(format: &str, analyzer: &str) -> HandlerError {
    HandlerError::FormatNotSupported {
        format: format.to_string(),
        analyzer: analyzer.to_string(),
    }
}

/// Maps a report builder's `None` to the generic [`HandlerError::NoReport`].
fn report_or_err(
    bytes: Option<Vec<u8>>,
    analyzer: &str,
    format: &str,
) -> Result<Vec<u8>, HandlerError> {
    bytes.ok_or_else(|| HandlerError::NoReport {
        analyzer: analyzer.to_string(),
        format: format.to_string(),
    })
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
// Run preflight: ONE place that validates the path, the repository, the
// flags and the format before any walk starts.
// ---------------------------------------------------------------------------

/// Every `--format` value codefang knows, across all phases.
const KNOWN_FORMATS: &[&str] = &[
    crate::formats::FORMAT_JSON,
    crate::formats::FORMAT_YAML,
    crate::formats::FORMAT_PLOT,
    crate::formats::FORMAT_BINARY,
    crate::formats::FORMAT_TIMESERIES,
    crate::formats::FORMAT_NDJSON,
    crate::formats::FORMAT_COMPACT,
    crate::formats::FORMAT_TEXT,
    crate::formats::FORMAT_TIMESERIES_NDJSON,
];

/// Validates a run BEFORE any walk: the path exists; a history selection
/// opens the repository, resolves `--since`, reads `--people-dict` and
/// validates `--limit` / `--tick-size` / `--max-changes-per-commit`; both
/// phases validate `--languages`; and the `--format` value is one every
/// selected phase can render. `static_ids` / `history_ids` are the resolved
/// selection (either may be empty).
///
/// # Errors
///
/// The [`HandlerError`] naming the first failing input.
pub fn preflight(
    ctx: &RunContext,
    static_ids: &[String],
    history_ids: &[String],
) -> Result<(), HandlerError> {
    if !std::path::Path::new(&ctx.path).exists() {
        return Err(HandlerError::PathNotFound(ctx.path.clone()));
    }
    if !static_ids.is_empty() {
        static_filter(ctx).map_err(HandlerError::InvalidFlag)?;
    }
    if !history_ids.is_empty() {
        history::limit_native_memory(ctx.matches);
        history::validate_history_run(ctx.matches)?;
    }
    preflight_format(ctx, static_ids, history_ids)?;
    if let Some(note) = per_file_ignored_note(
        !static_ids.is_empty(),
        ctx.matches.get_flag("per-file"),
        &ctx.raw_format(),
    ) {
        warn_once(&note);
    }
    Ok(())
}

/// The stderr note for a `--per-file` run whose format cannot carry per-file
/// entries: the flag is a `--format json` enrichment, and every other format
/// (`yaml`, `bin`, `text`, `compact`, `plot`) silently rendered without it.
/// `None` when no static analyzer is selected, the flag is off, or the
/// format is json.
#[must_use]
pub fn per_file_ignored_note(
    static_selected: bool,
    per_file: bool,
    format: &str,
) -> Option<String> {
    let is_json = crate::formats::normalize_format(format) == crate::formats::FORMAT_JSON;
    (static_selected && per_file && !is_json).then(|| {
        format!(
            "--per-file applies to --format json only; ignored for {}",
            format.trim()
        )
    })
}

/// Prints `note` on stderr once per process: the preflight runs in the CLI
/// entry point AND inside the library pipeline, and a multi-analyzer run
/// must not repeat the note per analyzer.
fn warn_once(note: &str) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| eprintln!("{note}"));
}

/// The format leg of [`preflight`]: `--ndjson` modifies `timeseries` only;
/// `plot` is routed by the command itself; everything else must resolve for
/// the selected phases.
fn preflight_format(
    ctx: &RunContext,
    static_ids: &[String],
    history_ids: &[String],
) -> Result<(), HandlerError> {
    let raw = ctx.raw_format();
    let normalized = crate::formats::normalize_format(&raw);
    if ctx.ndjson() && normalized != crate::formats::FORMAT_TIMESERIES {
        return Err(HandlerError::InvalidFlag(format!(
            "invalid --ndjson: requires --format timeseries, got --format {}",
            raw.trim()
        )));
    }
    if normalized == crate::formats::FORMAT_PLOT {
        return Ok(());
    }
    if !KNOWN_FORMATS.contains(&normalized.as_str()) {
        return Err(HandlerError::UnsupportedFormat(raw));
    }
    let has_static = !static_ids.is_empty();
    let has_history = !history_ids.is_empty();
    if crate::formats::resolve_formats(&raw, has_static, has_history).is_ok() {
        return Ok(());
    }
    let analyzer = if has_static && has_history {
        "a mixed static+history selection".to_string()
    } else {
        static_ids
            .iter()
            .chain(history_ids.iter())
            .next()
            .cloned()
            .unwrap_or_default()
    };
    Err(format_not_supported(&raw, &analyzer))
}

// ---------------------------------------------------------------------------
// Static analyzer glob/bin helpers (registry-ordered multi-analyzer bin output).
// ---------------------------------------------------------------------------

/// The static analyzers in registry order. The `bool` records whether the
/// analyzer's `--format bin` payload is produced (all are).
pub const STATIC_BIN_ANALYZERS: &[(&str, bool)] = &[
    ("static/clones", true),
    ("static/complexity", true),
    ("static/comments", true),
    ("static/halstead", true),
    ("static/cohesion", true),
    ("static/imports", true),
    ("static/composition", true),
];

/// True when `pat` is a literal static analyzer ID or a glob that could match
/// static IDs (and no history ID).
#[must_use]
pub fn is_static_id_or_glob(pat: &str) -> bool {
    if pat.contains(['*', '?', '[']) {
        let any_static = STATIC_BIN_ANALYZERS
            .iter()
            .any(|(id, _)| go_path_match(pat, id));
        any_static && !history_glob_matches(pat)
    } else {
        STATIC_BIN_ANALYZERS.iter().any(|(id, _)| *id == pat)
    }
}

/// True when the glob matches any known history analyzer ID.
#[must_use]
pub fn history_glob_matches(pat: &str) -> bool {
    HISTORY_COMBINED_ORDER
        .iter()
        .any(|id| go_path_match(pat, id))
}

/// Expands the requested patterns over the registry-ordered static analyzers and
/// concatenates each selected analyzer's CFB1 bin envelope. Returns `None` if
/// any selected analyzer cannot produce its envelope or any folder walk fails.
#[must_use]
pub fn static_multi_bin(patterns: &[&str], path: &str, filter: &StaticFilter) -> Option<Vec<u8>> {
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
        let env = static_single_bin(id, path, filter)?;
        out.extend_from_slice(&env);
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Static analyzer multi-analyzer JSON merge. For `codefang run --format json`
// with more than one static analyzer (or a glob that selects several), ONE
// merged report is rendered whose `sections` are the per-analyzer sections in
// REGISTRY order and whose `overall_score` is the executive-summary average of
// the scored sections (info-only sections, score < 0, are excluded; all-info ⇒
// overall is -1 / Info). Each analyzer's section value comes from its own
// crate-owned report builder, so the merge owns no analyzer math.
// ---------------------------------------------------------------------------

/// Registry-ordered map of static analyzer id → the crate-owned builder of
/// that analyzer's single-section report GoValue. The `bool` is the run's
/// `--per-file` flag; the [`StaticFilter`] is the run's shared walk filter.
type ReportValueFn = fn(&str, &StaticFilter, bool) -> Option<GoValue>;

use cf_gojson::{GoMap, GoValue, MapOrigin};

const STATIC_JSON_VALUE_BUILDERS: &[(&str, ReportValueFn)] = &[
    ("static/clones", static_clones::clones_report_value_flags),
    (
        "static/complexity",
        static_complexity::complexity_report_value_flags,
    ),
    (
        "static/comments",
        static_comments::comments_report_value_flags,
    ),
    (
        "static/halstead",
        static_halstead::halstead_report_value_flags,
    ),
    (
        "static/cohesion",
        static_cohesion::cohesion_report_value_flags,
    ),
    ("static/imports", static_imports::imports_report_value_flags),
    (
        "static/composition",
        static_json::composition_report_value_flags,
    ),
];

/// Pulls the `sections` array out of a single-analyzer report GoValue
/// (`{overall_score_label, sections, overall_score}`).
fn extract_sections(report: &GoValue) -> Vec<GoValue> {
    report
        .as_map()
        .and_then(|m| m.get("sections"))
        .and_then(|s| match s {
            GoValue::Array(items) => Some(items.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// Reads a section's numeric `score` field, defaulting to the info-only
/// sentinel `-1.0` when absent.
fn section_score(section: &GoValue) -> f64 {
    match section.as_map().and_then(|m| m.get("score")) {
        Some(GoValue::Float(f)) => *f,
        Some(GoValue::Int(i)) => *i as f64,
        _ => -1.0,
    }
}

/// `round(score*10)/10` → `"N/10"`; a negative score (info-only) renders
/// `"Info"`.
fn overall_score_label(score: f64) -> String {
    if score < 0.0 {
        return "Info".to_string();
    }
    let n = (score * 10.0).round() as i64;
    format!("{n}/10")
}

/// Expands `patterns` over the registry-ordered static analyzers and renders ONE
/// merged report: sections in registry order, `overall_score` the average of
/// the scored (`score >= 0`) sections (or `-1` when none are scored). `None`
/// if no static analyzer is selected or any selected analyzer cannot produce a
/// report.
#[must_use]
pub fn static_multi_json(
    patterns: &[&str],
    path: &str,
    filter: &StaticFilter,
    per_file: bool,
) -> Option<Vec<u8>> {
    let root = static_multi_report_value(patterns, path, filter, per_file)?;
    let bytes = cf_gojson::Encoder::indented("  ")
        .with_trailing_newline(true)
        .encode_to_vec(&root);
    Some(bytes)
}

/// The merged multi-static-analyzer `--format text` render: a ≥2-section
/// selection carries the executive-summary header followed by each analyzer's
/// section rendered EXACTLY as its solo `--format text` body — the sections
/// come from the same per-analyzer TEXT-shaped section values the solo text
/// handlers feed the renderer.
#[must_use]
pub fn static_multi_text(patterns: &[&str], path: &str, filter: &StaticFilter) -> Option<Vec<u8>> {
    type TextValueFn = fn(&str, &StaticFilter) -> Option<GoValue>;
    // Registry order (the summary-table order).
    const TEXT_VALUE_BUILDERS: &[(&str, TextValueFn)] = &[
        ("static/clones", |p, f| {
            static_clones::clones_report_value_flags(p, f, false)
        }),
        (
            "static/complexity",
            static_complexity::complexity_report_value_summary,
        ),
        (
            "static/comments",
            static_comments::comments_report_value_summary,
        ),
        (
            "static/halstead",
            static_halstead::halstead_report_value_summary,
        ),
        (
            "static/cohesion",
            static_cohesion::cohesion_report_value_summary,
        ),
        ("static/imports", |p, f| {
            static_imports::imports_report_value_flags(p, f, false)
        }),
        (
            "static/composition",
            static_json::composition_report_value_opts as TextValueFn,
        ),
    ];

    let mut sections: Vec<GoValue> = Vec::new();
    for &(id, build) in TEXT_VALUE_BUILDERS {
        let matched = patterns.iter().any(|pat| {
            if pat.contains(['*', '?', '[']) {
                go_path_match(pat, id)
            } else {
                *pat == id
            }
        });
        if !matched {
            continue;
        }
        let report = build(path, filter)?;
        sections.extend(extract_sections(&report));
    }
    if sections.is_empty() {
        return None;
    }

    let mut root = GoMap::new(MapOrigin::Struct);
    root.push("sections", GoValue::Array(sections));
    Some(section_render::render_text_report(&GoValue::Map(root)))
}

/// Builds the ONE merged report GoValue for a multi-static selection: sections
/// in registry order, `overall_score` the executive-summary average of the
/// scored sections. Every output format renders from this same value.
#[must_use]
fn static_multi_report_value(
    patterns: &[&str],
    path: &str,
    filter: &StaticFilter,
    per_file: bool,
) -> Option<GoValue> {
    let mut sections: Vec<GoValue> = Vec::new();
    let mut score_total = 0.0_f64;
    let mut score_count = 0_usize;

    for &(id, build) in STATIC_JSON_VALUE_BUILDERS {
        let matched = patterns.iter().any(|pat| {
            if pat.contains(['*', '?', '[']) {
                go_path_match(pat, id)
            } else {
                *pat == id
            }
        });
        if !matched {
            continue;
        }
        let report = build(path, filter, per_file)?;
        for section in extract_sections(&report) {
            let s = section_score(&section);
            if s >= 0.0 {
                score_total += s;
                score_count += 1;
            }
            sections.push(section);
        }
    }

    if sections.is_empty() {
        return None;
    }

    let overall = if score_count == 0 {
        -1.0
    } else {
        score_total / score_count as f64
    };

    let mut root = GoMap::new(MapOrigin::Struct);
    root.push(
        "overall_score_label",
        GoValue::Str(overall_score_label(overall)),
    );
    root.push("sections", GoValue::Array(sections));
    root.push("overall_score", GoValue::Float(overall));

    Some(GoValue::Map(root))
}

/// True when `patterns` select MORE THAN ONE static analyzer (a literal multi-id
/// list or a glob matching several), so the JSON path must merge sections rather
/// than emit a single-analyzer document.
#[must_use]
pub fn static_json_selects_multiple(patterns: &[&str]) -> bool {
    let mut matched = 0usize;
    for &(id, _) in STATIC_JSON_VALUE_BUILDERS {
        let hit = patterns.iter().any(|pat| {
            if pat.contains(['*', '?', '[']) {
                go_path_match(pat, id)
            } else {
                *pat == id
            }
        });
        if hit {
            matched += 1;
            if matched > 1 {
                return true;
            }
        }
    }
    false
}

/// Produces a single static analyzer's CFB1 bin envelope.
#[must_use]
pub fn static_single_bin(id: &str, path: &str, filter: &StaticFilter) -> Option<Vec<u8>> {
    match id {
        "static/clones" => static_clones::clones_report_bin(path, filter),
        "static/complexity" => static_complexity_bin::complexity_report_bin(path, filter),
        "static/comments" => static_comments::comments_report_bin(path, filter),
        "static/halstead" => static_halstead::halstead_bin_report(path, filter),
        "static/cohesion" => static_cohesion::cohesion_report_bin(path, filter),
        "static/imports" => static_imports::imports_report_bin(path, filter),
        "static/composition" => static_json::composition_bin_opts(path, filter),
        _ => None,
    }
}

/// The history analyzers in combined-model order (the order the combined
/// static+history render lists them). Used to expand `*`/globs.
pub const HISTORY_COMBINED_ORDER: &[&str] = &[
    "history/typos",
    "history/file-history",
    "history/imports",
    "history/shotness",
    "history/anomaly",
    "history/burndown",
    "history/couples",
    "history/devs",
    "history/quality",
    "history/sentiment",
];

/// The history analyzers in the *separate-phase* per-analyzer emit order — the
/// order a history-only selection (literal list or glob) writes each leaf's
/// standalone report.
pub const HISTORY_PHASE_EMIT_ORDER: &[&str] = &[
    "history/quality",
    "history/sentiment",
    "history/shotness",
    "history/couples",
    "history/imports",
    "history/typos",
    "history/anomaly",
    "history/burndown",
    "history/devs",
    "history/file-history",
];

/// Whether any requested pattern selects the analyzer `id`: a bare `*`
/// matches EVERY id (`path.Match("*", "history/typos")` is false — `*` does
/// not cross `/`); other globs use [`go_path_match`]; a literal id matches
/// exactly.
#[must_use]
fn pattern_selects_id(patterns: &[&str], id: &str) -> bool {
    let is_glob = |p: &str| p.contains(['*', '?', '[']);
    patterns.iter().any(|p| {
        if *p == "*" {
            true
        } else if is_glob(p) {
            go_path_match(p, id)
        } else {
            *p == id
        }
    })
}

/// Expands a requested pattern list into the concrete history leaf ids it
/// selects, in the separate-phase emit order ([`HISTORY_PHASE_EMIT_ORDER`]).
#[must_use]
pub fn expand_history_phase_ids(patterns: &[&str]) -> Vec<String> {
    let selected = |id: &str| pattern_selects_id(patterns, id);
    HISTORY_PHASE_EMIT_ORDER
        .iter()
        .filter(|id| selected(id))
        .map(|id| (*id).to_string())
        .collect()
}

/// Expands the requested analyzer patterns into concrete (static, history) id
/// lists in SELECTION order: patterns resolve IN ORDER (a literal id to
/// itself, a glob to the matching registry ids in registration order),
/// duplicates dropped first-wins, then ids are divided by mode preserving that
/// order. The plot path renders pages (and the index cards) in this order.
#[must_use]
pub fn expand_selection_ids(patterns: &[&str]) -> (Vec<String>, Vec<String>) {
    let is_glob = |p: &str| p.contains(['*', '?', '[']);
    let mut statics: Vec<String> = Vec::new();
    let mut history: Vec<String> = Vec::new();
    let push_unique = |list: &mut Vec<String>, id: &str| {
        if !list.iter().any(|have| have == id) {
            list.push(id.to_string());
        }
    };
    for pat in patterns {
        if is_glob(pat) {
            for (id, _) in STATIC_BIN_ANALYZERS {
                if *pat == "*" || go_path_match(pat, id) {
                    push_unique(&mut statics, id);
                }
            }
            for id in HISTORY_COMBINED_ORDER {
                if *pat == "*" || go_path_match(pat, id) {
                    push_unique(&mut history, id);
                }
            }
            continue;
        }
        // Literal id: itself, in its pattern position (unknown ids are kept
        // out; the caller surfaces the diagnostic).
        if STATIC_BIN_ANALYZERS.iter().any(|(id, _)| id == pat) {
            push_unique(&mut statics, pat);
        } else if HISTORY_COMBINED_ORDER.iter().any(|id| id == pat) {
            push_unique(&mut history, pat);
        }
    }
    (statics, history)
}

/// Expands the requested analyzer patterns into concrete (static, history) id
/// lists in combined-model order: static analyzers in [`STATIC_BIN_ANALYZERS`]
/// registry order, then history analyzers in [`HISTORY_COMBINED_ORDER`].
#[must_use]
pub fn expand_combined_ids(patterns: &[&str]) -> (Vec<String>, Vec<String>) {
    let matches = |id: &str| pattern_selects_id(patterns, id);
    let statics: Vec<String> = STATIC_BIN_ANALYZERS
        .iter()
        .filter(|(id, _)| matches(id))
        .map(|(id, _)| (*id).to_string())
        .collect();
    let history: Vec<String> = HISTORY_COMBINED_ORDER
        .iter()
        .filter(|id| matches(id))
        .map(|id| (*id).to_string())
        .collect();
    (statics, history)
}

/// Renders a multi-analyzer run as the single `codefang.run.v1` unified-model
/// document. Each selected analyzer is dispatched through its registry handler
/// with `--format bin`, producing a CFB1 envelope whose payload is the
/// analyzer's raw report JSON. The concatenated envelopes are decoded into a
/// [`cf_analyze::conversion::UnifiedModel`], stamped with run metadata, and
/// re-serialized in the requested `output_format` via
/// [`cf_analyze::conversion::write_converted_output`] so every machine format
/// (json/yaml/bin/ndjson/timeseries) follows from the one model value.
///
/// Used for the mixed static+history selection AND for a history-only
/// selection of several analyzers in `json`/`yaml`, so a multi-analyzer run
/// always emits ONE document (`{version, metadata, analyzers: [{id, mode,
/// report}]}`), never a concatenation.
///
/// # Errors
///
/// The first failing analyzer's [`HandlerError`], or a [`HandlerError::NoReport`]
/// when the model cannot be decoded / encoded.
pub fn render_combined(
    ctx: &RunContext,
    static_ids: &[String],
    history_ids: &[String],
    output_format: &str,
) -> Result<Vec<u8>, HandlerError> {
    let registry = default_registry();
    let mut ids: Vec<String> = Vec::new();
    let mut modes: Vec<cf_analyze::AnalyzerMode> = Vec::new();

    // Pre-compute the ONE shared UAST walk for the co-selected heavy history
    // analyzers before fanning out, so it runs as a single task.
    uast_walk::prewarm(ctx.matches);

    // Static phase, then history phase. Each handler is dispatched with the
    // normalized `binary` format; its CFB1 envelope is appended to the
    // combined buffer. The per-analyzer envelopes are independent, so they
    // are COMPUTED concurrently and appended in the deterministic
    // static-then-history order.
    let all_ids: Vec<&String> = static_ids.iter().chain(history_ids.iter()).collect();
    let envelopes: Vec<Result<Vec<u8>, HandlerError>> =
        run_concurrent(all_ids.len(), ANALYZER_CONCURRENCY, |i| {
            let entry = registry
                .lookup(all_ids[i])
                .ok_or_else(|| HandlerError::NoReport {
                    analyzer: all_ids[i].clone(),
                    format: crate::formats::FORMAT_BINARY.to_string(),
                })?;
            (entry.run)(ctx, crate::formats::FORMAT_BINARY)
        });
    // Sized once: a large report (538 MB of json for kubernetes history/*)
    // must not be copied while the buffer doubles.
    let total = envelopes
        .iter()
        .map(|env| env.as_ref().map_or(0, Vec::len))
        .sum();
    let mut raw: Vec<u8> = Vec::with_capacity(total);
    for (i, (id, env)) in all_ids.iter().zip(envelopes).enumerate() {
        raw.extend_from_slice(&env?);
        ids.push((*id).clone());
        modes.push(if i < static_ids.len() {
            cf_analyze::AnalyzerMode::static_mode()
        } else {
            cf_analyze::AnalyzerMode::history()
        });
    }

    let no_report = || HandlerError::NoReport {
        analyzer: ids.join(","),
        format: output_format.to_string(),
    };
    let mut model = cf_analyze::conversion::decode_combined_binary_reports(&raw, &ids, &modes)
        .map_err(|_| no_report())?;
    drop(raw);
    model.metadata = Some(cf_analyze::metadata::new_analysis_metadata(&ctx.path));

    let normalized = crate::formats::normalize_format(output_format);
    let render_format = crate::formats::apply_ndjson_modifier(&normalized, ctx.ndjson());

    // The model is not needed after rendering: render it in place.
    cf_analyze::conversion::render_converted_output(model, &render_format, None)
        .map_err(|_| no_report())
}

/// Renders a MIXED static+history selection. The formats whose content is
/// per-phase render each phase with its own renderer: `text` is the static
/// text sections followed by the history text, `compact` is every analyzer's
/// compact output (static first), and `timeseries` (with or without
/// `--ndjson`) is the history time series — static analyzers have no
/// per-commit data, so they are named once on stderr. Every other format is
/// the unified `codefang.run.v1` document ([`render_combined`]).
///
/// # Errors
///
/// Propagates the first phase renderer's [`HandlerError`].
pub fn render_mixed(
    ctx: &RunContext,
    static_ids: &[String],
    history_ids: &[String],
    output_format: &str,
) -> Result<Vec<u8>, HandlerError> {
    use crate::formats::{
        FORMAT_COMPACT, FORMAT_TEXT, FORMAT_TIMESERIES, FORMAT_TIMESERIES_NDJSON,
    };
    let normalized = crate::formats::normalize_format(output_format);
    let format = crate::formats::apply_ndjson_modifier(&normalized, ctx.ndjson());
    match format.as_str() {
        FORMAT_TEXT => {
            let patterns: Vec<&str> = static_ids.iter().map(String::as_str).collect();
            let filter = static_filter(ctx).map_err(HandlerError::InvalidFlag)?;
            let mut out = static_multi_text(&patterns, &ctx.path, &filter).ok_or_else(|| {
                HandlerError::NoReport {
                    analyzer: static_ids.join(","),
                    format: format.clone(),
                }
            })?;
            out.extend(history_formats::history_text(ctx, history_ids)?);
            Ok(out)
        }
        FORMAT_COMPACT => {
            let registry = default_registry();
            let mut out = Vec::new();
            for id in static_ids {
                let entry = registry.lookup(id).ok_or_else(|| HandlerError::NoReport {
                    analyzer: id.clone(),
                    format: format.clone(),
                })?;
                out.extend((entry.run)(ctx, FORMAT_COMPACT)?);
            }
            out.extend(history_formats::history_compact(ctx, history_ids)?);
            Ok(out)
        }
        FORMAT_TIMESERIES | FORMAT_TIMESERIES_NDJSON => {
            eprintln!(
                "warning: static analyzers have no per-commit time series; omitted: {}",
                static_ids.join(", ")
            );
            history_formats::history_timeseries(
                ctx,
                history_ids,
                format == FORMAT_TIMESERIES_NDJSON,
            )
        }
        _ => render_combined(ctx, static_ids, history_ids, output_format),
    }
}

/// `path.Match` semantics over an analyzer ID (`*`, `?`, `[...]`; `*` does not
/// cross `/`).
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
    let rest = if i < pat.len() {
        &pat[i + 1..]
    } else {
        &pat[i..]
    };
    (matched ^ negate, rest)
}

// ---------------------------------------------------------------------------
// Walk filters shared by both phases.
// ---------------------------------------------------------------------------

/// The vendor / generated path policy from the run flags
/// (`--include-vendored`, `--include-generated`, `--extra-excluded-prefixes`),
/// applied identically by both phases.
#[must_use]
pub fn path_policy_from_flags(sub: &clap::ArgMatches) -> cf_pathpolicy::Options {
    let extra: Vec<String> = sub
        .get_many::<String>("extra-excluded-prefixes")
        .map(|vals| vals.filter(|p| !p.is_empty()).cloned().collect())
        .unwrap_or_default();
    cf_pathpolicy::Options {
        include_vendored: sub.get_flag("include-vendored"),
        include_generated: sub.get_flag("include-generated"),
        extra_excluded_prefixes: extra,
    }
}

/// The static-phase path-policy options from the run flags.
pub(crate) fn static_path_policy(ctx: &RunContext) -> cf_pathpolicy::Options {
    path_policy_from_flags(ctx.matches)
}

/// Number of leading bytes scanned for a generated-file marker (`DO NOT
/// EDIT`, `Code generated`, `@generated`, …).
pub const GENERATED_MARKER_HEADER_BYTES: usize = 512;

/// The static-phase file filter shared by EVERY output format of every static
/// analyzer:
///
/// 1. the `--languages` restriction — fnmatch globs over the file BASENAME
///    (`None` ⇔ empty/`all` → no restriction);
/// 2. the `--include-vendored` / `--include-generated` /
///    `--extra-excluded-prefixes` policy over the path and, through
///    [`StaticFilter::skips_with_header`], over the file's first
///    [`GENERATED_MARKER_HEADER_BYTES`] bytes.
#[derive(Debug, Clone)]
pub struct StaticFilter {
    /// Vendor / generated exclusion policy.
    pub policy: cf_pathpolicy::Options,
    /// Basename globs from `--languages`; `None` disables the filter.
    pub language_globs: Option<Vec<String>>,
    /// Largest source file, in bytes, that is read and parsed (`--max-parse-file-size`).
    pub max_parse_bytes: u64,
}

impl Default for StaticFilter {
    /// The default policy, no language restriction, and the default parse size
    /// limit: a zero limit would skip every file.
    fn default() -> Self {
        Self::from_policy(cf_pathpolicy::Options::default())
    }
}

impl StaticFilter {
    /// Filter with the given path policy and no language restriction.
    #[must_use]
    pub fn from_policy(policy: cf_pathpolicy::Options) -> Self {
        StaticFilter {
            policy,
            language_globs: None,
            max_parse_bytes: DEFAULT_MAX_PARSE_FILE_BYTES,
        }
    }

    /// Reports whether the walk must skip `path` on path evidence alone (the
    /// language globs, then the path policy).
    #[must_use]
    pub fn skips(&self, path: &str) -> bool {
        if let Some(globs) = &self.language_globs {
            // The final path element.
            let base = path.rsplit('/').next().unwrap_or(path);
            if !globs.iter().any(|g| go_path_match(g, base)) {
                return true;
            }
        }
        cf_pathpolicy::exclude(path, None, &self.policy)
    }

    /// [`StaticFilter::skips`] plus the generated-content marker over the
    /// file's leading bytes (`header`, at most
    /// [`GENERATED_MARKER_HEADER_BYTES`] are consulted): a file whose header
    /// carries a generated-file marker is skipped unless
    /// `--include-generated`. This is the check every static walk must apply
    /// once it has read the file.
    #[must_use]
    pub fn skips_with_header(&self, path: &str, header: &[u8]) -> bool {
        if self.skips(path) {
            return true;
        }
        let head = &header[..header.len().min(GENERATED_MARKER_HEADER_BYTES)];
        cf_pathpolicy::exclude(path, Some(head), &self.policy)
    }
}

/// Builds the [`StaticFilter`] from the run flags: the path policy plus the
/// `--languages` globs.
///
/// # Errors
///
/// Returns `static --languages: unknown language: "<token>"` when a
/// `--languages` token resolves to no Linguist language.
pub(crate) fn static_filter(ctx: &RunContext) -> Result<StaticFilter, String> {
    let policy = static_path_policy(ctx);
    let tokens = language_tokens(ctx.matches);
    let globs = cf_langpath::globs(&tokens).map_err(|e| format!("static --languages: {e}"))?;
    let max_mib = ctx
        .matches
        .try_get_one::<i64>("max-parse-file-size")
        .ok()
        .flatten()
        .copied()
        .unwrap_or(DEFAULT_MAX_PARSE_FILE_MIB);
    if max_mib < 1 {
        return Err("static --max-parse-file-size: must be at least 1 MiB".to_string());
    }
    Ok(StaticFilter {
        policy,
        language_globs: if globs.wants_all {
            None
        } else {
            Some(globs.globs)
        },
        max_parse_bytes: u64::try_from(max_mib).unwrap_or(u64::MAX) * MIB_BYTES,
    })
}

/// The history-phase change filter shared by EVERY history analyzer:
///
/// 1. the vendor / generated / extra-prefix policy from the run flags, over
///    the change's path and over the blob's leading
///    [`GENERATED_MARKER_HEADER_BYTES`] bytes (the generated-file marker);
/// 2. the `--languages` restriction — the full enry cascade over the basename
///    AND the blob content (so `.h` resolves to C/C++/Objective-C by content
///    and an extensionless script by its shebang), applied only when the set
///    restricts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HistoryFilter {
    /// Vendor / generated exclusion policy from the run flags.
    pub policy: cf_pathpolicy::Options,
    /// Lowercased canonical Linguist names from `--languages`; `None` disables
    /// the filter (flag absent, empty, or the `all` token).
    pub languages: Option<std::collections::BTreeSet<String>>,
}

impl HistoryFilter {
    /// Filter with the given path policy and no language restriction.
    #[must_use]
    pub fn from_policy(policy: cf_pathpolicy::Options) -> Self {
        HistoryFilter {
            policy,
            languages: None,
        }
    }

    /// Reports whether one tree-diff change survives the filter. `name`/`hash`
    /// are the change's path and blob (Delete → `from`, otherwise `to`).
    #[must_use]
    pub fn includes_change(
        &self,
        repo: &cf_gitlib::Repository,
        name: &str,
        hash: cf_gitlib::hash::Hash,
    ) -> bool {
        // 1. path policy on the path alone.
        if cf_pathpolicy::exclude(name, None, &self.policy) {
            return false;
        }
        // The blob is read only when a content check is needed.
        let needs_content = !self.policy.include_generated || self.languages.is_some();
        if !needs_content || hash.is_zero() {
            return true;
        }
        let Ok(blob) = repo.lookup_blob(hash) else {
            return self.languages.is_none();
        };
        let content = blob.content();
        if !self.policy.include_generated {
            let head = &content[..content.len().min(GENERATED_MARKER_HEADER_BYTES)];
            if cf_pathpolicy::exclude(name, Some(head), &self.policy) {
                return false;
            }
        }
        // 2. the `--languages` set, by basename + content.
        let Some(languages) = &self.languages else {
            return true;
        };
        let base = name.rsplit('/').next().unwrap_or(name);
        cf_langpath::language_by_path_with_content(base, content)
            .is_some_and(|l| languages.contains(l.to_lowercase().as_str()))
    }

    /// Retains only the changes the filter admits, deriving the path/blob per
    /// change (Delete → `from`, otherwise `to`).
    pub fn retain_changes(
        &self,
        repo: &cf_gitlib::Repository,
        changes: &mut Vec<cf_gitlib::changes::Change>,
    ) {
        changes.retain(|c| {
            let (name, hash) = if matches!(c.action, cf_gitlib::changes::ChangeAction::Delete) {
                (&c.from.name, c.from.hash)
            } else {
                (&c.to.name, c.to.hash)
            };
            self.includes_change(repo, name, hash)
        });
    }
}

/// Reads the `--languages` tokens — the ONE parser both phases share: an
/// absent flag, an explicit empty value (`--languages ""`) and the `all`
/// token all mean "no restriction"; every other token is validated against
/// the Linguist tables by the caller.
#[must_use]
pub fn language_tokens(sub: &clap::ArgMatches) -> Vec<String> {
    let mut tokens: Vec<String> = sub
        .get_many::<String>("languages")
        .map(|vals| vals.cloned().collect())
        .unwrap_or_default();
    tokens.retain(|t| !t.trim().is_empty());
    tokens
}

/// The canonical lowercase language set for the validated `tokens`, or `None`
/// when the tokens do not restrict (empty / `all`).
fn language_token_set(tokens: &[String]) -> Option<std::collections::BTreeSet<String>> {
    if tokens.is_empty() {
        return None;
    }
    let mut set = std::collections::BTreeSet::new();
    for raw in tokens {
        let token = raw.trim();
        if token.eq_ignore_ascii_case("all") {
            // `all` disables the filter even alongside other tokens.
            return None;
        }
        // globs() validated every token before this is called.
        let canonical = cf_langpath::canonical_language(token).expect("tokens pre-validated");
        set.insert(canonical.to_lowercase());
    }
    Some(set)
}

/// Builds the [`HistoryFilter`] from the run flags: the shared path policy
/// plus the `--languages` canonical-name set.
///
/// # Errors
///
/// Returns `history --languages: unknown language: "<token>"` when a
/// `--languages` token resolves to no Linguist language.
pub(crate) fn history_filter(sub: &clap::ArgMatches) -> Result<HistoryFilter, String> {
    let policy = path_policy_from_flags(sub);
    let tokens = language_tokens(sub);
    let globs = cf_langpath::globs(&tokens).map_err(|e| format!("history --languages: {e}"))?;
    Ok(HistoryFilter {
        policy,
        languages: if globs.wants_all {
            None
        } else {
            language_token_set(&tokens)
        },
    })
}

// ---------------------------------------------------------------------------
// Per-analyzer registry handlers. Each owns its own `match format`; this is the
// ONE place that knows how to format a given analyzer. One handler per
// analyzer id — NOT one per (id,format).
// ---------------------------------------------------------------------------

/// The shared static dispatch shape: build the walk filter, run the
/// per-format report builder, map a missing report to [`HandlerError`].
fn static_dispatch(
    ctx: &RunContext,
    id: &str,
    format: &str,
    build: impl FnOnce(&str, &StaticFilter) -> Option<Option<Vec<u8>>>,
) -> Result<Vec<u8>, HandlerError> {
    let filter = static_filter(ctx).map_err(HandlerError::InvalidFlag)?;
    match build(&ctx.path, &filter) {
        Some(bytes) => report_or_err(bytes, id, format),
        None => Err(format_not_supported(format, id)),
    }
}

fn h_static_clones(ctx: &RunContext, format: &str) -> Result<Vec<u8>, HandlerError> {
    const ID: &str = "static/clones";
    let per_file = ctx.matches.get_flag("per-file");
    static_dispatch(ctx, ID, format, |path, filter| match format {
        "json" => Some(static_clones::clones_report_json_flags(
            path, filter, per_file,
        )),
        "yaml" => Some(static_clones::clones_report_yaml(path, filter)),
        "binary" | "bin" => Some(static_clones::clones_report_bin(path, filter)),
        "compact" => Some(static_clones::clones_report_compact(path, filter)),
        "text" => Some(
            static_clones::clones_report_value_flags(path, filter, false)
                .map(|v| section_render::render_text_report(&v)),
        ),
        _ => None,
    })
}

fn h_static_complexity(ctx: &RunContext, format: &str) -> Result<Vec<u8>, HandlerError> {
    const ID: &str = "static/complexity";
    let per_file = ctx.matches.get_flag("per-file");
    static_dispatch(ctx, ID, format, |path, filter| match format {
        "json" => Some(static_complexity::complexity_report_flags(
            path, filter, per_file,
        )),
        "yaml" => Some(static_complexity_yaml::complexity_report_yaml(path, filter)),
        "binary" | "bin" => Some(static_complexity_bin::complexity_report_bin(path, filter)),
        "compact" => Some(
            static_complexity::complexity_report_value_summary(path, filter)
                .map(|v| section_render::render_compact_report(&v)),
        ),
        "text" => Some(
            static_complexity::complexity_report_value_summary(path, filter)
                .map(|v| section_render::render_text_report(&v)),
        ),
        _ => None,
    })
}

fn h_static_cohesion(ctx: &RunContext, format: &str) -> Result<Vec<u8>, HandlerError> {
    const ID: &str = "static/cohesion";
    let per_file = ctx.matches.get_flag("per-file");
    static_dispatch(ctx, ID, format, |path, filter| match format {
        "json" => Some(static_cohesion::cohesion_report_json_flags(
            path, filter, per_file,
        )),
        "yaml" => Some(static_cohesion::cohesion_report_yaml(path, filter)),
        "binary" | "bin" => Some(static_cohesion::cohesion_report_bin(path, filter)),
        "compact" => Some(
            static_cohesion::cohesion_report_value_summary(path, filter)
                .map(|v| section_render::render_compact_report(&v)),
        ),
        "text" => Some(
            static_cohesion::cohesion_report_value_summary(path, filter)
                .map(|v| section_render::render_text_report(&v)),
        ),
        _ => None,
    })
}

fn h_static_composition(ctx: &RunContext, format: &str) -> Result<Vec<u8>, HandlerError> {
    const ID: &str = "static/composition";
    let per_file = ctx.matches.get_flag("per-file");
    static_dispatch(ctx, ID, format, |path, filter| match format {
        "json" => Some(static_json::composition_report_opts_flags(
            path, filter, per_file,
        )),
        "yaml" => Some(static_json::composition_yaml_opts(path, filter)),
        "binary" | "bin" => Some(static_json::composition_bin_opts(path, filter)),
        "compact" => Some(
            static_json::composition_report_value_opts(path, filter)
                .map(|v| section_render::render_compact_report(&v)),
        ),
        "text" => Some(
            static_json::composition_report_value_opts(path, filter)
                .map(|v| section_render::render_text_report(&v)),
        ),
        _ => None,
    })
}

fn h_static_halstead(ctx: &RunContext, format: &str) -> Result<Vec<u8>, HandlerError> {
    const ID: &str = "static/halstead";
    let per_file = ctx.matches.get_flag("per-file");
    static_dispatch(ctx, ID, format, |path, filter| match format {
        "json" => Some(static_halstead::halstead_json_report_flags(
            path, filter, per_file,
        )),
        "yaml" => Some(static_halstead::halstead_yaml_report(path, filter)),
        "binary" | "bin" => Some(static_halstead::halstead_bin_report(path, filter)),
        "compact" => Some(
            static_halstead::halstead_report_value_summary(path, filter)
                .map(|v| section_render::render_compact_report(&v)),
        ),
        "text" => Some(
            static_halstead::halstead_report_value_summary(path, filter)
                .map(|v| section_render::render_text_report(&v)),
        ),
        _ => None,
    })
}

fn h_static_imports(ctx: &RunContext, format: &str) -> Result<Vec<u8>, HandlerError> {
    const ID: &str = "static/imports";
    let per_file = ctx.matches.get_flag("per-file");
    static_dispatch(ctx, ID, format, |path, filter| match format {
        "json" => Some(static_imports::imports_report_json_flags(
            path, filter, per_file,
        )),
        "yaml" => Some(static_imports::imports_report_yaml(path, filter)),
        "binary" | "bin" => Some(static_imports::imports_report_bin(path, filter)),
        "compact" => Some(
            static_imports::imports_report_value_flags(path, filter, false)
                .map(|v| section_render::render_compact_report(&v)),
        ),
        "text" => Some(
            static_imports::imports_report_value_flags(path, filter, false)
                .map(|v| section_render::render_text_report(&v)),
        ),
        _ => None,
    })
}

fn h_static_comments(ctx: &RunContext, format: &str) -> Result<Vec<u8>, HandlerError> {
    const ID: &str = "static/comments";
    let per_file = ctx.matches.get_flag("per-file");
    static_dispatch(ctx, ID, format, |path, filter| match format {
        "json" => Some(static_comments::comments_report_json_flags(
            path, filter, per_file,
        )),
        "yaml" => Some(static_comments::comments_report_yaml(path, filter)),
        "binary" | "bin" => Some(static_comments::comments_report_bin(path, filter)),
        "compact" => Some(
            static_comments::comments_report_value_summary(path, filter)
                .map(|v| section_render::render_compact_report(&v)),
        ),
        "text" => Some(
            static_comments::comments_report_value_summary(path, filter)
                .map(|v| section_render::render_text_report(&v)),
        ),
        _ => None,
    })
}

fn h_history_imports(ctx: &RunContext, format: &str) -> Result<Vec<u8>, HandlerError> {
    const ID: &str = "history/imports";
    let metrics =
        history::imports_run_metrics(ctx.matches).ok_or_else(|| HandlerError::NoReport {
            analyzer: ID.to_string(),
            format: format.to_string(),
        })?;
    serialize_history_metrics(
        format,
        ID,
        "ImportsPerDeveloper",
        &metrics.to_go_value(),
        &metrics.to_go_value_yaml(),
    )
}

fn h_history_typos(ctx: &RunContext, format: &str) -> Result<Vec<u8>, HandlerError> {
    const ID: &str = "history/typos";
    let bytes = match format {
        "json" => history::typos_run_report(ctx.matches),
        "yaml" => history::typos_run_report_yaml(ctx.matches),
        "binary" | "bin" => history::typos_run_report_bin(ctx.matches),
        _ => return Err(format_not_supported(format, ID)),
    };
    report_or_err(bytes, ID, format)
}

fn h_history_couples(ctx: &RunContext, format: &str) -> Result<Vec<u8>, HandlerError> {
    const ID: &str = "history/couples";
    let value =
        couples_run::couples_run_value(ctx.matches).ok_or_else(|| HandlerError::NoReport {
            analyzer: ID.to_string(),
            format: format.to_string(),
        })?;
    serialize_history_metrics(format, ID, "Couples", &value, &value)
}

fn h_history_shotness(ctx: &RunContext, format: &str) -> Result<Vec<u8>, HandlerError> {
    const ID: &str = "history/shotness";
    let metrics =
        shotness_run::shotness_run_metrics(ctx.matches).ok_or_else(|| HandlerError::NoReport {
            analyzer: ID.to_string(),
            format: format.to_string(),
        })?;
    let value = metrics.to_go_value();
    serialize_history_metrics(format, ID, "Shotness", &value, &value)
}

fn h_history_devs(ctx: &RunContext, format: &str) -> Result<Vec<u8>, HandlerError> {
    const ID: &str = "history/devs";
    let sub = ctx.matches;
    let bytes = match format {
        "json" => history::devs_run_report(sub),
        "yaml" => history::devs_run_report_yaml(sub),
        "binary" | "bin" => history::devs_run_report_bin(sub),
        _ => return Err(format_not_supported(format, ID)),
    };
    report_or_err(bytes, ID, format)
}

fn h_history_anomaly(ctx: &RunContext, format: &str) -> Result<Vec<u8>, HandlerError> {
    use cf_anomaly::model::ToGoValue;
    const ID: &str = "history/anomaly";
    let metrics =
        history::anomaly_run_metrics(ctx.matches).ok_or_else(|| HandlerError::NoReport {
            analyzer: ID.to_string(),
            format: format.to_string(),
        })?;
    let value = metrics.to_go_value();
    serialize_history_metrics(format, ID, "TemporalAnomaly", &value, &value)
}

fn h_history_quality(ctx: &RunContext, format: &str) -> Result<Vec<u8>, HandlerError> {
    const ID: &str = "history/quality";
    let metrics = history::quality_metrics(ctx.matches).ok_or_else(|| HandlerError::NoReport {
        analyzer: ID.to_string(),
        format: format.to_string(),
    })?;
    let value = cf_quality::serialize::computed_metrics_value(&metrics);
    serialize_history_metrics(format, ID, ID, &value, &value)
}

fn h_history_sentiment(ctx: &RunContext, format: &str) -> Result<Vec<u8>, HandlerError> {
    use cf_sentiment::ToGoValue;
    const ID: &str = "history/sentiment";
    let metrics =
        history::sentiment_metrics(ctx.matches).ok_or_else(|| HandlerError::NoReport {
            analyzer: ID.to_string(),
            format: format.to_string(),
        })?;
    serialize_history_metrics(
        format,
        ID,
        ID,
        &metrics.to_go_value(),
        &metrics.to_go_value_yaml(),
    )
}

fn h_history_file_history(ctx: &RunContext, format: &str) -> Result<Vec<u8>, HandlerError> {
    const ID: &str = "history/file-history";
    let value =
        history::file_history_report_value(ctx.matches).ok_or_else(|| HandlerError::NoReport {
            analyzer: ID.to_string(),
            format: format.to_string(),
        })?;
    serialize_history_metrics(format, ID, "FileHistoryAnalysis", &value, &value)
}

fn h_history_burndown(ctx: &RunContext, format: &str) -> Result<Vec<u8>, HandlerError> {
    const ID: &str = "history/burndown";
    let sub = ctx.matches;
    let bytes = match format {
        "json" => burndown_ndjson::burndown_run_report(sub),
        "yaml" => burndown_ndjson::burndown_run_report_yaml(sub),
        "binary" | "bin" => burndown_ndjson::burndown_run_report_bin(sub),
        _ => return Err(format_not_supported(format, ID)),
    };
    report_or_err(bytes, ID, format)
}

/// Builds the single default analyzer [`Registry`]. One registry insertion
/// per analyzer; dispatch is a keyed lookup by id, not a per-format match
/// ladder.
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

    r.register(s("static/clones", h_static_clones));
    r.register(s("static/complexity", h_static_complexity));
    r.register(s("static/cohesion", h_static_cohesion));
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

#[cfg(test)]
mod static_filter_tests {
    use super::*;

    /// Fully permissive path policy, isolating the language-glob leg of the
    /// filter from the vendored/generated policy leg.
    fn permissive_policy() -> cf_pathpolicy::Options {
        cf_pathpolicy::Options {
            include_vendored: true,
            include_generated: true,
            ..cf_pathpolicy::Options::default()
        }
    }

    /// Parses `run` args through the REAL command tree and builds the shared
    /// walk filter exactly as the dispatchers do (`static_filter(ctx)`).
    fn filter_for(args: &[&str]) -> Result<StaticFilter, String> {
        let argv: Vec<&str> = ["codefang", "run"]
            .iter()
            .chain(args.iter())
            .copied()
            .chain(std::iter::once("."))
            .collect();
        let matches = crate::build_codefang_command()
            .try_get_matches_from(argv)
            .expect("run args parse");
        let sub = matches
            .subcommand_matches("run")
            .expect("run subcommand matches");
        let ctx = RunContext::from_matches(sub);
        static_filter(&ctx)
    }

    #[test]
    fn skips_matches_language_globs_on_basename() {
        // A `*.go` glob must match a NESTED path's basename.
        let filter = StaticFilter {
            policy: permissive_policy(),
            language_globs: Some(vec!["*.go".to_string()]),
            max_parse_bytes: DEFAULT_MAX_PARSE_FILE_BYTES,
        };
        assert!(!filter.skips("src/pkg/deep/file.go"));
        assert!(filter.skips("src/pkg/deep/file.py"));
    }

    #[test]
    fn skips_language_filter_applies_before_path_policy() {
        let filter = StaticFilter {
            policy: permissive_policy(),
            language_globs: Some(vec!["*.py".to_string()]),
            max_parse_bytes: DEFAULT_MAX_PARSE_FILE_BYTES,
        };
        assert!(filter.skips("vendor/dep.go"));
        // A language-matched path still falls through to the path policy:
        // vendor is excluded under the DEFAULT policy despite the glob match.
        let filter = StaticFilter {
            policy: cf_pathpolicy::Options::default(),
            language_globs: Some(vec!["*.go".to_string()]),
            max_parse_bytes: DEFAULT_MAX_PARSE_FILE_BYTES,
        };
        assert!(filter.skips("vendor/dep.go"));
    }

    #[test]
    fn skips_none_globs_means_no_language_restriction() {
        let filter = StaticFilter {
            policy: permissive_policy(),
            language_globs: None,
            max_parse_bytes: DEFAULT_MAX_PARSE_FILE_BYTES,
        };
        assert!(!filter.skips("a.py"));
        assert!(!filter.skips("vendor/dep.go"));
    }

    #[test]
    fn skips_empty_globs_matches_nothing() {
        // An empty glob VECTOR (as opposed to `None`) restricts to the empty
        // language set: every path is skipped.
        let filter = StaticFilter {
            policy: permissive_policy(),
            language_globs: Some(Vec::new()),
            max_parse_bytes: DEFAULT_MAX_PARSE_FILE_BYTES,
        };
        assert!(filter.skips("a.go"));
        assert!(filter.skips("a.py"));
    }

    /// The generated-content marker in the file header excludes the file
    /// under the default policy and is ignored with `--include-generated`.
    #[test]
    fn skips_with_header_applies_the_generated_marker() {
        let marked = b"// Code generated by protoc-gen-go. DO NOT EDIT.\npackage main\n";
        let plain = b"package main\n";
        let default = StaticFilter::default();
        assert!(default.skips_with_header("a.go", marked));
        assert!(!default.skips_with_header("a.go", plain));
        let permissive = StaticFilter::from_policy(permissive_policy());
        assert!(!permissive.skips_with_header("a.go", marked));
    }

    #[test]
    fn static_filter_parses_languages_into_basename_globs() {
        let filter = filter_for(&["--languages", "Go"]).expect("Go resolves");
        let globs = filter
            .language_globs
            .expect("--languages Go must restrict the walk");
        assert!(
            globs.iter().any(|g| g == "*.go"),
            "Go must contribute the *.go basename glob, got {globs:?}"
        );
    }

    #[test]
    fn static_filter_all_token_disables_the_restriction() {
        let filter = filter_for(&["--languages", "all"]).expect("all resolves");
        assert!(filter.language_globs.is_none());
        let filter = filter_for(&[]).expect("absent flag resolves");
        assert!(filter.language_globs.is_none());
    }

    #[test]
    fn static_filter_unknown_language_token_errors() {
        let err = filter_for(&["--languages", "NotALang"])
            .expect_err("unresolvable token must abort the run");
        assert!(
            err.contains("unknown language"),
            "error must carry the CLI contract wording, got {err:?}"
        );
    }

    /// `--extra-excluded-prefixes` reaches the path policy of both phases.
    #[test]
    fn extra_excluded_prefixes_reach_the_policy() {
        let filter = filter_for(&["--extra-excluded-prefixes", "gen/,build/"]).expect("parses");
        assert_eq!(
            filter.policy.extra_excluded_prefixes,
            vec!["gen/".to_string(), "build/".to_string()]
        );
        assert!(filter.skips("gen/a.go"));
        assert!(!filter.skips("src/a.go"));
    }
}

#[cfg(test)]
mod per_file_multi_tests {
    use super::*;

    /// Fixture: one Go file with functions/comments/imports plus a plain-text
    /// file, exercising every static analyzer through the merged JSON path.
    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("a.go"),
            "package main\n\nimport \"fmt\"\n\n// main prints.\nfunc main() {\n\tfmt.Println(\"hi\")\n}\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("notes.txt"), "just text\n").unwrap();
        dir
    }

    #[test]
    fn per_file_flag_enriches_every_merged_section() {
        let dir = fixture();
        let bytes = static_multi_json(
            &["static/*"],
            dir.path().to_str().unwrap(),
            &StaticFilter::default(),
            true,
        )
        .unwrap();
        let json = String::from_utf8(bytes).unwrap();
        let sections = json.matches("\"title\"").count();
        let files_keys = json.matches("\"files\"").count();
        assert_eq!(
            sections, files_keys,
            "every section must carry a files key (sections={sections} files={files_keys}):\n{json}"
        );
        assert!(
            json.contains("\"file_path\": \"a.go\""),
            "per-file entries missing:\n{json}"
        );
    }

    #[test]
    fn no_per_file_flag_omits_files_in_merge() {
        let dir = fixture();
        let bytes = static_multi_json(
            &["static/*"],
            dir.path().to_str().unwrap(),
            &StaticFilter::default(),
            false,
        )
        .unwrap();
        let json = String::from_utf8(bytes).unwrap();
        assert!(
            !json.contains("\"files\""),
            "files key must be omitted:\n{json}"
        );
    }

    /// The merged `overall_score` reads a report's top-level score.
    fn overall_score(report: &GoValue) -> f64 {
        match report.as_map().and_then(|m| m.get("overall_score")) {
            Some(GoValue::Float(f)) => *f,
            other => panic!("overall_score missing: {other:?}"),
        }
    }

    /// Info-only sections (imports, composition: score `-1`) are left out of
    /// the executive-summary average instead of dragging it toward `-1`; an
    /// all-info selection is `-1` / `Info`.
    #[test]
    fn info_only_sections_do_not_drag_the_overall_score() {
        let dir = fixture();
        let path = dir.path().to_str().unwrap();
        let filter = StaticFilter::default();
        let scored = static_multi_report_value(&["static/complexity"], path, &filter, false)
            .expect("complexity report");
        let mixed = static_multi_report_value(
            &["static/complexity", "static/imports", "static/composition"],
            path,
            &filter,
            false,
        )
        .expect("mixed report");
        assert!(
            overall_score(&scored) >= 0.0,
            "complexity is a scored section"
        );
        assert!(
            (overall_score(&mixed) - overall_score(&scored)).abs() < f64::EPSILON,
            "info-only sections changed the average: {} vs {}",
            overall_score(&mixed),
            overall_score(&scored)
        );
        let info_only = static_multi_report_value(
            &["static/imports", "static/composition"],
            path,
            &filter,
            false,
        )
        .expect("info report");
        assert!((overall_score(&info_only) - (-1.0)).abs() < f64::EPSILON);
        assert_eq!(
            info_only
                .as_map()
                .and_then(|m| m.get("overall_score_label"))
                .and_then(GoValue::as_str),
            Some("Info")
        );
    }
}

#[cfg(test)]
mod per_file_note_tests {
    use super::per_file_ignored_note;

    #[test]
    fn note_names_every_non_json_format_and_is_silent_for_json() {
        for format in ["yaml", "bin", "binary", "text", "compact", "plot"] {
            assert_eq!(
                per_file_ignored_note(true, true, format),
                Some(format!(
                    "--per-file applies to --format json only; ignored for {format}"
                ))
            );
        }
        assert_eq!(per_file_ignored_note(true, true, "json"), None);
        assert_eq!(per_file_ignored_note(true, true, " JSON "), None);
    }

    #[test]
    fn note_needs_the_flag_and_a_static_selection() {
        assert_eq!(per_file_ignored_note(true, false, "yaml"), None);
        assert_eq!(per_file_ignored_note(false, true, "yaml"), None);
    }
}

#[cfg(test)]
mod static_walk_tests {
    //! The ONE static walk: file selection, reading, stamping and the skip
    //! registry, exercised on real tempdir trees.

    use super::*;

    const PLAIN_GO: &str = "package main\n\nfunc f() {}\n";
    const MARKED_GO: &str =
        "// Code generated by protoc-gen-go. DO NOT EDIT.\npackage main\n\nfunc g() {}\n";

    fn write(dir: &Path, rel: &str, content: &[u8]) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, content).expect("write fixture");
    }

    fn permissive() -> StaticFilter {
        StaticFilter::from_policy(cf_pathpolicy::Options {
            include_vendored: true,
            include_generated: true,
            ..cf_pathpolicy::Options::default()
        })
    }

    /// The admitted root-relative paths of the source walk, in walk order.
    fn source_paths(root: &Path, filter: &StaticFilter) -> Vec<String> {
        let parser = Parser::new();
        walk_source_files(root.to_str().unwrap(), &parser, filter)
            .expect("root exists")
            .map(|f| f.rel_path)
            .collect()
    }

    /// The admitted root-relative paths of the all-files walk, in walk order.
    fn all_paths(root: &Path, filter: &StaticFilter) -> Vec<String> {
        walk_all_files(root.to_str().unwrap(), filter)
            .expect("root exists")
            .map(|f| f.rel_path)
            .collect()
    }

    #[test]
    fn file_root_yields_the_file_stamped_with_its_name() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "pkg/a.go", PLAIN_GO.as_bytes());
        let file = dir.path().join("pkg/a.go");
        let parser = Parser::new();
        let files: Vec<SourceFile> =
            walk_source_files(file.to_str().unwrap(), &parser, &permissive())
                .expect("file root")
                .collect();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].rel_path, "a.go");
        assert_eq!(files[0].path, file);
        assert_eq!(files[0].bytes, PLAIN_GO.as_bytes());
        assert_eq!(files[0].language, "go");
        assert_eq!(files[0].directory(), ".");
    }

    #[test]
    fn nested_directories_walk_lexically_and_skip_git_and_cache_dirs() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "b.go", PLAIN_GO.as_bytes());
        write(dir.path(), "a/z.go", PLAIN_GO.as_bytes());
        write(dir.path(), "a/y.go", PLAIN_GO.as_bytes());
        write(dir.path(), ".git/x.go", PLAIN_GO.as_bytes());
        write(
            dir.path(),
            "target/CACHEDIR.TAG",
            b"Signature: 8a477f597d28d172789f06886806bc55\n",
        );
        write(dir.path(), "target/t.go", PLAIN_GO.as_bytes());
        write(dir.path(), "notes.txt", b"not source\n");
        assert_eq!(
            source_paths(dir.path(), &permissive()),
            vec!["a/y.go", "a/z.go", "b.go"]
        );
        let parser = Parser::new();
        let nested = walk_source_files(dir.path().to_str().unwrap(), &parser, &permissive())
            .unwrap()
            .next()
            .unwrap();
        assert_eq!(nested.directory(), "a");
        // The all-files walk skips the same directories.
        assert_eq!(
            all_paths(dir.path(), &StaticFilter::default()),
            vec!["a/y.go", "a/z.go", "b.go", "notes.txt"]
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_loop_terminates_and_symlinked_dirs_are_never_files() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "sub/c.go", PLAIN_GO.as_bytes());
        // sub/loop -> .. (the root): following it would recurse forever.
        std::os::unix::fs::symlink("..", dir.path().join("sub/loop")).unwrap();
        // A symlink to a regular file is read through.
        std::os::unix::fs::symlink("sub/c.go", dir.path().join("link.go")).unwrap();
        // A dangling symlink contributes nothing.
        std::os::unix::fs::symlink("nowhere.go", dir.path().join("dangling.go")).unwrap();
        assert_eq!(
            source_paths(dir.path(), &permissive()),
            vec!["link.go", "sub/c.go"]
        );
        assert_eq!(
            all_paths(dir.path(), &StaticFilter::default()),
            vec!["link.go", "sub/c.go"],
            "a symlinked directory must not be counted as a file"
        );
    }

    #[test]
    fn oversized_file_is_skipped_and_recorded_once() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a.go", PLAIN_GO.as_bytes());
        let big = dir.path().join("big.go");
        // A sparse file: the size is what the cap reads, no bytes are written.
        std::fs::File::create(&big)
            .unwrap()
            .set_len(DEFAULT_MAX_PARSE_FILE_BYTES + 1)
            .unwrap();
        assert!(!was_skipped(&big));
        assert_eq!(source_paths(dir.path(), &permissive()), vec!["a.go"]);
        assert!(was_skipped(&big), "the oversized file must be recorded");
        // A second walk skips it again without re-recording (or re-printing)
        // it: the registry already holds the path.
        assert_eq!(source_paths(dir.path(), &permissive()), vec!["a.go"]);
        assert!(
            !note_skipped(&big, SkipReason::Oversized),
            "a repeat skip must not be recorded (or warned about) twice"
        );
        // The all-files walk still classifies it (header only).
        let all = walk_all_files(dir.path().to_str().unwrap(), &StaticFilter::default())
            .unwrap()
            .find(|f| f.rel_path == "big.go")
            .expect("composition sees the oversized file");
        assert_eq!(all.bytes.len(), ALL_FILES_HEADER_BYTES);
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_file_is_skipped_and_recorded() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a.go", PLAIN_GO.as_bytes());
        let secret = dir.path().join("secret.go");
        write(dir.path(), "secret.go", PLAIN_GO.as_bytes());
        std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read(&secret).is_ok() {
            // Running as root: nothing is unreadable.
            return;
        }
        assert_eq!(source_paths(dir.path(), &permissive()), vec!["a.go"]);
        assert!(was_skipped(&secret));
        // Composition still classifies it, on its path alone.
        let all = walk_all_files(dir.path().to_str().unwrap(), &StaticFilter::default())
            .unwrap()
            .find(|f| f.rel_path == "secret.go")
            .expect("composition sees the unreadable file");
        assert!(all.bytes.is_empty());
    }

    #[test]
    fn generated_marker_excludes_by_default_and_include_generated_admits() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a.go", PLAIN_GO.as_bytes());
        write(dir.path(), "gen.go", MARKED_GO.as_bytes());
        write(dir.path(), "gen.pb.go", PLAIN_GO.as_bytes());
        assert_eq!(
            source_paths(dir.path(), &StaticFilter::default()),
            vec!["a.go"]
        );
        assert_eq!(
            source_paths(dir.path(), &permissive()),
            vec!["a.go", "gen.go", "gen.pb.go"]
        );
    }

    #[test]
    fn vendored_paths_follow_the_policy() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a.go", PLAIN_GO.as_bytes());
        write(dir.path(), "vendor/dep.go", PLAIN_GO.as_bytes());
        write(dir.path(), "node_modules/m/i.js", b"export const x = 1;\n");
        assert_eq!(
            source_paths(dir.path(), &StaticFilter::default()),
            vec!["a.go"]
        );
        assert_eq!(
            source_paths(dir.path(), &permissive()),
            vec!["a.go", "node_modules/m/i.js", "vendor/dep.go"]
        );
    }

    /// `--extra-excluded-prefixes` matches the ROOT-RELATIVE path, so an
    /// absolute (tempdir) root does not defeat a `gen/` prefix.
    #[test]
    fn extra_excluded_prefixes_apply_to_the_root_relative_path() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a.go", PLAIN_GO.as_bytes());
        write(dir.path(), "gen/a.go", PLAIN_GO.as_bytes());
        assert!(dir.path().is_absolute());
        let filter = StaticFilter::from_policy(cf_pathpolicy::Options {
            extra_excluded_prefixes: vec!["gen/".to_string()],
            ..cf_pathpolicy::Options::default()
        });
        assert_eq!(source_paths(dir.path(), &filter), vec!["a.go"]);
        assert_eq!(all_paths(dir.path(), &filter), vec!["a.go"]);
    }

    #[test]
    fn languages_globs_restrict_by_basename() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a.go", PLAIN_GO.as_bytes());
        write(dir.path(), "deep/b.py", b"def f():\n    return 1\n");
        let py_only = StaticFilter {
            language_globs: Some(vec!["*.py".to_string()]),
            max_parse_bytes: DEFAULT_MAX_PARSE_FILE_BYTES,
            ..permissive()
        };
        assert_eq!(source_paths(dir.path(), &py_only), vec!["deep/b.py"]);
        assert_eq!(all_paths(dir.path(), &py_only), vec!["deep/b.py"]);
        assert_eq!(
            source_paths(dir.path(), &permissive()),
            vec!["a.go", "deep/b.py"]
        );
    }

    /// Composition's walk admits vendored, generated, binary and unsupported
    /// files with their header bytes; only the language globs narrow it.
    #[test]
    fn all_files_walk_sees_everything_with_header_bytes() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a.go", PLAIN_GO.as_bytes());
        write(dir.path(), "vendor/v.go", PLAIN_GO.as_bytes());
        write(dir.path(), "gen.go", MARKED_GO.as_bytes());
        write(dir.path(), "data.bin", b"\x00\x01\x02\xff");
        write(dir.path(), "notes.txt", b"plain text\n");
        let long = vec![b'x'; ALL_FILES_HEADER_BYTES + 100];
        write(dir.path(), "long.txt", &long);
        let files: Vec<SourceFile> =
            walk_all_files(dir.path().to_str().unwrap(), &StaticFilter::default())
                .unwrap()
                .collect();
        let paths: Vec<&str> = files.iter().map(|f| f.rel_path.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                "a.go",
                "data.bin",
                "gen.go",
                "long.txt",
                "notes.txt",
                "vendor/v.go"
            ]
        );
        let bin = files.iter().find(|f| f.rel_path == "data.bin").unwrap();
        assert_eq!(bin.bytes, b"\x00\x01\x02\xff");
        assert!(bin.language.is_empty());
        let capped = files.iter().find(|f| f.rel_path == "long.txt").unwrap();
        assert_eq!(capped.bytes.len(), ALL_FILES_HEADER_BYTES);
        // The source walk over the same tree admits only the plain Go file.
        assert_eq!(
            source_paths(dir.path(), &StaticFilter::default()),
            vec!["a.go"]
        );
    }

    #[test]
    fn missing_root_is_path_not_found() {
        let parser = Parser::new();
        let missing = "/nonexistent/codefang-static-walk";
        assert_eq!(
            walk_source_files(missing, &parser, &StaticFilter::default())
                .err()
                .map(|e| e.to_string()),
            Some(format!("path not found: {missing}"))
        );
        assert!(walk_all_files(missing, &StaticFilter::default()).is_err());
    }

    #[test]
    fn relative_paths_and_parent_dirs() {
        let rel = |p: &str, r: &str| make_relative_path(Path::new(p), Path::new(r));
        assert_eq!(rel("/r/a/b.go", "/r"), "a/b.go");
        assert_eq!(rel("/r/b.go", "/r/"), "b.go");
        assert_eq!(rel("./b.go", "."), "b.go");
        // A FILE root stamps the file's name, never `.`.
        assert_eq!(rel("/r/b.go", "/r/b.go"), "b.go");
        // A path outside the root stamps its name too.
        assert_eq!(rel("/x/b.go", "/r"), "b.go");
        assert_eq!(parent_dir("a/b.go"), "a");
        assert_eq!(parent_dir("b.go"), ".");
        assert_eq!(parent_dir("/b.go"), "/");
    }
}

#[cfg(test)]
mod dispatch_flag_tests {
    //! Walk-filter flags exercised through the REAL registry dispatch
    //! (`default_registry().lookup(id).run`), NOT by calling the report
    //! builders with a hand-made filter.

    use super::*;

    /// Fixture where the walk-filter flags change every total: one plain Go
    /// function, one generated-path Go function, one vendored Go function, one
    /// Python function.
    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("a.go"),
            "package main\n\nfunc plain(a int) int {\n\tif a > 0 {\n\t\treturn a\n\t}\n\treturn -a\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("gen.pb.go"),
            "package main\n\nfunc gen(a int) int {\n\tif a > 0 {\n\t\treturn a\n\t}\n\treturn -a\n}\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("vendor/dep")).unwrap();
        std::fs::write(
            dir.path().join("vendor/dep/d.go"),
            "package dep\n\nfunc vendored(a int) int {\n\tif a > 0 {\n\t\treturn a\n\t}\n\treturn -a\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("b.py"),
            "def py_fn(a):\n    if a > 0:\n        return a\n    return -a\n",
        )
        .unwrap();
        dir
    }

    /// Runs `codefang run --analyzers <id> --format <format> <extra..> <root>`
    /// through the real argv parse + registry handler, returning the rendered
    /// bytes — the exact dispatch path the CLI takes.
    fn dispatch(id: &str, format: &str, extra: &[&str], root: &std::path::Path) -> Vec<u8> {
        let root = root.to_str().unwrap();
        let mut argv = vec!["codefang", "run", "--analyzers", id, "--format", format];
        argv.extend_from_slice(extra);
        argv.push(root);
        let matches = crate::build_codefang_command()
            .try_get_matches_from(argv)
            .expect("run args parse");
        let sub = matches.subcommand_matches("run").expect("run subcommand");
        let ctx = RunContext::from_matches(sub);
        let registry = default_registry();
        let entry = registry.lookup(id).expect("analyzer registered");
        (entry.run)(&ctx, format).expect("handler produced a report")
    }

    /// Extracts the numeric "Total Functions" figure from a rendered text
    /// report.
    fn text_total_functions(bytes: &[u8]) -> i64 {
        let text = String::from_utf8_lossy(bytes);
        let line = text
            .lines()
            .find(|l| l.contains("Total Functions"))
            .unwrap_or_else(|| panic!("no Total Functions line in:\n{text}"));
        let rest = line.split("Total Functions").nth(1).unwrap();
        rest.split_whitespace()
            .next()
            .and_then(|w| w.parse().ok())
            .unwrap_or_else(|| panic!("unparseable Total Functions line: {line}"))
    }

    #[test]
    fn text_dispatch_applies_include_flags() {
        let dir = fixture();
        for id in ["static/complexity", "static/halstead"] {
            let default_total = text_total_functions(&dispatch(id, "text", &[], dir.path()));
            let permissive_total = text_total_functions(&dispatch(
                id,
                "text",
                &["--include-generated", "--include-vendored"],
                dir.path(),
            ));
            assert!(
                permissive_total > default_total,
                "{id}: text dispatch dropped the include flags \
                 (default={default_total}, permissive={permissive_total})"
            );
        }
    }

    #[test]
    fn text_dispatch_applies_language_filter() {
        let dir = fixture();
        let py_total = text_total_functions(&dispatch(
            "static/complexity",
            "text",
            &[
                "--include-generated",
                "--include-vendored",
                "--languages",
                "Python",
            ],
            dir.path(),
        ));
        assert_eq!(
            py_total, 1,
            "text dispatch must restrict the walk to the one Python function"
        );
    }

    #[test]
    fn compact_dispatch_applies_language_filter() {
        let dir = fixture();
        let unfiltered = dispatch("static/complexity", "compact", &[], dir.path());
        let ruby_only = dispatch(
            "static/complexity",
            "compact",
            &["--languages", "Ruby"],
            dir.path(),
        );
        assert_ne!(
            unfiltered, ruby_only,
            "compact dispatch dropped --languages (both renders identical)"
        );
    }

    #[test]
    fn empty_languages_value_means_no_filter() {
        let dir = fixture();
        let empty = text_total_functions(&dispatch(
            "static/complexity",
            "text",
            &["--languages", ""],
            dir.path(),
        ));
        let absent = text_total_functions(&dispatch("static/complexity", "text", &[], dir.path()));
        assert_eq!(empty, absent, "--languages \"\" must behave as no filter");
    }

    /// An unsupported format for a static analyzer names the format and the
    /// analyzer instead of silently producing nothing.
    #[test]
    fn unsupported_static_format_is_a_named_error() {
        let dir = fixture();
        let root = dir.path().to_str().unwrap();
        let matches = crate::build_codefang_command()
            .try_get_matches_from(["codefang", "run", "-a", "static/complexity", root])
            .expect("parse");
        let sub = matches.subcommand_matches("run").expect("run");
        let ctx = RunContext::from_matches(sub);
        let registry = default_registry();
        let entry = registry.lookup("static/complexity").expect("registered");
        let err = (entry.run)(&ctx, "ndjson").expect_err("ndjson is not a static format");
        assert_eq!(
            err.to_string(),
            "format ndjson is not supported for static/complexity"
        );
    }
}

#[cfg(test)]
mod history_filter_tests {
    //! The history-phase change filter (`--languages`, the vendor/generated
    //! policy and the generated-content marker) — exercised both at the
    //! filter level on a real multi-language git fixture and through the REAL
    //! registry dispatch.

    use super::*;

    /// A two-commit repo exercising every leg of the filter: a Go file
    /// (insert + modify), a Python file (insert + delete — the delete-side
    /// name leg), an EXTENSIONLESS script whose language is detectable only
    /// from blob content, a vendor-classified dotfile (the policy leg), a
    /// Markdown doc (neither set member), and a Go file carrying the
    /// generated-content marker (the header leg).
    fn history_fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = git2::Repository::init(dir.path()).expect("git init");
        let sig = git2::Signature::new(
            "Dev One",
            "dev@example.com",
            &git2::Time::new(1_700_000_000, 0),
        )
        .expect("signature");
        let commit = |message: &str, files: &[(&str, Option<&str>)]| {
            let mut index = repo.index().expect("index");
            for (path, content) in files {
                match content {
                    Some(text) => {
                        std::fs::write(dir.path().join(path), text).expect("write");
                        index
                            .add_path(std::path::Path::new(path))
                            .expect("add_path");
                    }
                    None => index
                        .remove_path(std::path::Path::new(path))
                        .expect("remove_path"),
                }
            }
            index.write().expect("index write");
            let tree_oid = index.write_tree().expect("write tree");
            let tree = repo.find_tree(tree_oid).expect("find tree");
            let parents: Vec<git2::Commit> = repo
                .head()
                .ok()
                .and_then(|h| h.peel_to_commit().ok())
                .into_iter()
                .collect();
            let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
            repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parent_refs)
                .expect("commit");
        };
        // main.go: 5 lines; app.py: 2; deploy (extensionless python): 2;
        // .gitignore (vendor-classified): 2; README.md: 1; marked.go: 3
        // (generated marker in the header).
        commit(
            "init",
            &[
                (
                    "main.go",
                    Some("package main\n\nfunc main() {\n\tprintln(\"hi\")\n}\n"),
                ),
                ("app.py", Some("#!/usr/bin/env python3\nprint('x')\n")),
                ("deploy", Some("#!/usr/bin/env python3\nprint('d')\n")),
                (".gitignore", Some("target/\nbuild/\n")),
                ("README.md", Some("# docs\n")),
                (
                    "marked.go",
                    Some("// Code generated by gen. DO NOT EDIT.\npackage main\n\nfunc g() {}\n"),
                ),
            ],
        );
        // main.go grows by one line; app.py is deleted.
        commit(
            "second",
            &[
                (
                    "main.go",
                    Some("package main\n\nfunc main() {\n\tprintln(\"hi\")\n\tprintln(\"x\")\n}\n"),
                ),
                ("app.py", None),
            ],
        );
        dir
    }

    /// Parses `codefang run <args...> <any-path>` into run matches and builds
    /// the history filter from them (flag layer).
    fn filter_from_run_args(args: &[&str]) -> Result<HistoryFilter, String> {
        let mut argv = vec!["codefang", "run"];
        argv.extend_from_slice(args);
        argv.push(".");
        let matches = crate::build_codefang_command()
            .try_get_matches_from(argv)
            .expect("run args parse");
        let sub = matches.subcommand_matches("run").expect("run subcommand");
        history_filter(sub)
    }

    #[test]
    fn absent_languages_flag_means_no_restriction() {
        let filter = filter_from_run_args(&[]).expect("no flags parses");
        assert_eq!(filter.languages, None);
    }

    #[test]
    fn languages_all_means_no_restriction() {
        let filter = filter_from_run_args(&["--languages", "all"]).expect("parses");
        assert_eq!(filter.languages, None);
    }

    #[test]
    fn languages_tokens_canonicalize_to_lowercase_set() {
        let filter = filter_from_run_args(&["--languages", "Go,python"]).expect("parses");
        let set = filter.languages.expect("restricts");
        assert_eq!(
            set.into_iter().collect::<Vec<_>>(),
            vec!["go".to_string(), "python".to_string()]
        );
    }

    /// `--languages ""` means NO filter in the history phase, exactly as in
    /// the static phase (one shared token parser).
    #[test]
    fn explicit_empty_languages_means_no_restriction() {
        let filter = filter_from_run_args(&["--languages", ""]).expect("parses");
        assert_eq!(filter.languages, None);
    }

    #[test]
    fn unknown_language_errors_with_history_message() {
        let err =
            filter_from_run_args(&["--languages", "notalang"]).expect_err("unknown token errors");
        assert_eq!(err, r#"history --languages: unknown language: "notalang""#);
    }

    /// The include flags reach the history policy (they were inert before).
    #[test]
    fn include_flags_reach_the_history_policy() {
        let filter =
            filter_from_run_args(&["--include-vendored", "--include-generated"]).expect("parses");
        assert!(filter.policy.include_vendored);
        assert!(filter.policy.include_generated);
        let default = filter_from_run_args(&[]).expect("parses");
        assert!(!default.policy.include_vendored);
        assert!(!default.policy.include_generated);
    }

    /// Walks the fixture and returns the retained change (name, action)
    /// pairs under `filter` — the exact `retain_changes` path every history
    /// walk shares.
    fn retained_names(
        dir: &std::path::Path,
        filter: &HistoryFilter,
    ) -> Vec<(String, cf_gitlib::changes::ChangeAction)> {
        let repo = cf_gitlib::Repository::open(dir.to_str().unwrap()).expect("open");
        let hashes = load_history_commit_hashes(
            &repo,
            &HistoryWindow {
                head: false,
                limit: 0,
                first_parent: false,
                since: SinceSpec::Inactive,
            },
        )
        .expect("history");
        let mut out = Vec::new();
        for hash in hashes {
            let commit = repo.lookup_commit(hash).expect("commit");
            let mut changes = history::commit_tree_changes(&repo, &commit).expect("diff");
            filter.retain_changes(&repo, &mut changes);
            out.extend(changes.into_iter().map(|c| {
                let name = if matches!(c.action, cf_gitlib::changes::ChangeAction::Delete) {
                    c.from.name
                } else {
                    c.to.name
                };
                (name, c.action)
            }));
        }
        out
    }

    #[test]
    fn language_filter_keeps_matching_paths_and_vendor_policy_holds() {
        let dir = history_fixture();
        let filter = filter_from_run_args(&["--languages", "Go"]).expect("parses");
        let retained = retained_names(dir.path(), &filter);
        // Two main.go changes survive; the Python files, README.md, the
        // vendor-classified .gitignore and the generated-marked marked.go do
        // NOT.
        assert_eq!(
            retained,
            vec![
                (
                    "main.go".to_string(),
                    cf_gitlib::changes::ChangeAction::Insert
                ),
                (
                    "main.go".to_string(),
                    cf_gitlib::changes::ChangeAction::Modify
                ),
            ]
        );
    }

    #[test]
    fn language_filter_uses_blob_content_for_extensionless_files() {
        let dir = history_fixture();
        let filter = filter_from_run_args(&["--languages", "Python"]).expect("parses");
        let retained = retained_names(dir.path(), &filter);
        let names: Vec<&str> = retained.iter().map(|(n, _)| n.as_str()).collect();
        // app.py by extension; `deploy` by SHEBANG content detection; the
        // app.py delete keeps the FROM-side name.
        assert_eq!(names, vec!["app.py", "deploy", "app.py"]);
        assert!(matches!(
            retained[2].1,
            cf_gitlib::changes::ChangeAction::Delete
        ));
    }

    /// An empty language SET (constructible only in code) retains nothing —
    /// the filter-level invariant behind `languages: None` meaning "no
    /// filter".
    #[test]
    fn empty_language_set_retains_nothing() {
        let dir = history_fixture();
        let filter = HistoryFilter {
            policy: cf_pathpolicy::Options::default(),
            languages: Some(std::collections::BTreeSet::new()),
        };
        assert!(retained_names(dir.path(), &filter).is_empty());
    }

    /// The generated-content marker (first 512 bytes) excludes a change under
    /// the default policy; `--include-generated` keeps it.
    #[test]
    fn generated_marker_in_blob_header_is_applied() {
        let dir = history_fixture();
        let default = filter_from_run_args(&[]).expect("parses");
        let names: Vec<String> = retained_names(dir.path(), &default)
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert!(
            !names.iter().any(|n| n == "marked.go"),
            "the generated marker must exclude marked.go by default, got {names:?}"
        );
        let permissive = filter_from_run_args(&["--include-generated"]).expect("parses");
        let names: Vec<String> = retained_names(dir.path(), &permissive)
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert!(
            names.iter().any(|n| n == "marked.go"),
            "--include-generated must keep marked.go, got {names:?}"
        );
    }

    /// Runs `codefang run --analyzers <id> --format json <extra..> <root>`
    /// through the real argv parse + registry handler, returning the parsed
    /// json aggregate.
    fn dispatch_aggregate(id: &str, extra: &[&str], root: &std::path::Path, key: &str) -> i64 {
        let root = root.to_str().unwrap();
        let mut argv = vec!["codefang", "run", "--analyzers", id, "--format", "json"];
        argv.extend_from_slice(extra);
        argv.push(root);
        let matches = crate::build_codefang_command()
            .try_get_matches_from(argv)
            .expect("run args parse");
        let sub = matches.subcommand_matches("run").expect("run subcommand");
        let ctx = RunContext::from_matches(sub);
        let registry = default_registry();
        let entry = registry.lookup(id).expect("analyzer registered");
        let bytes = (entry.run)(&ctx, "json").expect("handler produced a report");
        let json = String::from_utf8(bytes).expect("utf8");
        let agg = r#""aggregate":{"#;
        let pos = json.find(agg).expect("aggregate object");
        let from = &json[pos + agg.len()..];
        let needle = format!(r#""{key}":"#);
        let kpos = from.find(&needle).expect("aggregate key");
        let rest = &from[kpos + needle.len()..];
        rest[..rest.find([',', '}']).expect("value end")]
            .parse()
            .expect("integer value")
    }

    #[test]
    fn dispatch_history_devs_applies_language_filter() {
        let dir = history_fixture();
        let root = dir.path();

        let added_go = dispatch_aggregate(
            "history/devs",
            &["--languages", "Go"],
            root,
            "total_lines_added",
        );
        // main.go insert (+5) + modify (+1); everything else filtered away
        // (marked.go carries the generated marker).
        assert_eq!(added_go, 6);

        let (added_py, removed_py) = (
            dispatch_aggregate(
                "history/devs",
                &["--languages", "Python"],
                root,
                "total_lines_added",
            ),
            dispatch_aggregate(
                "history/devs",
                &["--languages", "Python"],
                root,
                "total_lines_removed",
            ),
        );
        // app.py (+2) + deploy (+2, content-detected); delete contributes −2.
        assert_eq!((added_py, removed_py), (4, 2));

        let all_added = dispatch_aggregate("history/devs", &[], root, "total_lines_added");
        // Unfiltered: 6 (Go) + 2 (app.py) + 2 (deploy) + 1 (README.md);
        // .gitignore is vendor-classified and marked.go generated — both
        // excluded even unfiltered.
        assert_eq!(all_added, 11);

        // `--include-generated` adds marked.go's 4 lines.
        let with_generated = dispatch_aggregate(
            "history/devs",
            &["--include-generated"],
            root,
            "total_lines_added",
        );
        assert_eq!(with_generated, 15);

        // `--languages ""` is no filter: both commits are attributed.
        let empty_commits =
            dispatch_aggregate("history/devs", &["--languages", ""], root, "total_commits");
        assert_eq!(empty_commits, 2);
    }
}

#[cfg(test)]
mod history_window_tests {
    use super::*;
    use cf_gitlib::helpers::parse_time;
    use cf_gitlib::testutil::{commit_files, init_repo, TestRepo};
    use cf_gitlib::Hash;

    /// Base instant of the fixture history: 2021-01-01T00:00:00Z.
    const T0: i64 = 1_609_459_200;
    /// Spacing between fixture commits (one hour).
    const STEP: i64 = 3_600;
    /// Fixture length: five linear commits; `hashes[0]` is the oldest.
    const N_COMMITS: usize = 5;
    /// The 3rd commit's instant as an RFC3339 cutoff (T0 + 2*STEP).
    const CUTOFF_3RD: &str = "2021-01-01T02:00:00Z";
    /// A date after the newest commit.
    const DATE_AFTER_HEAD: &str = "2021-01-02";
    /// A date at/before the fixture root, so `--since` filters nothing.
    const DATE_BEFORE_ROOT: &str = "2020-01-01";
    /// Restores `CODEFANG_NOW` on scope exit so an injected clock cannot leak
    /// into a sibling test thread.
    struct NowGuard;

    impl Drop for NowGuard {
        fn drop(&mut self) {
            std::env::remove_var("CODEFANG_NOW");
        }
    }

    /// Builds the five-commit linear fixture; returns the repo and its commit
    /// hashes in oldest-first order.
    fn linear_repo() -> (TestRepo, Vec<Hash>) {
        let test = init_repo().expect("init fixture repo");
        let mut hashes = Vec::new();
        for i in 0..N_COMMITS {
            let h = commit_files(
                &test,
                &format!("commit {i}"),
                T0 + i64::try_from(i).expect("index fits i64") * STEP,
                &[("f.txt", format!("v{i}").as_bytes())],
            )
            .expect("fixture commit");
            hashes.push(h);
        }
        (test, hashes)
    }

    fn window(limit: usize, since: SinceSpec) -> HistoryWindow {
        HistoryWindow {
            head: false,
            limit,
            first_parent: false,
            since,
        }
    }

    /// `--limit N` selects the N NEWEST commits, delivered oldest-first.
    #[test]
    fn limit_selects_the_newest_commits_oldest_first() {
        let (test, hashes) = linear_repo();
        let got =
            load_history_commit_hashes(&test.repo, &window(2, SinceSpec::Inactive)).expect("loads");
        assert_eq!(got.as_slice(), &hashes[3..]);
    }

    /// `--limit 0` (unset) is "no limit".
    #[test]
    fn limit_zero_loads_whole_history() {
        let (test, hashes) = linear_repo();
        let got =
            load_history_commit_hashes(&test.repo, &window(0, SinceSpec::Inactive)).expect("loads");
        assert_eq!(got.as_slice(), &hashes[..]);
    }

    /// `--head` is exactly `[HEAD]`, whatever the other bounds say.
    #[test]
    fn head_is_exactly_the_head_commit() {
        let (test, hashes) = linear_repo();
        let got = load_history_commit_hashes(
            &test.repo,
            &HistoryWindow {
                head: true,
                limit: 3,
                first_parent: false,
                since: SinceSpec::Active(DATE_BEFORE_ROOT.to_owned()),
            },
        )
        .expect("loads");
        assert_eq!(got, vec![hashes[4]]);
        assert_eq!(
            got,
            load_history_commit_hashes(&test.repo, &window(1, SinceSpec::Inactive)).expect("loads"),
            "--head and --limit 1 select the same window"
        );
    }

    /// The cutoff literal must mean exactly the 3rd fixture instant.
    #[test]
    fn cutoff_literal_matches_the_fixture_clock() {
        assert_eq!(
            parse_time(CUTOFF_3RD).expect("cutoff parses"),
            T0 + 2 * STEP,
            "CUTOFF_3RD must name the 3rd commit's instant"
        );
    }

    #[test]
    fn since_partial_window_yields_the_window() {
        let (test, hashes) = linear_repo();
        let got = load_history_commit_hashes(
            &test.repo,
            &window(0, SinceSpec::Active(CUTOFF_3RD.to_owned())),
        )
        .expect("a partial --since window must load, not abort");
        assert_eq!(got.as_slice(), &hashes[2..]);
    }

    #[test]
    fn since_cutoff_after_head_yields_empty_window() {
        let (test, _hashes) = linear_repo();
        let got = load_history_commit_hashes(
            &test.repo,
            &window(0, SinceSpec::Active(DATE_AFTER_HEAD.to_owned())),
        )
        .expect("a fully-excluded window is empty, not an error");
        assert!(got.is_empty(), "window must be empty, got {got:?}");
    }

    #[test]
    fn since_cutoff_at_or_before_root_yields_whole_history() {
        let (test, hashes) = linear_repo();
        let got = load_history_commit_hashes(
            &test.repo,
            &window(0, SinceSpec::Active(DATE_BEFORE_ROOT.to_owned())),
        )
        .expect("loads");
        assert_eq!(got.as_slice(), &hashes[..]);
    }

    /// `--limit` caps the NEWEST end of the `--since` window.
    #[test]
    fn limit_caps_the_newest_end_of_the_since_window() {
        let (test, hashes) = linear_repo();
        let cutoff = CUTOFF_3RD.to_owned();
        let unbounded =
            load_history_commit_hashes(&test.repo, &window(0, SinceSpec::Active(cutoff.clone())))
                .expect("window loads");
        assert_eq!(unbounded.as_slice(), &hashes[2..]);
        let capped = load_history_commit_hashes(&test.repo, &window(2, SinceSpec::Active(cutoff)))
            .expect("window loads");
        assert_eq!(capped.as_slice(), &hashes[3..]);
    }

    #[test]
    fn since_accepts_a_git_revision() {
        let (test, hashes) = linear_repo();
        let got = load_history_commit_hashes(
            &test.repo,
            &window(0, SinceSpec::Active("HEAD~2".to_owned())),
        )
        .expect("--since must resolve a revision like `HEAD~2`");
        assert_eq!(got.as_slice(), &hashes[2..]);
    }

    /// Duration forms resolve against the injectable clock (DESIGN §2.8), not
    /// the wall clock.
    #[test]
    fn duration_since_uses_the_injected_clock() {
        let _guard = NowGuard;
        std::env::set_var("CODEFANG_NOW", (T0 + 3 * STEP).to_string());
        let (test, hashes) = linear_repo();
        let got =
            load_history_commit_hashes(&test.repo, &window(0, SinceSpec::Active("1h".to_owned())))
                .expect("`--since 1h` must resolve against CODEFANG_NOW");
        assert_eq!(got.as_slice(), &hashes[2..]);
    }

    #[test]
    fn unresolvable_since_aborts_the_run() {
        let (test, _hashes) = linear_repo();
        assert!(
            load_history_commit_hashes(
                &test.repo,
                &window(0, SinceSpec::Active("not-a-time-or-ref".to_owned()))
            )
            .is_none(),
            "an unresolvable --since must yield None"
        );
    }
}

#[cfg(test)]
mod preflight_tests {
    //! The shared run validation: every user error has a named message.

    use super::*;

    fn ctx_err(args: &[&str], static_ids: &[&str], history_ids: &[&str]) -> Option<String> {
        let mut argv = vec!["codefang", "run"];
        argv.extend_from_slice(args);
        let matches = crate::build_codefang_command()
            .try_get_matches_from(argv)
            .expect("run args parse");
        let sub = matches.subcommand_matches("run").expect("run subcommand");
        let ctx = RunContext::from_matches(sub);
        let s: Vec<String> = static_ids.iter().map(|s| (*s).to_string()).collect();
        let h: Vec<String> = history_ids.iter().map(|s| (*s).to_string()).collect();
        preflight(&ctx, &s, &h).err().map(|e| e.to_string())
    }

    #[test]
    fn missing_path_is_named() {
        assert_eq!(
            ctx_err(&["/no/such/dir"], &["static/complexity"], &[]),
            Some("path not found: /no/such/dir".to_string())
        );
    }

    #[test]
    fn non_repository_is_named_for_history_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = dir.path().to_str().unwrap();
        assert_eq!(
            ctx_err(&[p], &[], &["history/devs"]),
            Some(format!("cannot open repository: {p}"))
        );
        // A static selection never needs a repository.
        assert_eq!(ctx_err(&[p], &["static/complexity"], &[]), None);
    }

    #[test]
    fn formats_are_validated_per_phase() {
        let dir = tempfile::tempdir().expect("tempdir");
        git2::Repository::init(dir.path()).expect("git init");
        let p = dir.path().to_str().unwrap();
        assert_eq!(
            ctx_err(&["--format", "bogus", p], &["static/complexity"], &[]),
            Some("unsupported format: bogus".to_string())
        );
        assert_eq!(
            ctx_err(&["--format", "ndjson", p], &["static/complexity"], &[]),
            Some("format ndjson is not supported for static/complexity".to_string())
        );
        for format in ["compact", "text"] {
            assert_eq!(
                ctx_err(
                    &["--format", format, p],
                    &["static/complexity"],
                    &["history/devs"]
                ),
                None,
                "a mixed selection renders --format {format} phase by phase"
            );
        }
        assert_eq!(
            ctx_err(&["--format", "compact", p], &[], &["history/devs"]),
            None
        );
        assert_eq!(
            ctx_err(&["--format", "plot", p], &[], &["history/devs"]),
            None
        );
    }

    #[test]
    fn numeric_history_flags_are_validated() {
        let dir = tempfile::tempdir().expect("tempdir");
        git2::Repository::init(dir.path()).expect("git init");
        let p = dir.path().to_str().unwrap();
        assert_eq!(
            ctx_err(&["--limit=-5", p], &[], &["history/devs"]),
            Some("invalid --limit: must be >= 0, got -5".to_string())
        );
        assert_eq!(
            ctx_err(&["--tick-size", "0", p], &[], &["history/devs"]),
            Some("invalid --tick-size: must be > 0, got 0".to_string())
        );
        assert_eq!(
            ctx_err(&["--max-changes-per-commit=-1", p], &[], &["history/devs"]),
            Some("invalid --max-changes-per-commit: must be >= 0, got -1".to_string())
        );
        assert_eq!(
            ctx_err(&["--people-dict", "/no/such/people.txt", p], &[], &["history/devs"]),
            Some(
                "cannot read --people-dict /no/such/people.txt: No such file or directory (os error 2)"
                    .to_string()
            )
        );
        // A static selection ignores the history flags entirely.
        assert_eq!(
            ctx_err(&["--limit=-5", p], &["static/complexity"], &[]),
            None
        );
    }

    /// A non-positive band, sample or typo distance was silently replaced by
    /// the default; it is an error now.
    #[test]
    fn non_positive_analyzer_sizes_are_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        git2::Repository::init(dir.path()).expect("git init");
        let p = dir.path().to_str().unwrap();
        for (flag, value) in [
            ("--granularity", "0"),
            ("--sampling", "0"),
            ("--granularity", "-3"),
            ("--typos-max-distance", "0"),
        ] {
            let arg = format!("{flag}={value}");
            assert_eq!(
                ctx_err(&[&arg, p], &[], &["history/burndown"]),
                Some(format!("invalid {flag}: must be > 0, got {value}"))
            );
        }
        assert_eq!(
            ctx_err(
                &["--granularity", "1", "--sampling", "1", p],
                &[],
                &["history/burndown"]
            ),
            None
        );
    }

    /// `--ndjson` only modifies `--format timeseries`; with any other format
    /// it was accepted and ignored.
    #[test]
    fn ndjson_requires_the_timeseries_format() {
        let dir = tempfile::tempdir().expect("tempdir");
        git2::Repository::init(dir.path()).expect("git init");
        let p = dir.path().to_str().unwrap();
        assert_eq!(
            ctx_err(&["--ndjson", p], &[], &["history/devs"]),
            Some("invalid --ndjson: requires --format timeseries, got --format json".to_string())
        );
        assert_eq!(
            ctx_err(
                &["--ndjson", "--format", "ndjson", p],
                &[],
                &["history/devs"]
            ),
            Some("invalid --ndjson: requires --format timeseries, got --format ndjson".to_string())
        );
        assert_eq!(
            ctx_err(
                &["--ndjson", "--format", "timeseries", p],
                &[],
                &["history/devs"]
            ),
            None
        );
    }
}
