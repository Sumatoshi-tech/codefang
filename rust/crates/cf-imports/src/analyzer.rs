//! Static import analyzer.
//!
//! Port of `internal/analyzers/imports/analyzer.go`. The static [`Analyzer`]
//! provides the `imports` report key (plus a `count`): it walks a parsed UAST
//! tree, extracts import paths from import nodes, deduplicates them, and applies
//! per-language cleanup heuristics ([`clean_import_path`] / [`parse_import_format`]
//! for Python/JavaScript/Go import-statement shapes).

use crate::metrics::{compute_all_metrics, ComputedMetrics};
use crate::node::{self, Node};
use crate::report::ReportValue;

/// Configuration key for the max dependency-risk rows shown in plots.
///
/// Mirrors Go `ConfigImportsMaxDependencyRiskRows`.
pub const CONFIG_IMPORTS_MAX_DEPENDENCY_RISK_ROWS: &str = "Imports.MaxDependencyRiskRows";

/// Minimum field count for the Python/JS import-format split heuristics.
///
/// Mirrors the Go `lenArg2`/`magic2*` constants (all equal to 2).
const LEN_ARG2: usize = 2;

/// Static import analyzer.
///
/// Mirrors Go `Analyzer`. `cfg_max_dependency_risk_rows` of 0 means "use the
/// default" (the field only affects plot output).
#[derive(Debug, Default, Clone)]
pub struct Analyzer {
    /// Override for the default max dependency-risk rows; 0 = use default.
    pub cfg_max_dependency_risk_rows: i64,
}

impl Analyzer {
    /// Creates a new analyzer. Mirrors Go `NewAnalyzer`.
    pub fn new() -> Self {
        Analyzer::default()
    }

    /// Returns the analyzer name. Mirrors Go `(*Analyzer).Name`.
    pub fn name(&self) -> &'static str {
        "imports"
    }

    /// Returns the CLI flag. Mirrors Go `(*Analyzer).Flag`.
    pub fn flag(&self) -> &'static str {
        "imports-analysis"
    }

    /// Returns the analyzer description. Mirrors Go `(*Analyzer).Description`.
    pub fn description(&self) -> &'static str {
        "Extracts and analyzes import statements from code"
    }

    /// Configures the analyzer from a facts map.
    ///
    /// Mirrors Go `(*Analyzer).Configure`: reads
    /// [`CONFIG_IMPORTS_MAX_DEPENDENCY_RISK_ROWS`] as an int when present.
    ///
    /// # Errors
    /// Never fails; the `Result` matches the Go signature.
    pub fn configure(
        &mut self,
        facts: &std::collections::BTreeMap<String, ReportValue>,
    ) -> Result<(), std::convert::Infallible> {
        if let Some(ReportValue::Int(v)) = facts.get(CONFIG_IMPORTS_MAX_DEPENDENCY_RISK_ROWS) {
            self.cfg_max_dependency_risk_rows = *v;
        }
        Ok(())
    }

    /// Runs static analysis on a parsed UAST root.
    ///
    /// Mirrors Go `(*Analyzer).Analyze`, returning a report with `imports`
    /// (the deduplicated import list, in first-seen order) and `count`.
    ///
    /// # Errors
    /// Never fails; the `Result` matches the Go signature.
    pub fn analyze(&self, root: &Node) -> Result<ReportValue, std::convert::Infallible> {
        let imports = extract_imports_from_uast(root);
        let count = imports.len() as i64;
        let mut report = ReportValue::map();
        report.insert(
            "imports",
            ReportValue::List(imports.into_iter().map(ReportValue::Str).collect()),
        );
        report.insert("count", ReportValue::Int(count));
        Ok(report)
    }

    /// Computes [`ComputedMetrics`] for a report (JSON/YAML/bin payload).
    ///
    /// Mirrors the shared `ComputeAllMetrics(report)` call used by the Go
    /// `FormatReportJSON`/`YAML`/`Binary` methods, with the same
    /// "fall back to empty metrics on error" behaviour.
    pub fn compute_metrics(&self, report: &ReportValue) -> ComputedMetrics {
        compute_all_metrics(report).unwrap_or_default()
    }

    /// Renders the JSON form of a report (`json.MarshalIndent` of
    /// [`ComputedMetrics`]).
    ///
    /// Mirrors Go `(*Analyzer).FormatReportJSON`, which uses
    /// `json.MarshalIndent(metrics, "", "  ")`. Routes through `cf-gojson`'s
    /// `marshal_indent` over the struct-origin [`ComputedMetrics::to_go_value`]
    /// so field order, the `dependencies`-nil-slice `null`, and float formatting
    /// are byte-identical (DESIGN §2.3). (The history-run path serializes the
    /// same value with compact `cf_gojson::marshal`.)
    pub fn format_report_json(&self, report: &ReportValue) -> String {
        let bytes = cf_gojson::marshal_indent(&self.compute_metrics(report).to_go_value());
        String::from_utf8(bytes).expect("cf-gojson emits valid UTF-8")
    }

    /// Encodes a report as a CFB1 `bin` envelope.
    ///
    /// Mirrors Go `(*Analyzer).FormatReportBinary` (which wraps the metrics in
    /// `reportutil.EncodeBinaryEnvelope`). The envelope payload is compact
    /// `json.Marshal` of the struct-origin metrics value, via cf-reportutil.
    pub fn format_report_binary(&self, report: &ReportValue, out: &mut Vec<u8>) {
        let value = self.compute_metrics(report).to_go_value();
        let envelope = cf_reportutil::encode_binary_envelope(&value)
            .expect("imports metrics never exceed the CFB1 length cap");
        out.extend_from_slice(&envelope);
    }
}

/// Extracts deduplicated import strings from a UAST tree.
///
/// Mirrors Go `extractImportsFromUAST`. Pre-order traversal; a node contributes
/// when its type is [`node::uast::IMPORT`] or it has the [`node::role::IMPORT`]
/// role. Duplicates (by extracted path) are dropped, preserving first-seen
/// order — the Go code uses a `seen` map and appends, which is insertion order.
pub fn extract_imports_from_uast(root: &Node) -> Vec<String> {
    let mut imports: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    root.visit_pre_order(&mut |n: &Node| {
        if n.type_ == node::uast::IMPORT || n.has_any_role(node::role::IMPORT) {
            let import_path = extract_import_path(n);
            if !import_path.is_empty() && seen.insert(import_path.clone()) {
                imports.push(import_path);
            }
        }
    });

    imports
}

/// Extracts an import path from an import node.
///
/// Mirrors Go `extractImportPath`: prefer the node token (cleaned); otherwise
/// search children.
fn extract_import_path(import_node: &Node) -> String {
    if !import_node.token.is_empty() {
        return clean_import_path(&import_node.token);
    }
    if import_node.children.is_empty() {
        return String::new();
    }
    extract_import_path_from_children(&import_node.children)
}

/// Searches children for an import path by type priority.
///
/// Mirrors Go `extractImportPathFromChildren`: first a literal with a token,
/// then an identifier with a token, then recursively.
fn extract_import_path_from_children(children: &[Node]) -> String {
    for child in children {
        if child.type_ == node::uast::LITERAL && !child.token.is_empty() {
            return clean_import_path(&child.token);
        }
    }
    for child in children {
        if child.type_ == node::uast::IDENTIFIER && !child.token.is_empty() {
            return clean_import_path(&child.token);
        }
    }
    for child in children {
        let path = extract_import_path(child);
        if !path.is_empty() {
            return path;
        }
    }
    String::new()
}

/// Cleans an import path: strips quotes/semicolons and parses statement forms.
///
/// Mirrors Go `cleanImportPath`. Trims any of `"`, `'`, `;` from both ends,
/// skips empty/`{`/`}` results, then applies [`parse_import_format`]; falls back
/// to the trimmed value.
fn clean_import_path(path: &str) -> String {
    let path = path.trim_matches(|c| c == '"' || c == '\'' || c == ';');

    if path.is_empty() || path == "{" || path == "}" {
        return String::new();
    }

    let parsed = parse_import_format(path);
    if !parsed.is_empty() {
        return parsed;
    }

    path.to_string()
}

/// Extracts a module name from common import-statement formats.
///
/// Mirrors Go `parseImportFormat`, in the same branch order:
///
/// 1. `from X import ...` (Python) -> `X`,
/// 2. `... from '...'` (JS) -> the quoted target,
/// 3. `import X ...` -> `X` (quotes trimmed),
/// 4. `... import ...` -> the part after `import ` (quotes trimmed).
///
/// Returns empty for destructuring/other shapes.
fn parse_import_format(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("from ") {
        // Python: "from typing import List, Dict" -> "typing".
        // Note Go fields() over the whole `path`, so index 1 is the word after "from".
        let _ = rest;
        let parts: Vec<&str> = path.split_whitespace().collect();
        if parts.len() >= LEN_ARG2 {
            return parts[1].to_string();
        }
        return String::new();
    }

    if path.contains(" from ") {
        // JavaScript: "React from 'react'" -> "react".
        let parts: Vec<&str> = path.splitn(2, " from ").collect();
        if parts.len() >= LEN_ARG2 {
            return parts[1].trim_matches(|c| c == '"' || c == '\'').to_string();
        }
        return String::new();
    }

    if path.starts_with("import ") {
        // Python "import os" -> "os"; JS "import './styles.css'" -> "./styles.css".
        let parts: Vec<&str> = path.split_whitespace().collect();
        if parts.len() >= LEN_ARG2 {
            return parts[1].trim_matches(|c| c == '"' || c == '\'').to_string();
        }
        return String::new();
    }

    if path.contains("import ") {
        // JS fallback: "... import './styles.css'" -> "./styles.css".
        let parts: Vec<&str> = path.splitn(2, "import ").collect();
        if parts.len() >= LEN_ARG2 {
            return parts[1].trim_matches(|c| c == '"' || c == '\'').to_string();
        }
        return String::new();
    }

    // JavaScript destructuring "{ useState, useEffect }" -> skip.
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{role, uast};

    // --- ported from analyzer_test.go ---

    #[test]
    fn test_analyzer_analyze() {
        let a = Analyzer::default();
        assert!(!a.name().is_empty());

        // Python: "import os".
        let root = Node::new(uast::IMPORT).with_token("import os");
        let report = a.analyze(&root).unwrap();
        let imports = report
            .as_map()
            .and_then(|m| m.get("imports"))
            .and_then(|v| v.as_list())
            .expect("imports list");
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0], ReportValue::Str("os".to_string()));
    }

    #[test]
    fn test_analyzer_format_json_contains_value() {
        let a = Analyzer::default();
        let mut report = ReportValue::map();
        report.insert(
            "imports",
            ReportValue::List(vec![
                ReportValue::Str("os".to_string()),
                ReportValue::Str("sys".to_string()),
            ]),
        );
        let json = a.format_report_json(&report);
        assert!(json.contains("os"), "expected os in JSON output: {json}");
    }

    #[test]
    fn test_extract_imports_from_uast() {
        // 1. Python "import os".
        let r1 = Node::new(uast::IMPORT).with_token("import os");
        let imps1 = extract_imports_from_uast(&r1);
        assert_eq!(imps1, vec!["os".to_string()]);

        // 2. Python "from x import y".
        let r2 = Node::new(uast::IMPORT).with_token("from x import y");
        let imps2 = extract_imports_from_uast(&r2);
        assert_eq!(imps2, vec!["x".to_string()]);

        // 3. JS "import React from 'react'" -> react.
        let r3 = Node::new(uast::IMPORT).with_token("import React from 'react'");
        let imps3 = extract_imports_from_uast(&r3);
        assert_eq!(imps3, vec!["react".to_string()]);

        // 4. JS "import './styles.css'".
        let r4 = Node::new(uast::IMPORT).with_token("import './styles.css'");
        let imps4 = extract_imports_from_uast(&r4);
        assert_eq!(imps4, vec!["./styles.css".to_string()]);

        // 5. Children traversal (RoleImport, literal child).
        let r5 = Node::new("")
            .with_roles([role::IMPORT])
            .with_children(vec![Node::new(uast::LITERAL).with_token("'module'")]);
        let imps5 = extract_imports_from_uast(&r5);
        assert_eq!(imps5, vec!["module".to_string()]);
    }

    #[test]
    fn test_format_report_binary_envelope() {
        let a = Analyzer::default();
        let mut report = ReportValue::map();
        report.insert(
            "imports",
            ReportValue::List(vec![ReportValue::Str("fmt".to_string())]),
        );
        report.insert("count", ReportValue::Int(1));
        let mut out = Vec::new();
        a.format_report_binary(&report, &mut out);
        assert_eq!(&out[0..4], b"CFB1");
        assert!(out.len() > 8);
    }
}
