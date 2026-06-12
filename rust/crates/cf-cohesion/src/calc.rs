//! Cohesion calculations.
//!
//! Computes the three module scalars and the per-function cohesion via
//! Bloom-filter membership of shared variables. The Bloom-filter sizing
//! constants and the "only variables appearing in more than one function are
//! shared" rule are part of the report contract.

use crate::analyzer::Function;
// The shared sketch crate's Bloom filter (FNV-128a hash kernel, pinned
// sizing + double hashing). The cohesion math MUST use this exact filter:
// a different hash family changes the per-function shared-variable false
// positives — and thus the cohesion scores in machine output.
use cf_alg_bloom::Filter;
use std::collections::HashMap;

/// 1% false-positive rate for per-function and global Bloom filters.
pub const BLOOM_FP_RATE: f64 = 0.01;
/// Minimum expected elements for a per-function filter.
pub const BLOOM_MIN_ELEMENTS: u64 = 16;
/// Minimum expected elements for the global shared-variable filter.
pub const BLOOM_GLOBAL_MIN_ELEMS: u64 = 64;

/// Clamps `v` to the inclusive range `[lo, hi]`: returns `lo` if `v < lo`,
/// `hi` if `v > hi`, else `v`.
#[must_use]
pub fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

/// Returns the distinct entries of `items`, preserving **first-seen order**.
#[must_use]
pub fn unique(items: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::with_capacity(items.len());
    let mut out = Vec::with_capacity(items.len());
    for s in items {
        if seen.insert(s.as_str()) {
            out.push(s.clone());
        }
    }
    out
}

/// LCOM and cohesion calculations. Stateless methods on the analyzer.
impl crate::analyzer::Analyzer {
    /// Calculates LCOM-HS (Henderson-Sellers): `LCOM = 1 - sum(mA) / (m * a)`.
    ///
    /// * `m` = number of functions
    /// * `a` = number of distinct variables across all functions
    /// * `mA` = for each variable, the count of functions that reference it
    ///
    /// Range `[0, 1]`; 0 = perfect cohesion, 1 = none. Returns `0.0` for `<= 1`
    /// function or when there are no variables.
    #[must_use]
    pub fn calculate_lcom(&self, functions: &[Function]) -> f64 {
        if functions.len() <= 1 {
            return 0.0;
        }
        let all_vars = collect_unique_variables(functions);
        if all_vars.is_empty() {
            return 0.0;
        }
        let m = functions.len() as f64;
        let a = all_vars.len() as f64;
        let sum_ma = count_variable_accesses(&all_vars, functions);
        clamp(1.0 - (sum_ma / (m * a)), 0.0, 1.0)
    }

    /// Converts LCOM-HS to a cohesion score (higher is better): `1 - lcom`.
    /// Returns `1.0` for `<= 1` function.
    #[must_use]
    pub fn calculate_cohesion_score(&self, lcom: f64, function_count: usize) -> f64 {
        if function_count <= 1 {
            return 1.0;
        }
        clamp(1.0 - lcom, 0.0, 1.0)
    }

    /// Average per-function cohesion. Returns `1.0` for an empty slice.
    #[must_use]
    pub fn calculate_function_cohesion(&self, functions: &[Function]) -> f64 {
        if functions.is_empty() {
            return 1.0;
        }
        let total: f64 = functions.iter().map(|f| f.cohesion).sum();
        total / functions.len() as f64
    }

    /// Per-function cohesion = (shared unique vars) / (unique vars).
    ///
    /// A variable is "shared" iff it tests positive against `global_filter` (which
    /// contains every variable used by more than one function). Functions with no
    /// variables score `1.0`; if `global_filter` is `None` the score is `0.0`.
    #[must_use]
    pub fn calculate_function_level_cohesion(
        &self,
        function: &Function,
        global_filter: Option<&Filter>,
    ) -> f64 {
        let unique_vars = unique(&function.variables);
        if unique_vars.is_empty() {
            return 1.0;
        }
        let Some(filter) = global_filter else {
            return 0.0;
        };
        let shared = unique_vars
            .iter()
            .filter(|v| filter.test(v.as_bytes()))
            .count();
        shared as f64 / unique_vars.len() as f64
    }
}

/// Gathers all distinct variable names across all functions. Order is
/// unspecified; only the *count* feeds the LCOM formula, so order does not
/// affect the scalar.
#[must_use]
pub fn collect_unique_variables(functions: &[Function]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    for f in functions {
        for v in &f.variables {
            seen.insert(v.clone());
        }
    }
    seen.into_iter().collect()
}

/// Counts function-variable access pairs: for each variable, how many functions
/// reference it. Uses one Bloom filter per function for O(1) membership tests.
///
/// The return is `f64` because it feeds the LCOM division directly. Bloom false
/// positives are part of the defined behavior and are reproduced via the shared
/// sketch crate.
#[must_use]
pub fn count_variable_accesses(all_vars: &[String], functions: &[Function]) -> f64 {
    let filters = build_per_function_bloom_filters(functions);
    let mut sum = 0.0;
    for var_name in all_vars {
        let key = var_name.as_bytes();
        for filter in filters.iter().flatten() {
            if filter.test(key) {
                sum += 1.0;
            }
        }
    }
    sum
}

/// Builds a Bloom filter for each function's variable set. A `None` entry
/// stands in for a filter whose construction failed (invalid parameters); such
/// entries are skipped by the membership loop.
#[must_use]
pub fn build_per_function_bloom_filters(functions: &[Function]) -> Vec<Option<Filter>> {
    functions
        .iter()
        .map(|f| {
            let n = (f.variables.len() as u64).max(BLOOM_MIN_ELEMENTS);
            match Filter::new_with_estimates(n, BLOOM_FP_RATE) {
                Ok(mut filter) => {
                    for v in &f.variables {
                        filter.add(v.as_bytes());
                    }
                    Some(filter)
                }
                Err(_) => None,
            }
        })
        .collect()
}

/// Builds the global shared-variable Bloom filter.
///
/// Only variables appearing in **more than one** function are added. Returns
/// `None` when there are no variables, or no variable is shared, or the filter
/// cannot be constructed.
#[must_use]
pub fn build_global_variable_filter(functions: &[Function]) -> Option<Filter> {
    // Count occurrences across functions.
    let mut seen: HashMap<&str, u32> = HashMap::new();
    for f in functions {
        for v in &f.variables {
            *seen.entry(v.as_str()).or_insert(0) += 1;
        }
    }
    if seen.is_empty() {
        return None;
    }
    let shared_count = seen.values().filter(|&&c| c > 1).count() as u64;
    if shared_count == 0 {
        return None;
    }
    let n = shared_count.max(BLOOM_GLOBAL_MIN_ELEMS);
    let mut filter = Filter::new_with_estimates(n, BLOOM_FP_RATE).ok()?;
    for (v, count) in &seen {
        if *count > 1 {
            filter.add(v.as_bytes());
        }
    }
    Some(filter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::{Analyzer, Function};

    fn func(name: &str, vars: &[&str]) -> Function {
        Function {
            name: name.to_string(),
            variables: vars.iter().map(|s| s.to_string()).collect(),
            line_count: 1,
            cohesion: 0.0,
        }
    }

    #[test]
    fn clamp_bounds() {
        assert_eq!(clamp(-1.0, 0.0, 1.0), 0.0);
        assert_eq!(clamp(2.0, 0.0, 1.0), 1.0);
        assert_eq!(clamp(0.5, 0.0, 1.0), 0.5);
    }

    #[test]
    fn unique_preserves_first_seen_order() {
        let v: Vec<String> = ["a", "b", "a", "c", "b"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(unique(&v), vec!["a", "b", "c"]);
    }

    #[test]
    fn lcom_zero_for_single_function() {
        let a = Analyzer::new();
        assert_eq!(a.calculate_lcom(&[func("f", &["x"])]), 0.0);
    }

    #[test]
    fn lcom_zero_when_no_variables() {
        let a = Analyzer::new();
        let fns = vec![func("f", &[]), func("g", &[])];
        assert_eq!(a.calculate_lcom(&fns), 0.0);
    }

    #[test]
    fn lcom_perfect_cohesion_all_share_all() {
        // Two functions each using the same single variable:
        // m=2, a=1, sumMA=2 -> LCOM = 1 - 2/(2*1) = 0.
        let a = Analyzer::new();
        let fns = vec![func("f", &["x"]), func("g", &["x"])];
        assert!((a.calculate_lcom(&fns) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn lcom_no_cohesion_disjoint_vars() {
        // Two functions with disjoint single variables:
        // m=2, a=2, sumMA=2 -> LCOM = 1 - 2/(2*2) = 0.5.
        let a = Analyzer::new();
        let fns = vec![func("f", &["x"]), func("g", &["y"])];
        assert!((a.calculate_lcom(&fns) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn cohesion_score_inverts_lcom() {
        let a = Analyzer::new();
        assert_eq!(a.calculate_cohesion_score(0.5, 2), 0.5);
        assert_eq!(a.calculate_cohesion_score(0.9, 1), 1.0); // <=1 fn -> 1.0
    }

    #[test]
    fn global_filter_none_when_nothing_shared() {
        // Disjoint vars: nothing appears in >1 function.
        let fns = vec![func("f", &["x"]), func("g", &["y"])];
        assert!(build_global_variable_filter(&fns).is_none());
    }

    #[test]
    fn global_filter_some_when_var_shared() {
        let fns = vec![func("f", &["x", "y"]), func("g", &["x"])];
        let filter = build_global_variable_filter(&fns).expect("x is shared");
        assert!(filter.test(b"x"));
    }

    #[test]
    fn function_level_cohesion_no_vars_is_one() {
        let a = Analyzer::new();
        assert_eq!(
            a.calculate_function_level_cohesion(&func("f", &[]), None),
            1.0
        );
    }

    #[test]
    fn function_level_cohesion_nil_filter_is_zero() {
        let a = Analyzer::new();
        assert_eq!(
            a.calculate_function_level_cohesion(&func("f", &["x"]), None),
            0.0
        );
    }

    #[test]
    fn function_level_cohesion_shared_ratio() {
        let a = Analyzer::new();
        let fns = vec![func("f", &["x", "y"]), func("g", &["x"])];
        let filter = build_global_variable_filter(&fns).unwrap();
        // f has unique vars {x, y}; only x is shared -> 1/2 = 0.5.
        let c = a.calculate_function_level_cohesion(&fns[0], Some(&filter));
        assert!((c - 0.5).abs() < 1e-9);
    }

    #[test]
    fn function_cohesion_average() {
        let a = Analyzer::new();
        let mut fns = vec![func("f", &["x"]), func("g", &["y"])];
        fns[0].cohesion = 1.0;
        fns[1].cohesion = 0.0;
        assert_eq!(a.calculate_function_cohesion(&fns), 0.5);
        assert_eq!(a.calculate_function_cohesion(&[]), 1.0);
    }
}
