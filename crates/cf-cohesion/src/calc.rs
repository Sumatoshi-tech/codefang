//! LCOM-HS and per-function cohesion math over distinct variable sets.
//!
//! All inputs are sets: a function's variables are distinct names, so every
//! count below is a count of *functions*, never of occurrences.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::analyzer::Function;

/// LCOM needs at least this many functions that use a variable; below it
/// there is nothing to be un-cohesive about and LCOM is `0`.
pub const MIN_FUNCTIONS_FOR_LCOM: usize = 2;
/// A variable is shared when at least this many functions use it.
pub const SHARED_MIN_FUNCTIONS: usize = 2;
/// Cohesion of a function that uses no variables (nothing to be isolated
/// from) and of a module with fewer than [`MIN_FUNCTIONS_FOR_LCOM`] functions.
pub const PERFECT_COHESION: f64 = 1.0;

/// LCOM-HS (Henderson-Sellers) over the functions that use at least one
/// variable: `1 - sum(mA) / (m * a)` with `m` their count, `a` the number of
/// distinct variables across them and `sum(mA)` the number of
/// (function, variable) usage pairs. `0.0` when fewer than
/// [`MIN_FUNCTIONS_FOR_LCOM`] functions use a variable.
///
/// ```
/// use cf_cohesion::calc::lcom;
/// use cf_cohesion::Function;
/// let f = Function::with_variables("f", &["x", "y"]);
/// let g = Function::with_variables("g", &["x"]);
/// // m = 2, a = 2, sum(mA) = 3 -> 1 - 3/4
/// assert!((lcom(&[f, g]) - 0.25).abs() < 1e-9);
/// ```
#[must_use]
pub fn lcom(functions: &[Function]) -> f64 {
    let using: Vec<&Function> = functions
        .iter()
        .filter(|f| !f.variables.is_empty())
        .collect();
    if using.len() < MIN_FUNCTIONS_FOR_LCOM {
        return 0.0;
    }
    let all: BTreeSet<&str> = using
        .iter()
        .flat_map(|f| f.variables.iter().map(String::as_str))
        .collect();
    let m = using.len() as f64;
    let a = all.len() as f64;
    let sum_ma: usize = using.iter().map(|f| f.variables.len()).sum();
    unit(1.0 - sum_ma as f64 / (m * a))
}

/// Cohesion score: `1 - lcom`, clamped to `[0, 1]`.
#[must_use]
pub fn cohesion_score(lcom: f64) -> f64 {
    unit(1.0 - lcom)
}

/// The variables used by at least [`SHARED_MIN_FUNCTIONS`] functions.
#[must_use]
pub fn shared_variables(functions: &[Function]) -> HashSet<&str> {
    let mut users: HashMap<&str, usize> = HashMap::new();
    for f in functions {
        for v in &f.variables {
            *users.entry(v.as_str()).or_insert(0) += 1;
        }
    }
    users
        .into_iter()
        .filter(|&(_, n)| n >= SHARED_MIN_FUNCTIONS)
        .map(|(v, _)| v)
        .collect()
}

/// Per-function cohesion: the fraction of the function's distinct variables
/// that are shared with another function. [`PERFECT_COHESION`] for a function
/// without variables.
#[must_use]
pub fn function_cohesion(f: &Function, shared: &HashSet<&str>) -> f64 {
    if f.variables.is_empty() {
        return PERFECT_COHESION;
    }
    let shared_count = f
        .variables
        .iter()
        .filter(|v| shared.contains(v.as_str()))
        .count();
    shared_count as f64 / f.variables.len() as f64
}

/// Per-function cohesion of every function, in order. When fewer than
/// [`MIN_FUNCTIONS_FOR_LCOM`] functions use a variable there is nothing to
/// compare against — a single-function file — and every function scores
/// [`PERFECT_COHESION`], consistent with `lcom = 0` for that file.
#[must_use]
pub fn function_cohesions(functions: &[Function]) -> Vec<f64> {
    let using = functions.iter().filter(|f| !f.variables.is_empty()).count();
    if using < MIN_FUNCTIONS_FOR_LCOM {
        return vec![PERFECT_COHESION; functions.len()];
    }
    let shared = shared_variables(functions);
    functions
        .iter()
        .map(|f| function_cohesion(f, &shared))
        .collect()
}

/// Arithmetic mean of `values`, or `default` when there are none.
#[must_use]
pub fn mean_or(values: &[f64], default: f64) -> f64 {
    if values.is_empty() {
        return default;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

/// Clamps to the unit interval (guards floating-point drift at the edges).
fn unit(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(name: &str, vars: &[&str]) -> Function {
        Function::with_variables(name, vars)
    }

    #[test]
    fn lcom_is_zero_below_two_functions_or_without_variables() {
        assert_eq!(lcom(&[]), 0.0);
        assert_eq!(lcom(&[f("f", &["x"])]), 0.0);
        assert_eq!(lcom(&[f("f", &[]), f("g", &[])]), 0.0);
    }

    #[test]
    fn lcom_disjoint_variables_is_half() {
        // m = 2, a = 2, sum(mA) = 2 -> 1 - 2/4.
        assert!((lcom(&[f("f", &["x"]), f("g", &["y"])]) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn lcom_ignores_functions_without_variables() {
        // h uses nothing: it is not part of the usage matrix.
        let fns = [f("f", &["x"]), f("g", &["x"]), f("h", &[])];
        assert_eq!(lcom(&fns), 0.0);
    }

    #[test]
    fn shared_means_at_least_two_functions() {
        let fns = [f("f", &["x", "y"]), f("g", &["x"])];
        let shared = shared_variables(&fns);
        assert!(shared.contains("x"));
        assert!(!shared.contains("y"));
    }

    #[test]
    fn function_cohesion_is_shared_over_unique() {
        let fns = [f("f", &["x", "y"]), f("g", &["x"]), f("h", &[])];
        let shared = shared_variables(&fns);
        assert!((function_cohesion(&fns[0], &shared) - 0.5).abs() < 1e-9);
        assert_eq!(function_cohesion(&fns[1], &shared), 1.0);
        assert_eq!(function_cohesion(&fns[2], &shared), PERFECT_COHESION);
    }

    #[test]
    fn single_variable_using_function_is_perfect() {
        // One function uses variables: nothing to compare against.
        assert_eq!(function_cohesions(&[f("f", &["x"])]), [1.0]);
        assert_eq!(
            function_cohesions(&[f("f", &["x"]), f("g", &[])]),
            [1.0, 1.0]
        );
        // Two functions use variables: the sharing rule applies.
        let got = function_cohesions(&[f("f", &["x", "y"]), f("g", &["x"]), f("h", &[])]);
        assert!((got[0] - 0.5).abs() < 1e-9);
        assert_eq!(&got[1..], [1.0, 1.0]);
    }

    #[test]
    fn mean_or_uses_default_when_empty() {
        assert_eq!(mean_or(&[], 1.0), 1.0);
        assert!((mean_or(&[1.0, 0.0], 1.0) - 0.5).abs() < 1e-9);
    }
}
