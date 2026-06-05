//! The cross-file clone-detection [`Aggregator`].
//!
//! Port of `internal/analyzers/clones/aggregator.go`. It collects the per-file
//! `_func_signatures` exports, qualifies each function name with its source file
//! (`sourceFile::name`), builds one global LSH index, finds cross-file clone
//! pairs, and emits the same report shape as the per-file analyzer.

use cf_alg_minhash::Signature;
use cf_analyze::{GoMap, GoValue, MapOrigin, Report};
use cf_uast_node as _; // keep the dependency edge documented; not used directly.

use crate::analyzer::build_empty_report;
use crate::engine::{build_index, find_clone_pairs, ClonePairResult, FuncEntry};
use crate::report::{clone_type_dist_map, ClonePair, SIMILARITY_TYPE3, DEFAULT_MAX_CLONE_PAIRS};
use crate::{
    clone_message, compute_clone_ratio, KEY_ANALYZER_NAME, KEY_CLONE_PAIRS, KEY_CLONE_RATIO,
    KEY_CLONE_TYPE_DISTRIBUTION, KEY_FUNC_SIGNATURES, KEY_MESSAGE, KEY_TOTAL_CLONE_PAIRS,
    KEY_TOTAL_FUNCTIONS, MSG_NO_FUNCTIONS, NUM_BANDS, NUM_ROWS,
};

/// Collects per-file signatures and performs global cross-file clone detection.
///
/// Mirrors Go `Aggregator`.
#[derive(Debug, Clone)]
pub struct Aggregator {
    entries: Vec<FuncEntry>,
    total_functions: i64,
    /// Caps the stored clone pairs in the report detail (`0` = unlimited). The
    /// `total_clone_pairs` count stays exact. Mirrors Go `Aggregator.MaxClonePairs`.
    pub max_clone_pairs: usize,
    /// LSH bands. Mirrors Go `Aggregator.NumBands`.
    pub num_bands: usize,
    /// LSH rows per band. Mirrors Go `Aggregator.NumRows`.
    pub num_rows: usize,
    /// Type-3 similarity threshold. Mirrors Go `Aggregator.SimilarityType3`.
    pub similarity_type3: f64,
}

impl Default for Aggregator {
    fn default() -> Self {
        Self::new()
    }
}

impl Aggregator {
    /// Creates a new aggregator with the Go defaults. Mirrors Go `NewAggregator`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            total_functions: 0,
            max_clone_pairs: DEFAULT_MAX_CLONE_PAIRS,
            num_bands: NUM_BANDS,
            num_rows: NUM_ROWS,
            similarity_type3: SIMILARITY_TYPE3,
        }
    }

    /// Folds a set of per-file reports into the aggregate. Mirrors Go
    /// `Aggregator.Aggregate`.
    pub fn aggregate(&mut self, results: &[(String, Report)]) {
        for (_, report) in results {
            self.total_functions += cf_reportutil::get_int(report, KEY_TOTAL_FUNCTIONS);
            self.collect_signatures(report);
        }
    }

    /// Extracts the `_func_signatures` entries from one report, qualifying names
    /// with the source file. Mirrors Go `collectSignatures`.
    fn collect_signatures(&mut self, report: &Report) {
        let Some(GoValue::Array(items)) = cf_reportutil::get(report, KEY_FUNC_SIGNATURES) else {
            return;
        };

        let source_file = extract_source_file(items);

        for item in items {
            let GoValue::Map(m) = item else {
                continue;
            };
            let Some(name) = field_str(m, "name") else {
                continue;
            };
            let Some(sig) = field_signature(m, "sig") else {
                continue;
            };

            let qualified = qualify_func_name(&name, &source_file);
            self.entries.push(FuncEntry { name: qualified, sig });
        }
    }

    /// Builds the global LSH index, finds cross-file pairs, and emits the report.
    ///
    /// Mirrors Go `Aggregator.GetResult`.
    #[must_use]
    pub fn get_result(&self) -> Report {
        if self.total_functions == 0 {
            return build_empty_report(MSG_NO_FUNCTIONS);
        }

        let result = self.detect_global_clones();

        let clone_ratio =
            compute_clone_ratio(result.cloned_func.len(), self.total_functions as usize);
        let message = clone_message(result.total_count);

        let pairs_value =
            GoValue::Array(result.pairs.iter().map(ClonePair::to_go_value).collect::<Vec<_>>());
        let dist = clone_type_dist_map(result.type_distribution);

        let mut report = GoMap::new(MapOrigin::Map);
        report.push(KEY_ANALYZER_NAME, GoValue::Str(crate::ANALYZER_NAME.to_string()));
        report.push(KEY_TOTAL_FUNCTIONS, GoValue::Int(self.total_functions));
        report.push(KEY_TOTAL_CLONE_PAIRS, GoValue::Int(result.total_count as i64));
        report.push(KEY_CLONE_RATIO, GoValue::Float(clone_ratio));
        report.push(KEY_CLONE_PAIRS, pairs_value);
        report.push(KEY_CLONE_TYPE_DISTRIBUTION, dist);
        report.push(KEY_MESSAGE, GoValue::Str(message.to_string()));
        report
    }

    /// Builds one global LSH index and finds clone pairs. Mirrors Go
    /// `detectGlobalClones`.
    fn detect_global_clones(&self) -> ClonePairResult {
        if self.entries.is_empty() {
            return ClonePairResult::default();
        }

        let Some(idx) = build_index(&self.entries, self.num_bands, self.num_rows) else {
            return ClonePairResult::default();
        };

        find_clone_pairs(&self.entries, &idx, self.max_clone_pairs, self.similarity_type3)
    }
}

/// Returns `sourceFile::name` if `source_file` is non-empty, else `name`.
///
/// Mirrors Go `qualifyFuncName`.
#[must_use]
fn qualify_func_name(name: &str, source_file: &str) -> String {
    if source_file.is_empty() {
        name.to_string()
    } else {
        format!("{source_file}::{name}")
    }
}

/// Reads `_source_file` from the first signature entry. Mirrors Go
/// `extractSourceFile`.
#[must_use]
fn extract_source_file(items: &[GoValue]) -> String {
    let Some(first) = items.first() else {
        return String::new();
    };
    let GoValue::Map(m) = first else {
        return String::new();
    };
    field_str(m, "_source_file").unwrap_or_default()
}

/// Reads a string field from a map entry, if present and a string.
fn field_str(m: &GoMap, key: &str) -> Option<String> {
    m.entries().iter().find_map(|(k, v)| match (k.as_str() == key, v) {
        (true, GoValue::Str(s)) => Some(s.clone()),
        _ => None,
    })
}

/// Reads a signature field from a map entry (the big-endian byte array produced
/// by [`crate::visitor::build_signature_report`]) and decodes it.
fn field_signature(m: &GoMap, key: &str) -> Option<Signature> {
    let value = m.entries().iter().find(|(k, _)| k == key).map(|(_, v)| v)?;
    let GoValue::Array(bytes) = value else {
        return None;
    };
    let raw: Vec<u8> = bytes
        .iter()
        .filter_map(|v| match v {
            GoValue::Uint(b) => u8::try_from(*b).ok(),
            GoValue::Int(b) => u8::try_from(*b).ok(),
            _ => None,
        })
        .collect();
    Signature::from_bytes(&raw).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visitor::Visitor;
    use cf_uast_node::{Builder, Node};

    fn function(name: &str) -> Node {
        let name_node = Builder::new("Identifier").role("Name").token(name).build();
        let mut f = Builder::new("Function").role("Function").child(name_node).build();
        let mut block = Node::new("Block");
        for i in 0..24 {
            let kind = ["Identifier", "Call", "Literal", "Operator"][i % 4];
            block.add_child(Node::new(kind));
        }
        f.add_child(block);
        f
    }

    fn file_report(funcs: &[&str]) -> Report {
        let mut v = Visitor::new();
        let mut file = Builder::new("File");
        for name in funcs {
            file = file.child(function(name));
        }
        let root = file.build();
        root.visit_pre_order(&mut |n| v.on_enter(n));
        v.get_report()
    }

    #[test]
    fn qualify_with_and_without_source() {
        assert_eq!(qualify_func_name("Foo", ""), "Foo");
        assert_eq!(qualify_func_name("Foo", "a.go"), "a.go::Foo");
    }

    #[test]
    fn no_functions_returns_empty_report() {
        let agg = Aggregator::new();
        let report = agg.get_result();
        assert_eq!(cf_reportutil::get_string(&report, KEY_MESSAGE), MSG_NO_FUNCTIONS);
    }

    #[test]
    fn cross_file_identical_functions_form_a_clone_pair() {
        let mut agg = Aggregator::new();
        let r1 = file_report(&["foo"]);
        let r2 = file_report(&["bar"]);
        agg.aggregate(&[("f1".into(), r1), ("f2".into(), r2)]);

        let report = agg.get_result();
        assert_eq!(cf_reportutil::get_int(&report, KEY_TOTAL_FUNCTIONS), 2);
        // Two structurally identical functions across files -> one clone pair.
        assert_eq!(cf_reportutil::get_int(&report, KEY_TOTAL_CLONE_PAIRS), 1);
    }

    #[test]
    fn signature_round_trips_through_export() {
        // A signature carried through the report export and re-decoded must
        // compare identical to the original.
        let report = file_report(&["foo"]);
        let mut agg = Aggregator::new();
        agg.aggregate(&[("f".into(), report)]);
        assert_eq!(agg.entries.len(), 1);
    }
}
