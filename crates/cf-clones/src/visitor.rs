//! The single-pass clone-detection [`Visitor`].
//!
//! During traversal it collects function nodes; [`Visitor::get_report`] then
//! builds their `MinHash` signatures and exports them under the
//! `_func_signatures` report key for the cross-file [`crate::Aggregator`].
//! Detection itself is deferred to the aggregator.

use cf_analyze::{GoMap, GoValue, MapOrigin, Report};
use cf_uast_node::Node;

use crate::analyzer::build_empty_report;
use crate::engine::{build_signature, is_function_node, FuncEntry};
use crate::shingler::Shingler;
use crate::{
    KEY_ANALYZER_NAME, KEY_CLONE_PAIRS, KEY_CLONE_RATIO, KEY_FUNC_SIGNATURES, KEY_MESSAGE,
    KEY_TOTAL_CLONE_PAIRS, KEY_TOTAL_FUNCTIONS, MSG_NO_CLONES, MSG_NO_FUNCTIONS, NUM_HASHES,
};

/// Collects function nodes and exports their signatures.
#[derive(Debug, Clone)]
pub struct Visitor {
    function_count: usize,
    entries: Vec<FuncEntry>,
    shingler: Shingler,
    num_hashes: usize,
}

impl Default for Visitor {
    fn default() -> Self {
        Self::new()
    }
}

impl Visitor {
    /// Creates a new clone-detection visitor with the default parameters.
    #[must_use]
    pub fn new() -> Self {
        Self {
            function_count: 0,
            entries: Vec::new(),
            shingler: Shingler::default(),
            num_hashes: NUM_HASHES,
        }
    }

    /// Visits a node on entry, collecting function nodes and (eagerly) their
    /// signatures.
    ///
    /// Computing the signature here (rather than lazily at report time) is
    /// equivalent because the signature depends only on the node's subtree at
    /// visit time; the set of functions counted and the signatures produced
    /// are the same either way.
    pub fn on_enter(&mut self, n: &Node) {
        if is_function_node(n) {
            self.function_count += 1;
            if let Some(entry) = build_signature(n, &self.shingler, self.num_hashes) {
                self.entries.push(entry);
            }
        }
    }

    /// Number of function nodes seen so far.
    #[must_use]
    pub fn function_count(&self) -> usize {
        self.function_count
    }

    /// Builds the signature-export report consumed by the aggregator. With no
    /// functions it returns the "No functions found" empty report.
    #[must_use]
    pub fn get_report(&self) -> Report {
        if self.function_count == 0 {
            return build_empty_report(MSG_NO_FUNCTIONS);
        }

        build_signature_report(self.function_count, &self.entries)
    }

    /// Returns the collected entries (the in-process equivalent of consuming
    /// `_func_signatures`).
    #[must_use]
    pub fn entries(&self) -> &[FuncEntry] {
        &self.entries
    }

    /// Like [`Visitor::get_report`] but stamps each exported signature item
    /// with `_source_file = source_file`.
    ///
    /// In the folder pipeline the framework sets `_source_file` on every
    /// `_func_signatures` collection item (the repo-relative path) after the
    /// visitor runs and before the aggregator reads it back to qualify
    /// function names. With no functions it returns the "No functions found"
    /// empty report (which carries no signatures to stamp).
    #[must_use]
    pub fn get_report_with_source(&self, source_file: &str) -> Report {
        if self.function_count == 0 {
            return build_empty_report(MSG_NO_FUNCTIONS);
        }
        build_signature_report_with_source(self.function_count, &self.entries, source_file)
    }
}

/// Builds the signature-export report, stamping each `{name, sig}` item with
/// `_source_file`.
#[must_use]
pub fn build_signature_report_with_source(
    total_functions: usize,
    entries: &[FuncEntry],
    source_file: &str,
) -> Report {
    let mut sig_entries = Vec::with_capacity(entries.len());
    for e in entries {
        let mut m = GoMap::new_struct();
        m.push("name", GoValue::Str(e.name.clone()));
        let bytes = e
            .sig
            .bytes()
            .into_iter()
            .map(|b| GoValue::Uint(u64::from(b)))
            .collect();
        m.push("sig", GoValue::Array(bytes));
        m.push(
            cf_analyze::SOURCE_FILE_KEY,
            GoValue::Str(source_file.to_string()),
        );
        sig_entries.push(GoValue::Object(m));
    }

    let mut report = GoMap::new(MapOrigin::Map);
    report.push(
        KEY_ANALYZER_NAME,
        GoValue::Str(crate::ANALYZER_NAME.to_string()),
    );
    report.push(KEY_TOTAL_FUNCTIONS, GoValue::Int(total_functions as i64));
    report.push(KEY_TOTAL_CLONE_PAIRS, GoValue::Int(0));
    report.push(KEY_CLONE_RATIO, GoValue::Float(0.0));
    report.push(KEY_CLONE_PAIRS, GoValue::Array(Vec::new()));
    report.push(KEY_MESSAGE, GoValue::Str(MSG_NO_CLONES.to_string()));
    report.push(KEY_FUNC_SIGNATURES, GoValue::Array(sig_entries));
    report
}

/// Builds the signature-export report.
///
/// The exported `_func_signatures` is an array of `{name, sig}` objects; the
/// signature is serialized via [`cf_alg_minhash::Signature::bytes`] (the
/// big-endian wire form) as a base value the aggregator decodes back.
#[must_use]
pub fn build_signature_report(total_functions: usize, entries: &[FuncEntry]) -> Report {
    let mut sig_entries = Vec::with_capacity(entries.len());
    for e in entries {
        let mut m = GoMap::new_struct();
        m.push("name", GoValue::Str(e.name.clone()));
        // Signature carried as its big-endian byte form (decoded by the
        // aggregator via Signature::from_bytes) — the portable equivalent of
        // passing the signature by reference.
        let bytes = e
            .sig
            .bytes()
            .into_iter()
            .map(|b| GoValue::Uint(u64::from(b)))
            .collect();
        m.push("sig", GoValue::Array(bytes));
        sig_entries.push(GoValue::Object(m));
    }

    let mut report = GoMap::new(MapOrigin::Map);
    report.push(
        KEY_ANALYZER_NAME,
        GoValue::Str(crate::ANALYZER_NAME.to_string()),
    );
    report.push(KEY_TOTAL_FUNCTIONS, GoValue::Int(total_functions as i64));
    report.push(KEY_TOTAL_CLONE_PAIRS, GoValue::Int(0));
    report.push(KEY_CLONE_RATIO, GoValue::Float(0.0));
    report.push(KEY_CLONE_PAIRS, GoValue::Array(Vec::new()));
    report.push(KEY_MESSAGE, GoValue::Str(MSG_NO_CLONES.to_string()));
    report.push(KEY_FUNC_SIGNATURES, GoValue::Array(sig_entries));
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uast::NodeBuilder;
    use cf_uast_node::Node;

    fn function(name: &str) -> Node {
        let name_node = NodeBuilder::new("Identifier")
            .role("Name")
            .token(name)
            .build();
        let mut f = NodeBuilder::new("Function")
            .role("Function")
            .child(name_node)
            .build();
        let mut block = NodeBuilder::new("Block").build();
        for i in 0..24 {
            let kind = ["Identifier", "Call", "Literal", "Operator"][i % 4];
            block.add_child(NodeBuilder::new(kind).build());
        }
        f.add_child(block);
        f
    }

    #[test]
    fn empty_visitor_reports_no_functions() {
        let v = Visitor::new();
        let report = v.get_report();
        assert_eq!(
            cf_reportutil::get_string(&report, KEY_MESSAGE),
            MSG_NO_FUNCTIONS
        );
    }

    #[test]
    fn visitor_collects_functions_and_exports_signatures() {
        let mut v = Visitor::new();
        let root = NodeBuilder::new("File")
            .child(function("foo"))
            .child(function("bar"))
            .build();
        root.visit_pre_order(&mut |n: &Node| v.on_enter(n));
        assert_eq!(v.function_count(), 2);
        assert_eq!(v.entries().len(), 2);

        let report = v.get_report();
        assert_eq!(cf_reportutil::get_int(&report, KEY_TOTAL_FUNCTIONS), 2);
        assert_eq!(cf_reportutil::get_int(&report, KEY_TOTAL_CLONE_PAIRS), 0);
        // The signature export key is present.
        assert!(cf_reportutil::get(&report, KEY_FUNC_SIGNATURES).is_some());
    }
}
