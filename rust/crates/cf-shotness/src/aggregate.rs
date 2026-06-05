//! Tick aggregation and co-change matrix construction.
//!
//! Ports the data-flow core of `internal/analyzers/shotness/aggregator.go` and
//! `report.go`: per-commit node touches accumulate into per-tick state, coupling
//! pairs are derived from sorted touched-node keys, and the merged per-node data
//! is flattened into the index-keyed `Nodes`/`Counters` report that the metric
//! functions consume.
//!
//! Name-collision semantics ("last-wins") happen upstream during node extraction
//! (see crate docs); here keys are already resolved, and merging is purely
//! additive on counts and coupling counters.

use std::collections::{BTreeMap, HashMap};

use crate::types::{NodeSummary, ReportData};

/// Minimum touched nodes to form at least one coupling pair.
/// Mirrors `minCouplingNodes`.
pub const MIN_COUPLING_NODES: usize = 2;
/// Divisor in `C(n,2) = n*(n-1)/2`. Mirrors `combinatorialPairDivisor`.
pub const COMBINATORIAL_PAIR_DIVISOR: i64 = 2;
/// Cap on touched nodes per commit for coupling map updates.
/// Mirrors `maxCouplingNodes`.
pub const MAX_COUPLING_NODES: usize = 500;

/// Per-node accumulation state in a tick. Mirrors Go `nodeShotnessData`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NodeShotnessData {
    /// Node identity.
    pub summary: NodeSummary,
    /// Accumulated self-change count.
    pub count: i64,
    /// Co-change counters keyed by the *other* node's key.
    ///
    /// A `BTreeMap` is used (rather than a hash map) so iteration during merge
    /// and report-building is deterministic; values are order-independent
    /// additive counts, so the final report is byte-stable.
    pub couples: BTreeMap<String, i64>,
}

/// Per-tick aggregated node map. Mirrors the node portion of Go `TickData`.
///
/// Keyed by node key; a `BTreeMap` keeps deterministic ordering for merges and
/// the sorted node order the report builder relies on.
pub type TickNodes = BTreeMap<String, NodeShotnessData>;

/// Accumulates per-commit node touch deltas into the tick node map.
///
/// Mirrors Go `accumulateNodes`. Each touched node's count increments by its
/// per-commit delta (always 1 for a first touch in the commit).
pub fn accumulate_nodes(acc: &mut TickNodes, nodes_touched: &BTreeMap<String, NodeSummary>) {
    for (key, summary) in nodes_touched {
        let nd = acc.entry(key.clone()).or_insert_with(|| NodeShotnessData {
            summary: summary.clone(),
            count: 0,
            couples: BTreeMap::new(),
        });
        nd.count += 1;
    }
}

/// Computes coupling pairs from touched nodes and updates coupling counters.
///
/// Mirrors Go `computeCouplingPairs`. Returns the combinatorial pair count
/// `C(n,2)`. When `n > MAX_COUPLING_NODES` the O(n²) counter updates are skipped
/// (mass refactor — coupling signal is noise) but the pair count is still
/// returned. Touched-node keys are sorted before pairing, matching Go's
/// `sort.Strings` + `alg.ForEachPair` (upper-triangle) walk.
pub fn compute_coupling_pairs(
    acc: &mut TickNodes,
    nodes_touched: &BTreeMap<String, NodeSummary>,
) -> i64 {
    let n = nodes_touched.len();
    if n < MIN_COUPLING_NODES {
        return 0;
    }

    let coupling_pairs = n as i64 * (n as i64 - 1) / COMBINATORIAL_PAIR_DIVISOR;

    if n > MAX_COUPLING_NODES {
        return coupling_pairs;
    }

    // BTreeMap already yields keys sorted ascending (matches Go sort.Strings).
    let keys: Vec<&String> = nodes_touched.keys().collect();

    for i in 0..keys.len() {
        for j in (i + 1)..keys.len() {
            let key1 = keys[i];
            let key2 = keys[j];

            if let Some(nd) = acc.get_mut(key1) {
                *nd.couples.entry(key2.clone()).or_insert(0) += 1;
            }
            if let Some(nd) = acc.get_mut(key2) {
                *nd.couples.entry(key1.clone()).or_insert(0) += 1;
            }
        }
    }

    coupling_pairs
}

/// Additively merges `src` node data into `dst`.
///
/// Mirrors Go `mergeNodesInto`: counts add, coupling counters add per key, and
/// new nodes are inserted with copied coupling maps.
pub fn merge_nodes_into(dst: &mut TickNodes, src: &TickNodes) {
    for (key, nd) in src {
        match dst.get_mut(key) {
            None => {
                dst.insert(key.clone(), nd.clone());
            }
            Some(existing) => {
                existing.count += nd.count;
                for (ck, cv) in &nd.couples {
                    *existing.couples.entry(ck.clone()).or_insert(0) += cv;
                }
            }
        }
    }
}

/// Builds the index-keyed `Nodes`/`Counters` report from merged node data.
///
/// Port of Go `buildReportFromMerged`. Nodes are ordered by sorted node key;
/// `counters[i][i]` holds node `i`'s self-change count and `counters[i][j]`
/// holds the co-change count with node `j` (only couples whose key resolves to a
/// node in the merged set are kept).
#[must_use]
pub fn build_report_from_merged(merged: &TickNodes) -> ReportData {
    let keys: Vec<String> = merged.keys().cloned().collect(); // BTreeMap → sorted.

    let mut reverse_keys: HashMap<&str, usize> = HashMap::with_capacity(keys.len());
    for (i, key) in keys.iter().enumerate() {
        reverse_keys.insert(key.as_str(), i);
    }

    let mut nodes: Vec<NodeSummary> = Vec::with_capacity(keys.len());
    let mut counters: Vec<HashMap<usize, i64>> = Vec::with_capacity(keys.len());

    for (i, key) in keys.iter().enumerate() {
        let nd = &merged[key];
        nodes.push(nd.summary.clone());

        let mut counter: HashMap<usize, i64> = HashMap::new();
        counter.insert(i, nd.count);

        for (ck, val) in &nd.couples {
            if let Some(&idx) = reverse_keys.get(ck.as_str()) {
                counter.insert(idx, *val);
            }
        }

        counters.push(counter);
    }

    ReportData { nodes, counters }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touched(keys: &[(&str, NodeSummary)]) -> BTreeMap<String, NodeSummary> {
        keys.iter().map(|(k, s)| (k.to_string(), s.clone())).collect()
    }

    #[test]
    fn accumulate_increments_counts() {
        let mut acc: TickNodes = BTreeMap::new();
        let foo = NodeSummary::new("Function", "foo", "a.go");
        let t = touched(&[(&foo.key(), foo.clone())]);
        accumulate_nodes(&mut acc, &t);
        accumulate_nodes(&mut acc, &t);
        assert_eq!(acc[&foo.key()].count, 2);
    }

    #[test]
    fn coupling_pairs_below_min_is_zero() {
        let mut acc: TickNodes = BTreeMap::new();
        let foo = NodeSummary::new("Function", "foo", "a.go");
        let t = touched(&[(&foo.key(), foo.clone())]);
        accumulate_nodes(&mut acc, &t);
        assert_eq!(compute_coupling_pairs(&mut acc, &t), 0);
    }

    #[test]
    fn coupling_pairs_two_nodes() {
        let mut acc: TickNodes = BTreeMap::new();
        let foo = NodeSummary::new("Function", "foo", "a.go");
        let bar = NodeSummary::new("Function", "bar", "a.go");
        let t = touched(&[(&foo.key(), foo.clone()), (&bar.key(), bar.clone())]);
        accumulate_nodes(&mut acc, &t);
        let pairs = compute_coupling_pairs(&mut acc, &t);
        assert_eq!(pairs, 1);
        assert_eq!(acc[&foo.key()].couples[&bar.key()], 1);
        assert_eq!(acc[&bar.key()].couples[&foo.key()], 1);
    }

    #[test]
    fn coupling_pairs_three_nodes_is_c_n_2() {
        let mut acc: TickNodes = BTreeMap::new();
        let a = NodeSummary::new("F", "a", "x");
        let b = NodeSummary::new("F", "b", "x");
        let c = NodeSummary::new("F", "c", "x");
        let t = touched(&[
            (&a.key(), a.clone()),
            (&b.key(), b.clone()),
            (&c.key(), c.clone()),
        ]);
        accumulate_nodes(&mut acc, &t);
        assert_eq!(compute_coupling_pairs(&mut acc, &t), 3); // C(3,2)
    }

    #[test]
    fn build_report_orders_by_sorted_key() {
        let mut acc: TickNodes = BTreeMap::new();
        let foo = NodeSummary::new("Function", "foo", "a.go");
        let bar = NodeSummary::new("Function", "bar", "a.go");
        let t = touched(&[(&foo.key(), foo.clone()), (&bar.key(), bar.clone())]);
        accumulate_nodes(&mut acc, &t);
        compute_coupling_pairs(&mut acc, &t);

        let report = build_report_from_merged(&acc);
        assert_eq!(report.nodes.len(), 2);
        // Function_bar_a.go < Function_foo_a.go
        assert_eq!(report.nodes[0].name, "bar");
        assert_eq!(report.nodes[1].name, "foo");
        assert_eq!(report.counters[0][&0], 1);
        assert_eq!(report.counters[1][&1], 1);
        assert_eq!(report.counters[0][&1], 1);
        assert_eq!(report.counters[1][&0], 1);
    }

    #[test]
    fn merge_is_additive() {
        let foo = NodeSummary::new("Function", "foo", "a.go");
        let bar = NodeSummary::new("Function", "bar", "a.go");
        let mut a: TickNodes = BTreeMap::new();
        let mut b: TickNodes = BTreeMap::new();
        let t = touched(&[(&foo.key(), foo.clone()), (&bar.key(), bar.clone())]);
        accumulate_nodes(&mut a, &t);
        compute_coupling_pairs(&mut a, &t);
        accumulate_nodes(&mut b, &t);
        compute_coupling_pairs(&mut b, &t);

        merge_nodes_into(&mut a, &b);
        assert_eq!(a[&foo.key()].count, 2);
        assert_eq!(a[&foo.key()].couples[&bar.key()], 2);
    }
}
