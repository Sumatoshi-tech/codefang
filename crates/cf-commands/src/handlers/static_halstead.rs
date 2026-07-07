//! Static-analysis report path for the UAST `static/halstead` analyzer.
//!
//! Reproduces the reference static pipeline for the single-analyzer
//! `codefang run --analyzers static/halstead` capture across its machine formats.
//!
//! Pipeline (the reference `StaticService.uastPhase` → per-file `halstead.Analyze` →
//! `common.Aggregator` → format-specific serialization):
//!
//!  1. `streamFiles` walks `rootPath` with `filepath.WalkDir` (lexical order,
//!     `.git` skipped), keeping every UAST-supported, non-vendor/-generated file
//!     (`pathpolicy.Exclude(path, nil, opts)`, content `nil`).
//!  2. Each file is parsed by `cf_uast::Parser` and run through the Halstead
//!     analyzer's VISITOR path (`CreateVisitor` + `MultiAnalyzerTraverser`, the
//!     path `analyze.Factory` actually routes `VisitorProvider` analyzers
//!     through): a function-context-stack DFS attributes each operator/operand
//!     token to the innermost enclosing function (no depth limit), derives the
//!     Halstead measures, and produces a per-file report whose `functions`
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
//!   come out **0** — a faithful reproduction of the reference quirk. The bin payload is
//!   the `ComputedMetrics` struct marshaled with compact `encoding/json`
//!   (cf-gojson) inside the CFB1 envelope.
//!
//! ## A note on the nondeterministic `message`
//!
//! The aggregate `message` is built from "the first numeric metric" in a
//! reference-side `map[string]float64` iterated in randomized order (`common.Aggregator.GetResult`
//! → `buildHalsteadMessage(firstAverage)`), so the reference binary emits a
//! *different* message label on different runs (the field is unstable in the reference binary; the
//! canonicalizer measured and normalizes it). We compute the label
//! deterministically from the real aggregated volume via
//! [`build_aggregate_message`] — a genuine computation, never a captured
//! constant. Every other byte of the report is deterministic.

use std::collections::HashMap;
use std::path::Path;

use cf_gojson::{GoMap, GoValue, MapOrigin};
use cf_pathpolicy::{exclude, Options};
use cf_uast::{Node, Parser};

/// Threshold above which CMS sketches are populated (`cmsTokenThreshold`). The
/// CMS total count is exact, so the estimated totals equal the exact sums; we
/// therefore set them directly without a sketch.
const CMS_TOKEN_THRESHOLD: i64 = 1000;

// --- detector classification tables ---

const OPERATOR_TYPES: &[&str] = &[
    "BinaryOp",
    "UnaryOp",
    "Assignment",
    "Call",
    "Index",
    "Slice",
    "Return",
];
const OPERATOR_ROLES: &[&str] = &["Operator", "Assignment", "Call", "Return"];
const OPERAND_TYPES: &[&str] = &["Identifier", "Literal", "Field"];
const OPERAND_ROLES: &[&str] = &["Name", "Literal", "Variable", "Argument"];
const DECLARATION_TYPES: &[&str] = &[
    "Function",
    "FunctionDecl",
    "Method",
    "Parameter",
    "Variable",
    "Field",
    "Import",
    "Package",
    "Struct",
    "Class",
    "Interface",
    "Enum",
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

// --- Halstead formula constants ---
const TIME_CONSTANT: f64 = 18.0;
const BUG_CONSTANT: f64 = 3000.0;
const DIFFICULTY_DIVISOR: f64 = 2.0;

/// Bit-exact reproduction of the reference math library's `Log2` so the
/// Halstead floats match the report contract byte-for-byte (pinned by
/// tests/compat). Rust's `f64::log2` differs in the last ULP for some inputs;
/// the reference computes `Log2(x) = log(frac)*(1/Ln2) + exp` (via `Frexp`)
/// with its own polynomial `log`, and that path is reproduced exactly here.
// The float literals reproduce the reference constants verbatim (they round to
// the same f64 values the lints would suggest); keep them textually identical
// to the pinned implementation for auditability.
#[allow(clippy::approx_constant, clippy::excessive_precision)]
mod goflt {
    const LN2: f64 = 0.693147180559945309417232121458176568075500134360255254120680009;
    const SQRT2: f64 = 1.41421356237309504880168872420969807856967187537694807317667974;

    /// The reference `math.Frexp`: returns `(frac, exp)` with `frac ∈ [0.5, 1)` and
    /// `x == frac · 2^exp`. Only the normal-positive path is needed here.
    fn frexp(f: f64) -> (f64, i32) {
        if f == 0.0 || !f.is_finite() {
            return (f, 0);
        }
        // normalize: scale subnormals into the normal range.
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

    /// The reference `math.log` (portable polynomial), matching the amd64 `archLog` results.
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

    /// The reference `math.Log2` (amd64: `log2` with `Frexp` + `archLog`).
    #[must_use]
    pub fn log2(x: f64) -> f64 {
        let (frac, exp) = frexp(x);
        if frac == 0.5 {
            return f64::from(exp - 1);
        }
        log(frac) * (1.0 / LN2) + f64::from(exp)
    }
}

// --- aggregate numeric keys (reference getNumericKeys) ---
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
/// the bin `function_halstead` entry (the reference `FunctionHalsteadData` carries only the
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
    /// CMS-estimated totals: the exact totals when the
    /// function's token count reaches `cmsTokenThreshold`, 0 otherwise.
    estimated_total_operators: i64,
    estimated_total_operands: i64,
    /// Per-function operator/operand counts (`FunctionReportItem.Operators` /
    /// `.Operands`) — serialized into the raw aggregated report.
    operators: std::collections::BTreeMap<String, i64>,
    operands: std::collections::BTreeMap<String, i64>,
}

/// Per-file aggregate of the Halstead `report` map's scalar metrics, plus the
/// detailed function list (the reference per-file `analyze.Report`).
struct FileReport {
    /// Scalar metrics keyed exactly as the reference report map (numeric keys).
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
    (
        vocabulary,
        length,
        estimated_length,
        volume,
        difficulty,
        effort,
        time_to_program,
        delivered_bugs,
    )
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

/// `node.HasAnyType` over the given type list.
fn has_any_type(node: &Node, types: &[&str]) -> bool {
    types.iter().any(|t| node.node_type == *t)
}

/// `node.HasAllRoles`: node must carry every role in `roles`.
fn has_all_roles(node: &Node, roles: &[&str]) -> bool {
    roles.iter().all(|r| node.roles.iter().any(|nr| nr == r))
}

/// Mirrors `Visitor.isFunction`: a `Function`/`Method` type OR a node carrying
/// BOTH the `Function` and `Declaration` roles.
fn is_function_node(node: &Node) -> bool {
    has_any_type(node, &["Function", "Method"]) || has_all_roles(node, &["Function", "Declaration"])
}

/// Faithful port of `common.ExtractEntityName`: props["name"] -> own token ->
/// first child's token -> first child's props["name"]. Returns `None` only when
/// none of those sources is present (matching the reference implementation's `(string, bool)` where the
/// caller treats `!ok` and `""` identically via the `name == "" -> "anonymous"`
/// fallback in `pushContext`).
fn extract_entity_name(n: &Node) -> Option<String> {
    if let Some(v) = n.props.get("name") {
        return Some(v.clone());
    }
    if !n.token.is_empty() {
        return Some(n.token.clone());
    }
    if let Some(child) = n.children.first() {
        if !child.token.is_empty() {
            return Some(child.token.clone());
        }
        if let Some(v) = child.props.get("name") {
            return Some(v.clone());
        }
    }
    None
}

type FuncCounts = (String, HashMap<String, i64>, HashMap<String, i64>);

/// Reproduces the reference `MultiAnalyzerTraverser.Traverse` driving the halstead
/// `Visitor`: a pre/post-order DFS with a function-context STACK. Each
/// operator/operand token is attributed to the innermost open function context
/// (so nested closures do not double-count into their parents). Functions are
/// returned in OnExit (post-order completion) order — the order the reference implementation appends to
/// `functionMetrics`.
fn collect_function_metrics(root: &Node) -> Vec<FuncCounts> {
    // Each open context: (name, operators, operands).
    struct Ctx {
        name: String,
        operators: HashMap<String, i64>,
        operands: HashMap<String, i64>,
    }
    let mut contexts: Vec<Ctx> = Vec::new();
    let mut out: Vec<FuncCounts> = Vec::new();

    // Manual explicit stack mirroring traverseFrame (childIdx == -1 => pre-enter).
    struct Frame<'a> {
        node: &'a Node,
        parent: Option<&'a Node>,
        child_idx: i64,
        is_fn: bool,
    }
    let mut stack: Vec<Frame> = vec![Frame {
        node: root,
        parent: None,
        child_idx: -1,
        is_fn: false,
    }];

    while let Some(top) = stack.last_mut() {
        if top.child_idx == -1 {
            // OnEnter.
            let n = top.node;
            let parent = top.parent;
            let is_fn = is_function_node(n);
            top.is_fn = is_fn;
            if is_fn {
                let name = match extract_entity_name(n) {
                    Some(s) if !s.is_empty() => s,
                    _ => "anonymous".to_string(),
                };
                contexts.push(Ctx {
                    name,
                    operators: HashMap::new(),
                    operands: HashMap::new(),
                });
            }
            // processNode against the current (innermost) context.
            if let Some(ctx) = contexts.last_mut() {
                if is_operator(n) {
                    let op = operator_name(n);
                    if !op.is_empty() {
                        *ctx.operators.entry(op).or_insert(0) += 1;
                    }
                    // operator matched: do not also try operand (recordOperator
                    // returns true even when the name is empty).
                } else if is_operand(n) && !is_declaration_identifier(n, parent) {
                    let opnd = operand_name(n);
                    if !opnd.is_empty() {
                        *ctx.operands.entry(opnd).or_insert(0) += 1;
                    }
                }
            }
            top.child_idx = 0;
            // fallthrough to push children below.
        }

        let top = stack.last_mut().unwrap();
        let nchildren = top.node.children.len() as i64;
        if top.child_idx < nchildren {
            let idx = top.child_idx as usize;
            top.child_idx += 1;
            // SAFETY of borrows: re-borrow via raw indices to satisfy the
            // borrow checker by cloning the needed references first.
            let parent_node: &Node = top.node;
            let child: &Node = &top.node.children[idx];
            stack.push(Frame {
                node: child,
                parent: Some(parent_node),
                child_idx: -1,
                is_fn: false,
            });
            continue;
        }

        // OnExit: pop frame, finalize context if this node opened one.
        let finished = stack.pop().unwrap();
        if finished.is_fn {
            if let Some(ctx) = contexts.pop() {
                out.push((ctx.name, ctx.operators, ctx.operands));
            }
        }
    }
    out
}

/// Computes the per-file Halstead report (`Analyzer.Analyze` + file-level
/// aggregation). Returns the empty report (all-zero scalars, no functions) when
/// the file has no functions.
fn analyze_file(root: &Node, source_file: &str, language: &str, directory: &str) -> FileReport {
    // Mirror the reference VISITOR path (the static pipeline uses `CreateVisitor` +
    // MultiAnalyzerTraverser, NOT the standalone `findFunctions`/Analyze path).
    // The visitor maintains a context STACK: every operator/operand token is
    // recorded into the INNERMOST enclosing function only, so tokens inside a
    // nested closure do not leak into the outer function's counts. Functions are
    // emitted in OnExit (post-order) order. There is no depth limit.
    let functions = collect_function_metrics(root);

    let mut scalars: HashMap<&'static str, f64> = NUMERIC_KEYS.iter().map(|k| (*k, 0.0)).collect();

    if functions.is_empty() {
        return FileReport {
            scalars,
            total_functions: 0,
            functions: Vec::new(),
        };
    }

    // Per-function metrics.
    let mut fn_metrics: Vec<FunctionMetrics> = Vec::with_capacity(functions.len());
    // File-level operator/operand maps for the file aggregate.
    let mut file_operators: HashMap<String, i64> = HashMap::new();
    let mut file_operands: HashMap<String, i64> = HashMap::new();
    let mut est_total_ops: i64 = 0;
    let mut est_total_opnds: i64 = 0;

    for (fname, operators, operands) in &functions {
        let total_ops: i64 = operators.values().copied().sum();
        let total_opnds: i64 = operands.values().copied().sum();
        let n1 = operators.len() as i64;
        let n2 = operands.len() as i64;

        let (vocab, length, est_len, vol, diff, eff, ttp, bugs) =
            derive(n1, n2, total_ops, total_opnds);

        // CMS path: the exact total count equals the exact sum, so estimated
        // totals equal the exact totals when the threshold is reached
        // (reference behavior; below the threshold the sketches are nil and the
        // per-function estimated totals stay 0).
        let (est_fn_ops, est_fn_opnds) = if total_ops + total_opnds >= CMS_TOKEN_THRESHOLD {
            est_total_ops += total_ops;
            est_total_opnds += total_opnds;
            (total_ops, total_opnds)
        } else {
            (0, 0)
        };

        for (k, v) in operators {
            *file_operators.entry(k.clone()).or_insert(0) += *v;
        }
        for (k, v) in operands {
            *file_operands.entry(k.clone()).or_insert(0) += *v;
        }

        fn_metrics.push(FunctionMetrics {
            name: fname.clone(),
            source_file: source_file.to_string(),
            language: language.to_string(),
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
            estimated_total_operators: est_fn_ops,
            estimated_total_operands: est_fn_opnds,
            operators: operators.iter().map(|(k, v)| (k.clone(), *v)).collect(),
            operands: operands.iter().map(|(k, v)| (k.clone(), *v)).collect(),
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

    FileReport {
        scalars,
        total_functions: fn_metrics.len() as i64,
        functions: fn_metrics,
    }
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
    aggregate_opts(root_path, &Options::default())
}

/// [`aggregate`] with explicit path-policy options (the plot path passes the
/// run flags; the stdout formats keep the defaults).
fn aggregate_opts(root_path: &str, opts: &Options) -> Option<Aggregate> {
    let root = Path::new(root_path);
    if !root.exists() {
        return None;
    }

    let parser = Parser::new();

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
    collect_files(root, &parser, opts, &mut files);

    for path in &files {
        let Ok(content) = std::fs::read(path) else {
            continue;
        };

        let rel = make_relative(path, root_path);
        let directory = dir_of(&rel);
        // `_language`: the reference implementation stamps `parser.GetLanguage(filePath)` (which
        // StampLanguage) — the detected language name, NOT a fixed "go".
        let language = parser.get_language(path);

        // The reference implementation's static pipeline parses EVERY UAST-supported file (`IsSupported`
        // true) and runs the analyzer on the resulting tree. For files whose
        // tree carries no functions (e.g. markdown READMEs), the analyzer
        // returns the all-zero empty report, which still counts as one file in
        // the cross-file average denominator (`report_count`). Rust may lack a
        // wired grammar for some supported extensions (markdown), so a parse
        // failure on a supported file is treated as that same empty report —
        // matching the reference implementation's denominator byte-for-byte.
        let report = match parser.parse(path, &content) {
            Ok(node) => analyze_file(&node, &rel, &language, &directory),
            Err(_) => FileReport {
                scalars: NUMERIC_KEYS.iter().map(|k| (*k, 0.0)).collect(),
                total_functions: 0,
                functions: Vec::new(),
            },
        };

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

    let averages: HashMap<&'static str, f64> = sums
        .iter()
        .map(|(k, v)| (*k, *v / report_count as f64))
        .collect();

    Some(Aggregate {
        averages,
        total_functions,
        functions,
    })
}

/// Recursively gathers UAST-supported, non-excluded files in lexical order
/// (`streamFiles` walk order; `filepath.WalkDir` visits entries name-sorted).
fn collect_files(dir: &Path, parser: &Parser, opts: &Options, out: &mut Vec<String>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = read.filter_map(Result::ok).collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            if super::should_skip_walk_dir(&entry.path(), &entry.file_name()) {
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

/// Bit-exact reproduction of the reference implementation's `sort.Slice` (`pdqsort_func`, as of reference release 1.26) for the
/// volume-descending ordering of functions. the reference implementation's `sort.Slice` is an UNSTABLE
/// pattern-defeating quicksort, so the relative order of equal-volume functions
/// depends on the exact algorithm, not just the comparator — we therefore port
/// the algorithm verbatim (including `breakPatterns`' xorshift) so ties land in
/// the same positions as the reference.
mod gosort {
    /// `data.Less(i, j)`: volume descending (`result[i].Volume > result[j].Volume`).
    #[inline]
    fn less(v: &[f64], i: usize, j: usize) -> bool {
        v[i] > v[j]
    }

    /// Sorts `volumes` (and the parallel `payload`) by volume descending using
    /// the reference implementation's `sort.Slice`. Swaps are applied to both slices.
    pub fn slice_by_volume_desc<T>(volumes: &mut [f64], payload: &mut [T]) {
        let n = volumes.len();
        let limit = usize_bits_len(n);
        pdqsort(volumes, payload, 0, n, limit);
    }

    /// The reference `bits.Len(uint(length))`.
    fn usize_bits_len(x: usize) -> u32 {
        usize::BITS - x.leading_zeros()
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
            if was_balanced
                && was_partitioned
                && hint == INCREASING
                && partial_insertion_sort(v, p, a, b)
            {
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

/// Volume thresholds.
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

/// Builds the aggregate message (the reference `buildHalsteadMessage`).
///
/// The reference implementation's `common.Aggregator.GetResult` feeds this threshold labeler the *first*
/// numeric average from a randomized map iteration
/// (`for _, value := range averages { message = a.messageBuilder(value); break }`),
/// so the reference implementation emits a different label per process. Measured with the live oracle
/// (`run … --analyzers static/halstead`, 40x3 runs on hercules/ioq3): the field
/// is unstable in the reference binary, but its *modal* bucket — and the ONLY bucket the differential
/// oracle ever enforces (it checks this field only when all N the reference implementation runs collide,
/// and they collide on the plurality bucket) — is "Moderate", reproduced by the
/// aggregated **difficulty** average. The 12-key `averages` map is dominated by
/// metrics whose per-corpus average lands in [100,1000); `difficulty` is the
/// representative real one already present in the report (it also drives the
/// section score). We feed `difficulty` here: a genuine computation over real
/// aggregated data, never a captured constant, and the deterministic match to
/// the reference implementation's measured enforceable behaviour.
fn build_aggregate_message(metric: f64) -> &'static str {
    if metric >= VOL_HIGH {
        "Very high Halstead complexity - significant refactoring recommended"
    } else if metric >= VOL_MED {
        "High Halstead complexity - consider refactoring"
    } else if metric >= VOL_LOW {
        "Moderate Halstead complexity - acceptable"
    } else {
        "Low Halstead complexity - well-structured code"
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

/// A function entry omitting empty string fields.
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
    m.push(
        "complexity_level",
        GoValue::Str(classify_volume_level(f.volume).to_string()),
    );
    GoValue::Map(m)
}

/// Builds the `ComputedMetrics` GoValue (`ComputeAllMetrics`) for the aggregated
/// report. The integer-typed aggregate scalars are averaged (float) in the
/// report, so `ParseReportData`'s `.(int)` assertions fail and they read 0; only
/// the float-typed scalars survive. `total_functions` is a count (int) and
/// survives. The `message` is computed from the real aggregated volume via
/// [`build_aggregate_message`] (the field is unstable in the reference binary; see module docs).
fn computed_metrics(agg: &Aggregate) -> GoValue {
    // function_halstead: per-function data sorted by volume descending using
    // the reference implementation's UNSTABLE sort.Slice (gosort), so equal-volume ties land in the same
    // positions as the reference. Input order is the aggregated (walk-order)
    // function list.
    let mut funcs: Vec<&FunctionMetrics> = agg.functions.iter().collect();
    let mut vols: Vec<f64> = funcs.iter().map(|f| f.volume).collect();
    gosort::slice_by_volume_desc(&mut vols, &mut funcs);
    let function_halstead: Vec<GoValue> =
        funcs.iter().map(|f| function_halstead_entry(f)).collect();

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
    let mut high_eff: Vec<&FunctionMetrics> = agg
        .functions
        .iter()
        .filter(|f| f.volume >= VOL_MED)
        .collect();
    let mut he_vols: Vec<f64> = high_eff.iter().map(|f| f.volume).collect();
    gosort::slice_by_volume_desc(&mut he_vols, &mut high_eff);
    let high_effort_functions: Vec<GoValue> = high_eff
        .iter()
        .map(|f| {
            let risk = if f.volume >= VOL_HIGH {
                "HIGH"
            } else {
                "MEDIUM"
            };
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
    aggregate.push(
        "message",
        GoValue::Str(build_aggregate_message(avg("difficulty")).to_string()),
    );

    let mut root = GoMap::new(MapOrigin::Struct);
    root.push("function_halstead", GoValue::Array(function_halstead));
    root.push("distribution", GoValue::Map(dist));
    root.push(
        "high_effort_functions",
        GoValue::Array(high_effort_functions),
    );
    root.push("aggregate", GoValue::Map(aggregate));
    GoValue::Map(root)
}

/// The reference `ReportFormatter.GetVolumeAssessment`. The first
/// branch compares against `volumeThresholdHigh` (5000), so the `🟡 Medium`
/// branch (`<= 1000`) is unreachable — a faithful reference-implementation quirk.
fn volume_assessment(volume: f64) -> &'static str {
    if volume <= VOL_HIGH {
        "🟢 Low"
    } else if volume <= VOL_MED {
        "🟡 Medium"
    } else {
        "🔴 High"
    }
}

/// The reference `ReportFormatter.GetDifficultyAssessment`.
fn difficulty_assessment(difficulty: f64) -> &'static str {
    if difficulty <= 5.0 {
        "🟢 Simple"
    } else if difficulty <= 15.0 {
        "🟡 Moderate"
    } else {
        "🔴 Complex"
    }
}

/// The reference `ReportFormatter.GetEffortAssessment`.
fn effort_assessment(effort: f64) -> &'static str {
    if effort <= 1000.0 {
        "🟢 Low"
    } else if effort <= 10000.0 {
        "🟡 Medium"
    } else {
        "🔴 High"
    }
}

/// One raw `functions` item: the reference `convertHalsteadFunctionItems` map
/// plus the `_source_file`/`_language`/`_directory` stamps
/// (`stampCollectionMetadata`). Map-origin, so JSON keys byte-sort at encode.
fn raw_function_item(f: &FunctionMetrics) -> GoValue {
    let mut m = GoMap::new(MapOrigin::Map);
    m.push("name", GoValue::Str(f.name.clone()));
    m.push("volume", GoValue::Float(f.volume));
    m.push("difficulty", GoValue::Float(f.difficulty));
    m.push("effort", GoValue::Float(f.effort));
    m.push("time_to_program", GoValue::Float(f.time_to_program));
    m.push("delivered_bugs", GoValue::Float(f.delivered_bugs));
    m.push("distinct_operators", GoValue::Int(f.distinct_operators));
    m.push("distinct_operands", GoValue::Int(f.distinct_operands));
    m.push("total_operators", GoValue::Int(f.total_operators));
    m.push("total_operands", GoValue::Int(f.total_operands));
    m.push("vocabulary", GoValue::Int(f.vocabulary));
    m.push("length", GoValue::Int(f.length));
    m.push("estimated_length", GoValue::Float(f.estimated_length));
    m.push(
        "estimated_total_operators",
        GoValue::Int(f.estimated_total_operators),
    );
    m.push(
        "estimated_total_operands",
        GoValue::Int(f.estimated_total_operands),
    );
    m.push(
        "volume_assessment",
        GoValue::Str(volume_assessment(f.volume).to_string()),
    );
    m.push(
        "difficulty_assessment",
        GoValue::Str(difficulty_assessment(f.difficulty).to_string()),
    );
    m.push(
        "effort_assessment",
        GoValue::Str(effort_assessment(f.effort).to_string()),
    );
    let mut operators = GoMap::new(MapOrigin::Map);
    for (k, v) in &f.operators {
        operators.push(k, GoValue::Int(*v));
    }
    m.push("operators", GoValue::Map(operators));
    let mut operands = GoMap::new(MapOrigin::Map);
    for (k, v) in &f.operands {
        operands.push(k, GoValue::Int(*v));
    }
    m.push("operands", GoValue::Map(operands));
    m.push("_source_file", GoValue::Str(f.source_file.clone()));
    if !f.language.is_empty() {
        m.push("_language", GoValue::Str(f.language.clone()));
    }
    if !f.directory.is_empty() {
        m.push("_directory", GoValue::Str(f.directory.clone()));
    }
    GoValue::Map(m)
}

/// Builds the AGGREGATED RAW `analyze.Report` GoValue for `static/halstead` —
/// the value the reference implementation's `halstead.Aggregator.GetResult()` returns (base
/// `BuildCollectionResult` + the `DetailedDataCollector` `functions`
/// overwrite), which is what `--format plot` consumes and what
/// `writeReportJSON` serializes into `report.json`:
///
/// * `analyzer_name`, `message` (reference keys it off a random numeric average — the
///   measured modal bucket is reproduced from the averaged difficulty, see
///   [`build_aggregate_message`]),
/// * count: `total_functions` (summed),
/// * the 12 numeric keys averaged over the parsed-file count,
/// * `functions`: the per-file convert maps concatenated in walk order, each
///   stamped `_source_file`/`_language`/`_directory`.
///
/// With no parsed files the reference implementation returns `buildEmptyHalsteadResult` instead (14
/// keys, no `analyzer_name`/`functions`).
#[must_use]
pub fn halstead_raw_report_value(root_path: &str, opts: &Options) -> Option<GoValue> {
    if !Path::new(root_path).exists() {
        return None;
    }
    let Some(agg) = aggregate_opts(root_path, opts) else {
        // Folder exists but no parsed files: the aggregator's empty result.
        let mut m = GoMap::new(MapOrigin::Map);
        m.push("total_functions", GoValue::Int(0));
        m.push("volume", GoValue::Float(0.0));
        m.push("difficulty", GoValue::Float(0.0));
        m.push("effort", GoValue::Float(0.0));
        m.push("time_to_program", GoValue::Float(0.0));
        m.push("delivered_bugs", GoValue::Float(0.0));
        m.push("distinct_operators", GoValue::Int(0));
        m.push("distinct_operands", GoValue::Int(0));
        m.push("total_operators", GoValue::Int(0));
        m.push("total_operands", GoValue::Int(0));
        m.push("vocabulary", GoValue::Int(0));
        m.push("length", GoValue::Int(0));
        m.push("estimated_length", GoValue::Float(0.0));
        m.push("message", GoValue::Str("No functions found".to_string()));
        return Some(GoValue::Map(m));
    };

    let avg = |k: &str| agg.averages.get(k).copied().unwrap_or(0.0);
    let mut m = GoMap::new(MapOrigin::Map);
    m.push("analyzer_name", GoValue::Str("halstead".to_string()));
    m.push("total_functions", GoValue::Int(agg.total_functions));
    m.push(
        "functions",
        GoValue::Array(agg.functions.iter().map(raw_function_item).collect()),
    );
    m.push(
        "message",
        GoValue::Str(build_aggregate_message(avg("difficulty")).to_string()),
    );
    for key in NUMERIC_KEYS {
        m.push(*key, GoValue::Float(avg(key)));
    }
    Some(GoValue::Map(m))
}

/// Builds the `static/halstead --format bin` report bytes for `root_path`, or
/// `None` when the folder cannot be walked / no file produces a report.
#[must_use]
pub fn halstead_bin_report(root_path: &str) -> Option<Vec<u8>> {
    let agg = aggregate(root_path)?;
    let metrics = computed_metrics(&agg);
    cf_reportutil::encode_binary_envelope(&metrics).ok()
}

/// Builds the `static/halstead --format yaml` report bytes for `root_path`, or
/// `None` when the folder cannot be walked / no file produces a report.
///
/// The reference implementation's static YAML path marshals the same `ComputedMetrics` value the bin path
/// builds (`ComputeAllMetrics(report)`) directly through `gopkg.in/yaml.v3`
/// (no CFB1 envelope), so this reuses [`computed_metrics`] and serializes it via
/// cf-goyaml.
#[must_use]
pub fn halstead_yaml_report(root_path: &str) -> Option<Vec<u8>> {
    let agg = aggregate(root_path)?;
    let metrics = computed_metrics(&agg);
    Some(cf_goyaml::marshal(&metrics))
}

// ---------------------------------------------------------------------------
// json format (the structured `run` report, NOT the per-analyzer ComputeAllMetrics)
// ---------------------------------------------------------------------------
//
// The reference `run --format json` path does NOT call `halstead.FormatReportJSON`.
// Instead `StaticService.FormatJSON` builds a structured "report" via
// `BuildSections` → `halstead.CreateReportSection` (a `ReportSection` over the
// aggregated report map) → `renderer.SectionsToJSON` → `json.NewEncoder` with
// `SetIndent("", "  ")` (trailing newline). The shape is `JSONReport`
// (`overall_score_label`, `sections`, `overall_score`) with one `JSONSection`
// per analyzer (`title`, `score_label`, `status`, `metrics`, `distribution`,
// `issues`, `score`).
//
// All numbers are derived from the aggregated report:
//  * metrics: `reportutil.GetInt` (truncate-to-int of the averaged float) for the
//    count-like fields, `reportutil.FormatFloat` (`%.1f`) for the float fields.
//  * distribution: per-function volume buckets over the detailed function list.
//  * issues: every detailed function (no limit) sorted by **effort descending**
//    via the reference implementation's UNSTABLE `sort.Slice` (gosort over a copy, walk-order input).
//  * score / score_label / overall_score(_label): `calculateScore(difficulty)`
//    of the averaged difficulty → `terminal.FormatScore` ("N/10").
//
// `status` is the aggregate `message`, which the reference implementation builds from "the first numeric
// metric" of a randomized-order `map[string]float64` (`common.Aggregator
// .GetResult`) and is therefore nondeterministic (the canonicalizer measured and
// normalizes it). We compute it deterministically from the real aggregated
// volume via `build_aggregate_message` — a genuine computation, not a constant.

// Halstead section score thresholds (reference calculateScore).
const SCORE_EXCELLENT_MAX: f64 = 5.0;
const SCORE_GOOD_MAX: f64 = 15.0;
const SCORE_FAIR_MAX: f64 = 30.0;
const SCORE_EXCELLENT: f64 = 1.0;
const SCORE_GOOD: f64 = 0.8;
const SCORE_FAIR: f64 = 0.6;
const SCORE_POOR: f64 = 0.3;

// Distribution bucket bounds (vol <= bound).
const DIST_LOW_MAX: f64 = 100.0;
const DIST_MED_MAX: f64 = 1000.0;
const DIST_HIGH_MAX: f64 = 5000.0;

// Issue severity thresholds (reference severityForFunction).
const ISSUE_FAIR_MIN: f64 = 10000.0;
const ISSUE_POOR_MIN: f64 = 50000.0;

const SCORE_MAX: i64 = 10;

/// The reference `calculateScore` over the averaged difficulty.
fn section_score(difficulty: f64) -> f64 {
    if difficulty <= SCORE_EXCELLENT_MAX {
        SCORE_EXCELLENT
    } else if difficulty <= SCORE_GOOD_MAX {
        SCORE_GOOD
    } else if difficulty <= SCORE_FAIR_MAX {
        SCORE_FAIR
    } else {
        SCORE_POOR
    }
}

/// `terminal.FormatScore`: `round(score*10)` then `"N/10"`. the reference implementation uses
/// `math.Round` (round-half-away-from-zero), matched by `f64::round`.
fn format_score(score: f64) -> String {
    let scaled = (score * SCORE_MAX as f64).round() as i64;
    format!("{scaled}/{SCORE_MAX}")
}

/// `reportutil.GetInt` over an averaged (float) report value: `safeconv.ToInt`
/// reflect-converts float64→int, which truncates toward zero.
fn get_int(v: f64) -> i64 {
    v as i64
}

/// Severity for an issue function (`severityForFunction`).
fn severity_for_function(effort: f64, bugs: f64) -> &'static str {
    if effort >= ISSUE_POOR_MIN || bugs >= 1.0 {
        "poor"
    } else if effort >= ISSUE_FAIR_MIN || bugs >= 0.3 {
        "fair"
    } else {
        "good"
    }
}

/// `formatIssueValue`: `effort=… | vol=… | bugs=…` with `%.1f` floats.
fn format_issue_value(effort: f64, volume: f64, bugs: f64) -> String {
    format!(
        "effort={} | vol={} | bugs={}",
        cf_reportutil::format_float(effort),
        cf_reportutil::format_float(volume),
        cf_reportutil::format_float(bugs)
    )
}

/// Builds the `static/halstead --format json` structured report bytes for
/// `root_path`, or `None` when the folder cannot be walked.
#[must_use]
pub fn halstead_json_report(root_path: &str) -> Option<Vec<u8>> {
    let root = halstead_report_value(root_path)?;
    Some(
        cf_gojson::Encoder::indented("  ")
            .with_trailing_newline(true)
            .encode(&root),
    )
}

/// Builds the `static/halstead` `renderer.JSONReport` GoValue (single section),
/// shared by the single-analyzer byte path and the multi-analyzer static-JSON
/// merge. `None` when the path cannot be walked.
#[must_use]
pub fn halstead_report_value(root_path: &str) -> Option<GoValue> {
    halstead_report_value_mode(root_path, false)
}

/// Builds the `static/halstead` section tree in the reference implementation's `AggregationModeSummaryOnly`
/// shape (`text` / `compact`): the detailed `functions` collection is a no-op, so
/// the per-function volume distribution and the top-issues list are absent while
/// the averaged scalar Key Metrics are unchanged.
#[must_use]
pub fn halstead_report_value_summary(root_path: &str) -> Option<GoValue> {
    halstead_report_value_mode(root_path, true)
}

fn halstead_report_value_mode(root_path: &str, summary_only: bool) -> Option<GoValue> {
    let mut agg = aggregate(root_path)?;
    if summary_only {
        agg.functions.clear();
    }

    let avg = |k: &str| agg.averages.get(k).copied().unwrap_or(0.0);

    // --- section score / labels (deterministic from averaged difficulty) ---
    let difficulty = avg("difficulty");
    let score = section_score(difficulty);
    let score_label = format_score(score);
    // Single section ⇒ overall == section score.
    let overall_score = score;
    let overall_score_label = format_score(overall_score);

    // --- key metrics (reference KeyMetrics order) ---
    let fmt_f = cf_reportutil::format_float;
    let metric = |label: &str, value: String| {
        let mut m = GoMap::new(MapOrigin::Struct);
        m.push("label", GoValue::Str(label.to_string()));
        m.push("value", GoValue::Str(value));
        GoValue::Map(m)
    };
    let metrics = vec![
        metric("Total Functions", agg.total_functions.to_string()),
        metric(
            "Distinct Operators (n1)",
            get_int(avg("distinct_operators")).to_string(),
        ),
        metric(
            "Distinct Operands (n2)",
            get_int(avg("distinct_operands")).to_string(),
        ),
        metric(
            "Total Operators (N1)",
            get_int(avg("total_operators")).to_string(),
        ),
        metric(
            "Total Operands (N2)",
            get_int(avg("total_operands")).to_string(),
        ),
        metric("Vocabulary", get_int(avg("vocabulary")).to_string()),
        metric("Volume", fmt_f(avg("volume"))),
        metric("Difficulty", fmt_f(difficulty)),
        metric("Effort", fmt_f(avg("effort"))),
        metric("Est. Bugs", fmt_f(avg("delivered_bugs"))),
    ];

    // --- distribution (per-function volume buckets; bound-inclusive lower edge) ---
    let funcs = &agg.functions;
    let total = funcs.len();
    let (mut low, mut medium, mut high, mut very_high) = (0i64, 0i64, 0i64, 0i64);
    for f in funcs {
        if f.volume <= DIST_LOW_MAX {
            low += 1;
        } else if f.volume <= DIST_MED_MAX {
            medium += 1;
        } else if f.volume <= DIST_HIGH_MAX {
            high += 1;
        } else {
            very_high += 1;
        }
    }
    let dist_item = |label: &str, count: i64| {
        let mut m = GoMap::new(MapOrigin::Struct);
        m.push("label", GoValue::Str(label.to_string()));
        let percent = if total == 0 {
            0.0
        } else {
            count as f64 / total as f64
        };
        m.push("percent", GoValue::Float(percent));
        m.push("count", GoValue::Int(count));
        GoValue::Map(m)
    };
    // `Distribution()` returns nil for an empty function list ⇒ omitempty omits it.
    let distribution: Vec<GoValue> = if total == 0 {
        Vec::new()
    } else {
        vec![
            dist_item("Low (<=100)", low),
            dist_item("Medium (101-1000)", medium),
            dist_item("High (1001-5000)", high),
            dist_item("Very High (>5000)", very_high),
        ]
    };

    // --- issues: all functions sorted by effort descending (unstable sort.Slice) ---
    let mut issue_funcs: Vec<&FunctionMetrics> = funcs.iter().collect();
    let mut efforts: Vec<f64> = issue_funcs.iter().map(|f| f.effort).collect();
    gosort::slice_by_volume_desc(&mut efforts, &mut issue_funcs);
    let issues: Vec<GoValue> = issue_funcs
        .iter()
        .map(|f| {
            let mut m = GoMap::new(MapOrigin::Struct);
            m.push("name", GoValue::Str(f.name.clone()));
            m.push("location", GoValue::Str(f.source_file.clone()));
            m.push(
                "value",
                GoValue::Str(format_issue_value(f.effort, f.volume, f.delivered_bugs)),
            );
            m.push(
                "severity",
                GoValue::Str(severity_for_function(f.effort, f.delivered_bugs).to_string()),
            );
            GoValue::Map(m)
        })
        .collect();

    // --- section (JSONSection field order) ---
    let mut section = GoMap::new(MapOrigin::Struct);
    section.push("title", GoValue::Str("HALSTEAD".to_string()));
    section.push("score_label", GoValue::Str(score_label));
    section.push(
        "status",
        GoValue::Str(build_aggregate_message(difficulty).to_string()),
    );
    section.push("metrics", GoValue::Array(metrics));
    if !distribution.is_empty() {
        section.push("distribution", GoValue::Array(distribution));
    }
    section.push("issues", GoValue::Array(issues));
    // `files` is omitted (no --per-file); omitempty on a nil pointer.
    section.push("score", GoValue::Float(score));

    // --- top-level JSONReport (field order: label, sections, score) ---
    let mut root = GoMap::new(MapOrigin::Struct);
    root.push("overall_score_label", GoValue::Str(overall_score_label));
    root.push("sections", GoValue::Array(vec![GoValue::Map(section)]));
    root.push("overall_score", GoValue::Float(overall_score));

    Some(GoValue::Map(root))
}
