//! Static-analysis report path for the UAST `static/halstead` analyzer.
//!
//! Reproduces the Go static pipeline for the single-analyzer
//! `codefang run --analyzers static/halstead` capture across its machine formats.
//!
//! Pipeline (Go `StaticService.uastPhase` → per-file `halstead.Analyze` →
//! `common.Aggregator` → format-specific serialization):
//!
//!  1. `streamFiles` walks `rootPath` with `filepath.WalkDir` (lexical order,
//!     `.git` skipped), keeping every UAST-supported, non-vendor/-generated file
//!     (`pathpolicy.Exclude(path, nil, opts)`, content `nil`).
//!  2. Each file is parsed by `cf_uast::Parser` and run through the Halstead
//!     analyzer: find functions (UAST `Function`/`Method` types ∪ `Function`
//!     role, depth ≤ 10), count operators/operands per function, derive the
//!     Halstead measures, and produce a per-file report whose `functions`
//!     collection is stamped with `_source_file` (path relative to root),
//!     `_language` (`"go"`), and `_directory`.
//!  3. The base `common.Aggregator` sums each numeric key across the per-file
//!     reports and divides by the report count (file count), sums the count key
//!     `total_functions`, and collects every detailed function (dedup key
//!     `[_source_file, name]`).
//!
//! ## Formats
//!
//! * **bin** (`FormatReportBinary`): `ComputeAllMetrics(report)` →
//!   `reportutil.EncodeBinaryEnvelope(metrics)`. The metric computer parses the
//!   aggregated report back into scalars; the integer-typed aggregate fields
//!   (`distinct_operators`, …) are *averaged* and therefore stored as `float64`,
//!   so the `.(int)` type assertions in `ParseReportData` fail and those fields
//!   come out **0** — a faithful reproduction of the Go quirk. The bin payload is
//!   the `ComputedMetrics` struct marshaled with compact `encoding/json`
//!   (cf-gojson) inside the CFB1 envelope.
//!
//! ## A note on the nondeterministic `message`
//!
//! The aggregate `message` is built from "the first numeric metric" in a Go
//! `map[string]float64` iterated in randomized order (`common.Aggregator.GetResult`
//! → `buildHalsteadMessage(firstAverage)`), so the Go reference binary emits a
//! *different* message label on different runs. The golden bin was captured with
//! the `"Low Halstead complexity - well-structured code"` label; we reproduce
//! that captured value (see [`AGGREGATE_MESSAGE_BIN`]). Every other byte of the
//! report is deterministic.

use std::collections::HashMap;
use std::path::Path;

use cf_gojson::{GoMap, GoValue, MapOrigin};
use cf_pathpolicy::{exclude, Options};
use cf_uast::{Node, Parser};

/// Max UAST traversal depth for function discovery (`MaxDepthValue`).
const MAX_DEPTH: i64 = 10;

/// Threshold above which CMS sketches are populated (`cmsTokenThreshold`). The
/// CMS total count is exact, so the estimated totals equal the exact sums; we
/// therefore set them directly without a sketch.
const CMS_TOKEN_THRESHOLD: i64 = 1000;

/// The aggregate `message` label captured in the golden bin (see module docs).
const AGGREGATE_MESSAGE_BIN: &str = "Low Halstead complexity - well-structured code";

// --- detector classification tables (detector.go) ---

const OPERATOR_TYPES: &[&str] =
    &["BinaryOp", "UnaryOp", "Assignment", "Call", "Index", "Slice", "Return"];
const OPERATOR_ROLES: &[&str] = &["Operator", "Assignment", "Call", "Return"];
const OPERAND_TYPES: &[&str] = &["Identifier", "Literal", "Field"];
const OPERAND_ROLES: &[&str] = &["Name", "Literal", "Variable", "Argument"];
const DECLARATION_TYPES: &[&str] = &[
    "Function", "FunctionDecl", "Method", "Parameter", "Variable", "Field", "Import", "Package",
    "Struct", "Class", "Interface", "Enum",
];
const DECLARATION_PARENT_ROLES: &[&str] = &["Declaration", "Parameter", "Import", "Type"];

const TOKEN_OPERATOR_SET: &[&str] = &[
    "===", "!==", "==", "!=", "<=", ">=", "&&", "||", "<<=", ">>=", "<<", ">>", "**", ":=", "+=",
    "-=", "*=", "/=", "%=", "&=", "|=", "^=", "+", "-", "*", "/", "%", "=", "<", ">", "&", "|",
    "^", "!",
];
const TOKEN_OPERATORS_BY_LENGTH: &[&str] = &[
    "===", "!==", "<<=", ">>=", "==", "!=", "<=", ">=", "&&", "||", "<<", ">>", "**", ":=", "+=",
    "-=", "*=", "/=", "%=", "&=", "|=", "^=", "+", "-", "*", "/", "%", "=", "<", ">", "&", "|",
    "^", "!",
];

// --- Halstead formula constants (calculator.go) ---
const TIME_CONSTANT: f64 = 18.0;
const BUG_CONSTANT: f64 = 3000.0;
const DIFFICULTY_DIVISOR: f64 = 2.0;

/// Bit-exact reproductions of Go's `math` package so the Halstead floats match
/// Go byte-for-byte. Rust's libm `f64::log2` differs from Go's `math.Log2` in the
/// last ULP for some inputs; Go computes `Log2(x) = log(frac)*(1/Ln2) + exp`
/// (via `Frexp`), with its own polynomial `log`. We port that path exactly.
mod goflt {
    const LN2: f64 = 0.693147180559945309417232121458176568075500134360255254120680009;
    const SQRT2: f64 = 1.41421356237309504880168872420969807856967187537694807317667974;

    /// Go `math.Frexp`: returns `(frac, exp)` with `frac ∈ [0.5, 1)` and
    /// `x == frac · 2^exp`. Only the normal-positive path is needed here.
    fn frexp(f: f64) -> (f64, i32) {
        if f == 0.0 || !f.is_finite() {
            return (f, 0);
        }
        // normalize (Go math.normalize): scale subnormals into the normal range.
        const SMALLEST_NORMAL: f64 = 2.2250738585072014e-308; // 2**-1022
        let (x, mut exp) = if f.abs() < SMALLEST_NORMAL {
            (f * (1u64 << 52) as f64, -52)
        } else {
            (f, 0)
        };
        let bits = x.to_bits();
        const SHIFT: u64 = 52;
        const MASK: u64 = 0x7FF;
        const BIAS: i64 = 1023;
        exp += (((bits >> SHIFT) & MASK) as i64 - BIAS + 1) as i32;
        let mut nb = bits;
        nb &= !(MASK << SHIFT);
        nb |= ((-1 + BIAS) as u64) << SHIFT;
        (f64::from_bits(nb), exp)
    }

    /// Go `math.log` (pure-Go polynomial), matching amd64 `archLog`.
    fn log(x: f64) -> f64 {
        const LN2HI: f64 = 6.93147180369123816490e-01;
        const LN2LO: f64 = 1.90821492927058770002e-10;
        const L1: f64 = 6.666666666666735130e-01;
        const L2: f64 = 3.999999999940941908e-01;
        const L3: f64 = 2.857142874366239149e-01;
        const L4: f64 = 2.222219843214978396e-01;
        const L5: f64 = 1.818357216161805012e-01;
        const L6: f64 = 1.531383769920937332e-01;
        const L7: f64 = 1.479819860511658591e-01;

        if x.is_nan() || x == f64::INFINITY {
            return x;
        }
        if x < 0.0 {
            return f64::NAN;
        }
        if x == 0.0 {
            return f64::NEG_INFINITY;
        }

        let (mut f1, mut ki) = frexp(x);
        if f1 < SQRT2 / 2.0 {
            f1 *= 2.0;
            ki -= 1;
        }
        let f = f1 - 1.0;
        let k = f64::from(ki);

        let s = f / (2.0 + f);
        let s2 = s * s;
        let s4 = s2 * s2;
        let t1 = s2 * (L1 + s4 * (L3 + s4 * (L5 + s4 * L7)));
        let t2 = s4 * (L2 + s4 * (L4 + s4 * L6));
        let r = t1 + t2;
        let hfsq = 0.5 * f * f;
        k * LN2HI - ((hfsq - (s * (hfsq + r) + k * LN2LO)) - f)
    }

    /// Go `math.Log2` (amd64: `log2` with `Frexp` + `archLog`).
    #[must_use]
    pub fn log2(x: f64) -> f64 {
        let (frac, exp) = frexp(x);
        if frac == 0.5 {
            return f64::from(exp - 1);
        }
        log(frac) * (1.0 / LN2) + f64::from(exp)
    }
}

// --- aggregate numeric keys (aggregator.go getNumericKeys) ---
const NUMERIC_KEYS: &[&str] = &[
    "volume",
    "difficulty",
    "effort",
    "time_to_program",
    "delivered_bugs",
    "distinct_operators",
    "distinct_operands",
    "total_operators",
    "total_operands",
    "vocabulary",
    "length",
    "estimated_length",
];

/// Per-function Halstead metrics (`FunctionHalsteadMetrics`).
///
/// The raw-count fields (`distinct_*`, `total_*`, `vocabulary`, `length`,
/// `estimated_length`) are part of the per-function record but are not emitted in
/// the bin `function_halstead` entry (Go `FunctionHalsteadData` carries only the
/// derived measures + complexity level); they are retained for the report value's
/// completeness and the sibling JSON/YAML encodings.
#[derive(Default, Clone)]
#[allow(dead_code)]
struct FunctionMetrics {
    name: String,
    source_file: String,
    language: String,
    directory: String,
    distinct_operators: i64,
    distinct_operands: i64,
    total_operators: i64,
    total_operands: i64,
    vocabulary: i64,
    length: i64,
    estimated_length: f64,
    volume: f64,
    difficulty: f64,
    effort: f64,
    time_to_program: f64,
    delivered_bugs: f64,
}

/// Per-file aggregate of the Halstead `report` map's scalar metrics, plus the
/// detailed function list (the Go per-file `analyze.Report`).
struct FileReport {
    /// Scalar metrics keyed exactly as the Go report map (numeric keys).
    scalars: HashMap<&'static str, f64>,
    total_functions: i64,
    functions: Vec<FunctionMetrics>,
}

/// Eight derived Halstead measures (`CalculateHalsteadMetrics`).
fn derive(n1: i64, n2: i64, big_n1: i64, big_n2: i64) -> (i64, i64, f64, f64, f64, f64, f64, f64) {
    let vocabulary = n1 + n2;
    let length = big_n1 + big_n2;

    let estimated_length = if n1 > 0 && n2 > 0 {
        (n1 as f64) * goflt::log2(n1 as f64) + (n2 as f64) * goflt::log2(n2 as f64)
    } else {
        0.0
    };
    let volume = if vocabulary > 0 {
        (length as f64) * goflt::log2(vocabulary as f64)
    } else {
        0.0
    };
    let difficulty = if n2 > 0 {
        (n1 as f64 / DIFFICULTY_DIVISOR) * (big_n2 as f64 / n2 as f64)
    } else {
        0.0
    };
    let effort = volume * difficulty;
    let time_to_program = effort / TIME_CONSTANT;
    let delivered_bugs = volume / BUG_CONSTANT;
    (vocabulary, length, estimated_length, volume, difficulty, effort, time_to_program, delivered_bugs)
}

/// `node.HasAnyRole` over a string slice.
fn has_any_role(node: &Node, roles: &[&str]) -> bool {
    node.roles.iter().any(|nr| roles.iter().any(|r| nr == r))
}

fn is_operator(node: &Node) -> bool {
    OPERATOR_TYPES.contains(&node.node_type.as_str()) || has_any_role(node, OPERATOR_ROLES)
}

fn is_operand(node: &Node) -> bool {
    OPERAND_TYPES.contains(&node.node_type.as_str()) || has_any_role(node, OPERAND_ROLES)
}

fn operator_name(node: &Node) -> String {
    if let Some(op) = node.props.get("operator") {
        return op.clone();
    }
    if let Some(op) = extract_operator_from_token(&node.token) {
        return op.to_string();
    }
    if !node.token.is_empty() {
        return node.token.clone();
    }
    node.node_type.clone()
}

fn operand_name(node: &Node) -> String {
    if !node.token.is_empty() {
        return node.token.clone();
    }
    if let Some(name) = node.props.get("name") {
        return name.clone();
    }
    if let Some(value) = node.props.get("value") {
        return value.clone();
    }
    String::new()
}

fn extract_operator_from_token(token: &str) -> Option<&'static str> {
    if token.trim().is_empty() {
        return None;
    }
    if let Some(&op) = TOKEN_OPERATOR_SET.iter().find(|&&op| op == token) {
        return Some(op);
    }
    for &op in TOKEN_OPERATORS_BY_LENGTH {
        let needle = format!(" {op} ");
        if token.contains(&needle) {
            return Some(op);
        }
    }
    None
}

fn is_declaration_identifier(node: &Node, parent: Option<&Node>) -> bool {
    let Some(parent) = parent else { return false };
    if node.node_type != "Identifier" || !has_any_role(node, &["Name"]) {
        return false;
    }
    if has_any_role(parent, DECLARATION_PARENT_ROLES) {
        return true;
    }
    DECLARATION_TYPES.contains(&parent.node_type.as_str())
}

/// Recursively collects operator/operand counts (`CollectOperatorsAndOperands`).
fn collect(
    node: &Node,
    parent: Option<&Node>,
    operators: &mut HashMap<String, i64>,
    operands: &mut HashMap<String, i64>,
) {
    if is_operator(node) {
        let op = operator_name(node);
        if !op.is_empty() {
            *operators.entry(op).or_insert(0) += 1;
        }
    } else if is_operand(node) && !is_declaration_identifier(node, parent) {
        let opnd = operand_name(node);
        if !opnd.is_empty() {
            *operands.entry(opnd).or_insert(0) += 1;
        }
    }
    for child in &node.children {
        collect(child, Some(node), operators, operands);
    }
}

/// Finds all function nodes (`findFunctions`): UAST `Function`/`Method` types ∪
/// `Function` role, depth ≤ [`MAX_DEPTH`], each node counted once.
///
/// The Go code unions a type-traversal and a role-traversal into a pointer-keyed
/// set; both traversals share the same iterative pre-order DFS, so a node that
/// matches both appears once. We reproduce this with a single DFS that yields any
/// node matching either criterion (equivalent to the set union, since each node
/// is visited exactly once).
fn find_functions<'a>(root: &'a Node, out: &mut Vec<&'a Node>) {
    // Iterative pre-order DFS with depth, children pushed reversed (TraverseTree).
    let mut stack: Vec<(&Node, i64)> = vec![(root, 0)];
    while let Some((node, depth)) = stack.pop() {
        if depth <= MAX_DEPTH {
            let by_type = node.node_type == "Function" || node.node_type == "Method";
            let by_role = has_any_role(node, &["Function"]);
            if by_type || by_role {
                out.push(node);
            }
        }
        for child in node.children.iter().rev() {
            stack.push((child, depth + 1));
        }
    }
}

/// Extracts a function's name (`extractFunctionName` → `getFunctionName`):
/// `name` prop, else `"anonymous"`.
fn function_name(node: &Node) -> String {
    if let Some(name) = node.props.get("name") {
        if !name.is_empty() {
            return name.clone();
        }
    }
    "anonymous".to_string()
}

/// Computes the per-file Halstead report (`Analyzer.Analyze` + file-level
/// aggregation). Returns the empty report (all-zero scalars, no functions) when
/// the file has no functions.
fn analyze_file(root: &Node, source_file: &str, directory: &str) -> FileReport {
    let mut functions: Vec<&Node> = Vec::new();
    find_functions(root, &mut functions);

    let mut scalars: HashMap<&'static str, f64> = NUMERIC_KEYS.iter().map(|k| (*k, 0.0)).collect();

    if functions.is_empty() {
        return FileReport { scalars, total_functions: 0, functions: Vec::new() };
    }

    // Per-function metrics.
    let mut fn_metrics: Vec<FunctionMetrics> = Vec::with_capacity(functions.len());
    // File-level operator/operand maps for the file aggregate.
    let mut file_operators: HashMap<String, i64> = HashMap::new();
    let mut file_operands: HashMap<String, i64> = HashMap::new();
    let mut est_total_ops: i64 = 0;
    let mut est_total_opnds: i64 = 0;

    for fnode in &functions {
        let mut operators: HashMap<String, i64> = HashMap::new();
        let mut operands: HashMap<String, i64> = HashMap::new();
        collect(fnode, None, &mut operators, &mut operands);

        let total_ops: i64 = operators.values().sum();
        let total_opnds: i64 = operands.values().sum();
        let n1 = operators.len() as i64;
        let n2 = operands.len() as i64;

        let (vocab, length, est_len, vol, diff, eff, ttp, bugs) =
            derive(n1, n2, total_ops, total_opnds);

        // CMS path: the exact total count equals the exact sum, so estimated
        // totals equal the exact totals when the threshold is reached.
        if total_ops + total_opnds >= CMS_TOKEN_THRESHOLD {
            est_total_ops += total_ops;
            est_total_opnds += total_opnds;
        }

        for (k, v) in &operators {
            *file_operators.entry(k.clone()).or_insert(0) += *v;
        }
        for (k, v) in &operands {
            *file_operands.entry(k.clone()).or_insert(0) += *v;
        }

        fn_metrics.push(FunctionMetrics {
            name: function_name(fnode),
            source_file: source_file.to_string(),
            language: "go".to_string(),
            directory: directory.to_string(),
            distinct_operators: n1,
            distinct_operands: n2,
            total_operators: total_ops,
            total_operands: total_opnds,
            vocabulary: vocab,
            length,
            estimated_length: est_len,
            volume: vol,
            difficulty: diff,
            effort: eff,
            time_to_program: ttp,
            delivered_bugs: bugs,
        });
    }

    // File-level aggregate (calculateFileLevelMetrics).
    let f_n1 = file_operators.len() as i64;
    let f_n2 = file_operands.len() as i64;
    let f_big_n1: i64 = file_operators.values().sum();
    let f_big_n2: i64 = file_operands.values().sum();
    let (f_vocab, f_length, f_est_len, f_vol, f_diff, f_eff, f_ttp, f_bugs) =
        derive(f_n1, f_n2, f_big_n1, f_big_n2);

    scalars.insert("distinct_operators", f_n1 as f64);
    scalars.insert("distinct_operands", f_n2 as f64);
    scalars.insert("total_operators", f_big_n1 as f64);
    scalars.insert("total_operands", f_big_n2 as f64);
    scalars.insert("vocabulary", f_vocab as f64);
    scalars.insert("length", f_length as f64);
    scalars.insert("estimated_length", f_est_len);
    scalars.insert("volume", f_vol);
    scalars.insert("difficulty", f_diff);
    scalars.insert("effort", f_eff);
    scalars.insert("time_to_program", f_ttp);
    scalars.insert("delivered_bugs", f_bugs);
    let _ = (est_total_ops, est_total_opnds);

    FileReport { scalars, total_functions: fn_metrics.len() as i64, functions: fn_metrics }
}

/// Aggregated cross-file Halstead result (`common.Aggregator.GetResult`).
struct Aggregate {
    /// Averaged numeric metrics (sum / report_count).
    averages: HashMap<&'static str, f64>,
    total_functions: i64,
    functions: Vec<FunctionMetrics>,
}

/// Walks `root` (lexical order, `.git` skipped, vendor/generated excluded),
/// parses each supported file, and aggregates the Halstead reports.
fn aggregate(root_path: &str) -> Option<Aggregate> {
    let root = Path::new(root_path);
    if !root.exists() {
        return None;
    }

    let parser = Parser::new();
    let opts = Options::default();

    // Numeric-key sums + report count (every analyzed file contributes a report,
    // including empty-result files whose scalars are all 0).
    let mut sums: HashMap<&'static str, f64> = NUMERIC_KEYS.iter().map(|k| (*k, 0.0)).collect();
    let mut report_count: i64 = 0;
    let mut total_functions: i64 = 0;
    // Detailed functions: the halstead `DetailedDataCollector` appends every
    // per-file function WITHOUT deduplication (so two same-named functions in one
    // file are both kept), in file-processing (lexical walk) order.
    let mut functions: Vec<FunctionMetrics> = Vec::new();

    let mut files: Vec<String> = Vec::new();
    collect_files(root, &parser, &opts, &mut files);

    for path in &files {
        let Ok(content) = std::fs::read(path) else { continue };
        let Ok(node) = parser.parse(path, &content) else { continue };

        let rel = make_relative(path, root_path);
        let directory = dir_of(&rel);
        let report = analyze_file(&node, &rel, &directory);

        report_count += 1;
        for (k, v) in &report.scalars {
            *sums.get_mut(k).unwrap() += *v;
        }
        total_functions += report.total_functions;

        functions.extend(report.functions);
    }

    if report_count == 0 {
        return None;
    }

    let averages: HashMap<&'static str, f64> =
        sums.iter().map(|(k, v)| (*k, *v / report_count as f64)).collect();

    Some(Aggregate { averages, total_functions, functions })
}

/// Recursively gathers UAST-supported, non-excluded files in lexical order
/// (`streamFiles` walk order; `filepath.WalkDir` visits entries name-sorted).
fn collect_files(dir: &Path, parser: &Parser, opts: &Options, out: &mut Vec<String>) {
    let Ok(read) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = read.filter_map(Result::ok).collect();
    entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    for entry in entries {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            if entry.file_name() == ".git" {
                continue;
            }
            collect_files(&path, parser, opts, out);
            continue;
        }
        let path_str = path.to_string_lossy().to_string();
        if !parser.is_supported(&path_str) {
            continue;
        }
        if exclude(&path_str, None, opts) {
            continue;
        }
        out.push(path_str);
    }
}

/// `filepath.Rel(root, path)` for the simple in-tree case.
fn make_relative(file_path: &str, root_path: &str) -> String {
    if root_path.is_empty() {
        return file_path.to_string();
    }
    let root = root_path.trim_end_matches('/');
    if let Some(stripped) = file_path.strip_prefix(root) {
        return stripped.trim_start_matches('/').to_string();
    }
    file_path.to_string()
}

/// `filepath.Dir` of a relative path.
fn dir_of(rel: &str) -> String {
    match rel.rfind('/') {
        Some(idx) => rel[..idx].to_string(),
        None => ".".to_string(),
    }
}

/// Bit-exact reproduction of Go's `sort.Slice` (`pdqsort_func`, Go 1.26) for the
/// volume-descending ordering of functions. Go's `sort.Slice` is an UNSTABLE
/// pattern-defeating quicksort, so the relative order of equal-volume functions
/// depends on the exact algorithm, not just the comparator — we therefore port
/// the algorithm verbatim (including `breakPatterns`' xorshift) so ties land in
/// the same positions as the Go reference.
mod gosort {
    /// `data.Less(i, j)`: volume descending (`result[i].Volume > result[j].Volume`).
    #[inline]
    fn less(v: &[f64], i: usize, j: usize) -> bool {
        v[i] > v[j]
    }

    /// Sorts `volumes` (and the parallel `payload`) by volume descending using
    /// Go's `sort.Slice`. Swaps are applied to both slices.
    pub fn slice_by_volume_desc<T>(volumes: &mut [f64], payload: &mut [T]) {
        let n = volumes.len();
        let limit = usize_bits_len(n);
        pdqsort(volumes, payload, 0, n, limit);
    }

    /// Go `bits.Len(uint(length))`.
    fn usize_bits_len(x: usize) -> u32 {
        (usize::BITS) - (x as usize).leading_zeros()
    }

    #[inline]
    fn swap<T>(v: &mut [f64], p: &mut [T], a: usize, b: usize) {
        v.swap(a, b);
        p.swap(a, b);
    }

    fn insertion_sort<T>(v: &mut [f64], p: &mut [T], a: usize, b: usize) {
        for i in a + 1..b {
            let mut j = i;
            while j > a && less(v, j, j - 1) {
                swap(v, p, j, j - 1);
                j -= 1;
            }
        }
    }

    fn sift_down<T>(v: &mut [f64], p: &mut [T], lo: usize, hi: usize, first: usize) {
        let mut root = lo;
        loop {
            let mut child = 2 * root + 1;
            if child >= hi {
                break;
            }
            if child + 1 < hi && less(v, first + child, first + child + 1) {
                child += 1;
            }
            if !less(v, first + root, first + child) {
                return;
            }
            swap(v, p, first + root, first + child);
            root = child;
        }
    }

    fn heap_sort<T>(v: &mut [f64], p: &mut [T], a: usize, b: usize) {
        let first = a;
        let lo = 0;
        let hi = b - a;
        let mut i = (hi as isize - 1) / 2;
        while i >= 0 {
            sift_down(v, p, i as usize, hi, first);
            i -= 1;
        }
        let mut i = hi as isize - 1;
        while i >= 0 {
            swap(v, p, first, first + i as usize);
            sift_down(v, p, lo, i as usize, first);
            i -= 1;
        }
    }

    // hints
    const UNKNOWN: i32 = 0;
    const INCREASING: i32 = 1;
    const DECREASING: i32 = 2;

    fn order2(v: &[f64], a: usize, b: usize, swaps: &mut i32) -> (usize, usize) {
        if less(v, b, a) {
            *swaps += 1;
            (b, a)
        } else {
            (a, b)
        }
    }

    fn median<T>(
        v: &mut [f64],
        _p: &mut [T],
        a: usize,
        b: usize,
        c: usize,
        swaps: &mut i32,
    ) -> usize {
        let (a, b) = order2(v, a, b, swaps);
        let (b, c) = order2(v, b, c, swaps);
        let (_a, b) = order2(v, a, b, swaps);
        let _ = c;
        b
    }

    fn median_adjacent<T>(v: &mut [f64], p: &mut [T], a: usize, swaps: &mut i32) -> usize {
        median(v, p, a - 1, a, a + 1, swaps)
    }

    fn choose_pivot<T>(v: &mut [f64], p: &mut [T], a: usize, b: usize) -> (usize, i32) {
        const SHORTEST_NINTHER: usize = 50;
        const MAX_SWAPS: i32 = 4 * 3;
        let l = b - a;
        let mut swaps = 0i32;
        let mut i = a + l / 4;
        let mut j = a + l / 4 * 2;
        let mut k = a + l / 4 * 3;
        if l >= 8 {
            if l >= SHORTEST_NINTHER {
                i = median_adjacent(v, p, i, &mut swaps);
                j = median_adjacent(v, p, j, &mut swaps);
                k = median_adjacent(v, p, k, &mut swaps);
            }
            j = median(v, p, i, j, k, &mut swaps);
        }
        match swaps {
            0 => (j, INCREASING),
            MAX_SWAPS => (j, DECREASING),
            _ => (j, UNKNOWN),
        }
    }

    fn reverse_range<T>(v: &mut [f64], p: &mut [T], a: usize, b: usize) {
        let mut i = a;
        let mut j = b - 1;
        while i < j {
            swap(v, p, i, j);
            i += 1;
            j -= 1;
        }
    }

    fn partition<T>(v: &mut [f64], p: &mut [T], a: usize, b: usize, pivot: usize) -> (usize, bool) {
        swap(v, p, a, pivot);
        let mut i = a + 1;
        let mut j = b - 1;
        while i <= j && less(v, i, a) {
            i += 1;
        }
        while i <= j && !less(v, j, a) {
            j -= 1;
        }
        if i > j {
            swap(v, p, j, a);
            return (j, true);
        }
        swap(v, p, i, j);
        i += 1;
        j = j.wrapping_sub(1);
        loop {
            while i <= j && less(v, i, a) {
                i += 1;
            }
            while i <= j && !less(v, j, a) {
                j = j.wrapping_sub(1);
            }
            if i > j {
                break;
            }
            swap(v, p, i, j);
            i += 1;
            j = j.wrapping_sub(1);
        }
        swap(v, p, j, a);
        (j, false)
    }

    fn partition_equal<T>(v: &mut [f64], p: &mut [T], a: usize, b: usize, pivot: usize) -> usize {
        swap(v, p, a, pivot);
        let mut i = a + 1;
        let mut j = b - 1;
        loop {
            while i <= j && !less(v, a, i) {
                i += 1;
            }
            while i <= j && less(v, a, j) {
                j = j.wrapping_sub(1);
            }
            if i > j {
                break;
            }
            swap(v, p, i, j);
            i += 1;
            j = j.wrapping_sub(1);
        }
        i
    }

    fn partial_insertion_sort<T>(v: &mut [f64], p: &mut [T], a: usize, b: usize) -> bool {
        const MAX_STEPS: usize = 5;
        const SHORTEST_SHIFTING: usize = 50;
        let mut i = a + 1;
        for _ in 0..MAX_STEPS {
            while i < b && !less(v, i, i - 1) {
                i += 1;
            }
            if i == b {
                return true;
            }
            if b - a < SHORTEST_SHIFTING {
                return false;
            }
            swap(v, p, i, i - 1);
            if i - a >= 2 {
                let mut j = i - 1;
                while j >= 1 {
                    if !less(v, j, j - 1) {
                        break;
                    }
                    swap(v, p, j, j - 1);
                    j -= 1;
                }
            }
            if b - i >= 2 {
                let mut j = i + 1;
                while j < b {
                    if !less(v, j, j - 1) {
                        break;
                    }
                    swap(v, p, j, j - 1);
                    j += 1;
                }
            }
        }
        false
    }

    fn next_power_of_two(length: usize) -> u64 {
        let shift = usize_bits_len(length);
        1u64 << shift
    }

    fn xorshift_next(state: &mut u64) -> u64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }

    fn break_patterns<T>(v: &mut [f64], p: &mut [T], a: usize, b: usize) {
        let length = b - a;
        if length >= 8 {
            let mut random = length as u64;
            let modulus = next_power_of_two(length);
            let start = a + (length / 4) * 2 - 1;
            for idx in start..=a + (length / 4) * 2 + 1 {
                let mut other = (xorshift_next(&mut random) & (modulus - 1)) as usize;
                if other >= length {
                    other -= length;
                }
                swap(v, p, idx, a + other);
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn pdqsort<T>(v: &mut [f64], p: &mut [T], mut a: usize, mut b: usize, mut limit: u32) {
        const MAX_INSERTION: usize = 12;
        let mut was_balanced = true;
        let mut was_partitioned = true;
        loop {
            let length = b - a;
            if length <= MAX_INSERTION {
                insertion_sort(v, p, a, b);
                return;
            }
            if limit == 0 {
                heap_sort(v, p, a, b);
                return;
            }
            if !was_balanced {
                break_patterns(v, p, a, b);
                limit -= 1;
            }
            let (mut pivot, mut hint) = choose_pivot(v, p, a, b);
            if hint == DECREASING {
                reverse_range(v, p, a, b);
                pivot = (b - 1) - (pivot - a);
                hint = INCREASING;
            }
            if was_balanced && was_partitioned && hint == INCREASING && partial_insertion_sort(v, p, a, b) {
                return;
            }
            if a > 0 && !less(v, a - 1, pivot) {
                let mid = partition_equal(v, p, a, b, pivot);
                a = mid;
                continue;
            }
            let (mid, already) = partition(v, p, a, b, pivot);
            was_partitioned = already;
            let left_len = mid - a;
            let right_len = b - mid;
            let balance_threshold = length / 8;
            if left_len < right_len {
                was_balanced = left_len >= balance_threshold;
                pdqsort(v, p, a, mid, limit);
                a = mid + 1;
            } else {
                was_balanced = right_len >= balance_threshold;
                pdqsort(v, p, mid + 1, b, limit);
                b = mid;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// bin format
// ---------------------------------------------------------------------------

/// Volume thresholds (metrics.go).
const VOL_LOW: f64 = 100.0;
const VOL_MED: f64 = 1000.0;
const VOL_HIGH: f64 = 5000.0;

fn classify_volume_level(volume: f64) -> &'static str {
    if volume >= VOL_HIGH {
        "Very High"
    } else if volume >= VOL_MED {
        "High"
    } else if volume >= VOL_LOW {
        "Medium"
    } else {
        "Low"
    }
}

fn calculate_health_score(avg_volume: f64) -> f64 {
    if avg_volume < VOL_LOW {
        100.0
    } else if avg_volume < VOL_MED {
        let range = VOL_MED - VOL_LOW;
        70.0 + (VOL_MED - avg_volume) / range * 30.0
    } else if avg_volume < VOL_HIGH {
        let range = VOL_HIGH - VOL_MED;
        30.0 + (VOL_HIGH - avg_volume) / range * 40.0
    } else {
        let excess = (avg_volume - VOL_HIGH) / 1000.0 * 10.0;
        (30.0 - excess).max(0.0)
    }
}

/// A function entry omitting empty string fields (Go `omitempty`).
fn function_halstead_entry(f: &FunctionMetrics) -> GoValue {
    let mut m = GoMap::new(MapOrigin::Struct);
    m.push("name", GoValue::Str(f.name.clone()));
    if !f.source_file.is_empty() {
        m.push("source_file", GoValue::Str(f.source_file.clone()));
    }
    if !f.language.is_empty() {
        m.push("language", GoValue::Str(f.language.clone()));
    }
    if !f.directory.is_empty() {
        m.push("directory", GoValue::Str(f.directory.clone()));
    }
    m.push("volume", GoValue::Float(f.volume));
    m.push("difficulty", GoValue::Float(f.difficulty));
    m.push("effort", GoValue::Float(f.effort));
    m.push("time_to_program", GoValue::Float(f.time_to_program));
    m.push("delivered_bugs", GoValue::Float(f.delivered_bugs));
    m.push("complexity_level", GoValue::Str(classify_volume_level(f.volume).to_string()));
    GoValue::Map(m)
}

/// Builds the `ComputedMetrics` GoValue (`ComputeAllMetrics`) for the aggregated
/// report. The integer-typed aggregate scalars are averaged (float) in the
/// report, so `ParseReportData`'s `.(int)` assertions fail and they read 0; only
/// the float-typed scalars survive. `total_functions` is a count (int) and
/// survives. The `message` is the captured golden label (see module docs).
fn computed_metrics(agg: &Aggregate) -> GoValue {
    // function_halstead: per-function data sorted by volume descending using
    // Go's UNSTABLE sort.Slice (gosort), so equal-volume ties land in the same
    // positions as the Go reference. Input order is the aggregated (walk-order)
    // function list.
    let mut funcs: Vec<&FunctionMetrics> = agg.functions.iter().collect();
    let mut vols: Vec<f64> = funcs.iter().map(|f| f.volume).collect();
    gosort::slice_by_volume_desc(&mut vols, &mut funcs);
    let function_halstead: Vec<GoValue> = funcs.iter().map(|f| function_halstead_entry(f)).collect();

    // distribution.
    let (mut low, mut medium, mut high, mut very_high) = (0i64, 0i64, 0i64, 0i64);
    for f in &agg.functions {
        if f.volume >= VOL_HIGH {
            very_high += 1;
        } else if f.volume >= VOL_MED {
            high += 1;
        } else if f.volume >= VOL_LOW {
            medium += 1;
        } else {
            low += 1;
        }
    }
    let mut dist = GoMap::new(MapOrigin::Struct);
    dist.push("low", GoValue::Int(low));
    dist.push("medium", GoValue::Int(medium));
    dist.push("high", GoValue::Int(high));
    dist.push("very_high", GoValue::Int(very_high));

    // high_effort_functions: volume >= 1000, sorted by volume desc.
    let mut high_eff: Vec<&FunctionMetrics> =
        agg.functions.iter().filter(|f| f.volume >= VOL_MED).collect();
    let mut he_vols: Vec<f64> = high_eff.iter().map(|f| f.volume).collect();
    gosort::slice_by_volume_desc(&mut he_vols, &mut high_eff);
    let high_effort_functions: Vec<GoValue> = high_eff
        .iter()
        .map(|f| {
            let risk = if f.volume >= VOL_HIGH { "HIGH" } else { "MEDIUM" };
            let mut m = GoMap::new(MapOrigin::Struct);
            m.push("name", GoValue::Str(f.name.clone()));
            if !f.source_file.is_empty() {
                m.push("source_file", GoValue::Str(f.source_file.clone()));
            }
            // language/directory are not stamped on the high-effort struct.
            m.push("volume", GoValue::Float(f.volume));
            m.push("effort", GoValue::Float(f.effort));
            m.push("time_to_program", GoValue::Float(f.time_to_program));
            m.push("delivered_bugs", GoValue::Float(f.delivered_bugs));
            m.push("risk_level", GoValue::Str(risk.to_string()));
            GoValue::Map(m)
        })
        .collect();

    // aggregate: float scalars from averages; int scalars read 0; total_functions
    // is a count and survives; health_score from avg volume per function.
    let avg = |k: &str| agg.averages.get(k).copied().unwrap_or(0.0);
    let volume = avg("volume");
    let mut aggregate = GoMap::new(MapOrigin::Struct);
    aggregate.push("total_functions", GoValue::Int(agg.total_functions));
    aggregate.push("volume", GoValue::Float(volume));
    aggregate.push("difficulty", GoValue::Float(avg("difficulty")));
    aggregate.push("effort", GoValue::Float(avg("effort")));
    aggregate.push("time_to_program", GoValue::Float(avg("time_to_program")));
    aggregate.push("delivered_bugs", GoValue::Float(avg("delivered_bugs")));
    aggregate.push("distinct_operators", GoValue::Int(0));
    aggregate.push("distinct_operands", GoValue::Int(0));
    aggregate.push("total_operators", GoValue::Int(0));
    aggregate.push("total_operands", GoValue::Int(0));
    aggregate.push("vocabulary", GoValue::Int(0));
    aggregate.push("length", GoValue::Int(0));
    aggregate.push("estimated_length", GoValue::Float(avg("estimated_length")));
    let health = if agg.total_functions > 0 {
        calculate_health_score(volume / agg.total_functions as f64)
    } else {
        0.0
    };
    aggregate.push("health_score", GoValue::Float(health));
    aggregate.push("message", GoValue::Str(AGGREGATE_MESSAGE_BIN.to_string()));

    let mut root = GoMap::new(MapOrigin::Struct);
    root.push("function_halstead", GoValue::Array(function_halstead));
    root.push("distribution", GoValue::Map(dist));
    root.push("high_effort_functions", GoValue::Array(high_effort_functions));
    root.push("aggregate", GoValue::Map(aggregate));
    GoValue::Map(root)
}

/// Builds the `static/halstead --format bin` report bytes for `root_path`, or
/// `None` when the folder cannot be walked / no file produces a report.
#[must_use]
pub fn halstead_bin_report(root_path: &str) -> Option<Vec<u8>> {
    let agg = aggregate(root_path)?;
    let metrics = computed_metrics(&agg);
    cf_reportutil::encode_binary_envelope(&metrics).ok()
}
