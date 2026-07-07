//! `cf-alg-mapx` — generic map and slice helpers.
//!
//! Three families of helpers used throughout the analyzers:
//!
//! * **Clone**: [`clone_func`], [`clone_nested`] — deep copies of (nested) maps.
//! * **Merge**: [`merge_additive`], [`merge_nested_additive`] — additive merges
//!   of numeric maps (`dst[k] += src[k]`).
//! * **Sorted / set / unique**: [`sorted_keys`] (the headline deterministic
//!   ordering helper), [`sort_and_limit`], [`build_lookup_set`], [`unique`],
//!   plus [`estimate_map_size`].
//!
//! # The absent/empty distinction
//!
//! Callers distinguish an *absent* map/slice from an *empty* (but allocated)
//! one, and these helpers deliberately propagate that distinction
//! (reference-implementation behavior): every function returns `None` when
//! given `None`, but returns an allocated empty value when given an allocated
//! empty value.
//!
//! So the cloning/extraction helpers take `Option<&...>` and return
//! `Option<...>`. The mutating merge helpers take `Option<&mut ...>` for
//! `dst` (a `None` `dst` is a no-op).

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

/// Numeric constraint for additive merges (the `+=` operator).
///
/// Admits every signed/unsigned integer width and both float widths via a
/// blanket impl for the corresponding primitive types.
pub trait Numeric: Copy + std::ops::AddAssign {}

macro_rules! impl_numeric {
    ($($t:ty),* $(,)?) => {
        $( impl Numeric for $t {} )*
    };
}

impl_numeric!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize, f32, f64);

/// Returns a deep copy of `m`, applying `clone_v` to each value.
///
/// Returns `None` for a `None` map.
///
/// The keys are cloned with [`Clone`]; each value is produced by the supplied
/// `clone_v` closure, which is how callers express a deep copy of value types
/// that are not trivially `Clone` (for example, copying an inner slice rather
/// than aliasing it).
///
/// # Examples
///
/// ```
/// use std::collections::HashMap;
/// use cf_alg_mapx::clone_func;
///
/// let mut src: HashMap<String, Vec<i32>> = HashMap::new();
/// src.insert("x".into(), vec![1, 2, 3]);
///
/// let mut cloned = clone_func(Some(&src), |v: &Vec<i32>| v.clone()).unwrap();
/// cloned.get_mut("x").unwrap()[0] = 99;
///
/// // The source is untouched.
/// assert_eq!(src["x"][0], 1);
/// ```
///
/// A `None` map yields `None`:
///
/// ```
/// use std::collections::HashMap;
/// use cf_alg_mapx::clone_func;
///
/// let out = clone_func::<String, Vec<i32>, _>(None, |v: &Vec<i32>| v.clone());
/// assert!(out.is_none());
/// ```
#[must_use]
pub fn clone_func<K, V, F>(m: Option<&HashMap<K, V>>, mut clone_v: F) -> Option<HashMap<K, V>>
where
    K: Eq + Hash + Clone,
    F: FnMut(&V) -> V,
{
    let m = m?;
    let mut clone = HashMap::with_capacity(m.len());
    for (k, v) in m {
        clone.insert(k.clone(), clone_v(v));
    }
    Some(clone)
}

/// Returns a deep copy of a two-level nested map.
///
/// Outer and inner maps are independently allocated. Returns `None` for a
/// `None` map. A `None` inner map is preserved as `None`.
///
/// Values are cloned with [`Clone`].
///
/// # Examples
///
/// ```
/// use std::collections::HashMap;
/// use cf_alg_mapx::clone_nested;
///
/// let mut src: HashMap<i32, Option<HashMap<i32, i64>>> = HashMap::new();
/// src.insert(1, Some([(10, 100i64), (20, 200)].into_iter().collect()));
/// src.insert(2, None); // a nil inner map
///
/// let mut cloned = clone_nested(Some(&src)).unwrap();
/// cloned.get_mut(&1).unwrap().as_mut().unwrap().insert(10, 999);
///
/// // Source inner map is independent; the nil inner map is preserved.
/// assert_eq!(src[&1].as_ref().unwrap()[&10], 100);
/// assert!(cloned[&2].is_none());
/// ```
#[must_use]
pub fn clone_nested<K1, K2, V>(
    m: Option<&HashMap<K1, Option<HashMap<K2, V>>>>,
) -> Option<HashMap<K1, Option<HashMap<K2, V>>>>
where
    K1: Eq + Hash + Clone,
    K2: Eq + Hash + Clone,
    V: Clone,
{
    let m = m?;
    let mut clone = HashMap::with_capacity(m.len());
    for (k1, inner) in m {
        match inner {
            None => {
                clone.insert(k1.clone(), None);
            }
            Some(inner) => {
                clone.insert(k1.clone(), Some(inner.clone()));
            }
        }
    }
    Some(clone)
}

/// Additively merges `src` into `dst`: `dst[k] += src[k]` for every key in
/// `src`.
///
/// If `dst` is `None`, this is a no-op. Keys present only in `src` are
/// inserted with their `src` value (the missing `dst` entry starts at the
/// additive identity).
///
/// # Examples
///
/// ```
/// use std::collections::HashMap;
/// use cf_alg_mapx::merge_additive;
///
/// let mut dst: HashMap<&str, i32> = [("a", 1), ("b", 2)].into_iter().collect();
/// let src: HashMap<&str, i32> = [("b", 3), ("c", 4)].into_iter().collect();
/// merge_additive(Some(&mut dst), &src);
///
/// assert_eq!(dst["a"], 1);
/// assert_eq!(dst["b"], 5);
/// assert_eq!(dst["c"], 4);
/// ```
pub fn merge_additive<K, V>(dst: Option<&mut HashMap<K, V>>, src: &HashMap<K, V>)
where
    K: Eq + Hash + Clone,
    V: Numeric + Default,
{
    let Some(dst) = dst else { return };
    for (k, v) in src {
        *dst.entry(k.clone()).or_default() += *v;
    }
}

/// Additively merges two-level maps.
///
/// For each key `k1` in `src` with a
/// non-empty inner map, the inner map is merged additively into `dst[k1]` via
/// [`merge_additive`]; a missing `dst[k1]` is initialized first. Empty inner
/// maps in `src` are skipped (so they do not allocate a `dst` entry). If `dst`
/// is `None`, this is a no-op.
///
/// # Examples
///
/// ```
/// use std::collections::HashMap;
/// use cf_alg_mapx::merge_nested_additive;
///
/// let mut dst: HashMap<i32, HashMap<i32, i64>> = HashMap::new();
/// let mut src: HashMap<i32, HashMap<i32, i64>> = HashMap::new();
/// src.insert(1, [(10, 100i64), (20, 200)].into_iter().collect());
/// merge_nested_additive(Some(&mut dst), &src);
///
/// assert_eq!(dst[&1][&10], 100);
/// assert_eq!(dst[&1][&20], 200);
/// ```
///
/// Empty inner maps are skipped:
///
/// ```
/// use std::collections::HashMap;
/// use cf_alg_mapx::merge_nested_additive;
///
/// let mut dst: HashMap<i32, HashMap<i32, i64>> = HashMap::new();
/// let mut src: HashMap<i32, HashMap<i32, i64>> = HashMap::new();
/// src.insert(1, HashMap::new());
/// merge_nested_additive(Some(&mut dst), &src);
///
/// assert!(!dst.contains_key(&1));
/// ```
pub fn merge_nested_additive<K1, K2, V>(
    dst: Option<&mut HashMap<K1, HashMap<K2, V>>>,
    src: &HashMap<K1, HashMap<K2, V>>,
) where
    K1: Eq + Hash + Clone,
    K2: Eq + Hash + Clone,
    V: Numeric + Default,
{
    let Some(dst) = dst else { return };
    for (k1, inner) in src {
        if inner.is_empty() {
            continue;
        }
        let target = dst.entry(k1.clone()).or_default();
        merge_additive(Some(target), inner);
    }
}

/// Estimates the memory usage of `m`, assuming `entry_bytes` per entry.
///
/// Computes `m.len() * entry_bytes`. A `None` (absent) map has length zero,
/// so it estimates `0`.
///
/// # Examples
///
/// ```
/// use std::collections::HashMap;
/// use cf_alg_mapx::estimate_map_size;
///
/// let m: HashMap<&str, i32> = [("a", 1), ("b", 2), ("c", 3)].into_iter().collect();
/// assert_eq!(estimate_map_size(Some(&m), 56), 3 * 56);
/// assert_eq!(estimate_map_size::<&str, i32>(None, 56), 0);
/// ```
#[must_use]
pub fn estimate_map_size<K, V>(m: Option<&HashMap<K, V>>, entry_bytes: i64) -> i64 {
    m.map_or(0, |m| m.len() as i64) * entry_bytes
}

/// Returns the keys of `m` in sorted (ascending) order.
///
/// Returns `None` for a `None` (absent) map. This is the
/// deterministic-ordering primitive used throughout the analyzers: callers
/// iterate `sorted_keys(Some(&m))` instead of `m.keys()` so that output is
/// reproducible regardless of the map's internal (unspecified) iteration
/// order.
///
/// # Examples
///
/// ```
/// use std::collections::HashMap;
/// use cf_alg_mapx::sorted_keys;
///
/// let m: HashMap<&str, i32> =
///     [("banana", 2), ("apple", 1), ("cherry", 3)].into_iter().collect();
/// assert_eq!(sorted_keys(Some(&m)), Some(vec!["apple", "banana", "cherry"]));
/// ```
///
/// A `None` map yields `None`; an empty map yields `Some([])`:
///
/// ```
/// use std::collections::HashMap;
/// use cf_alg_mapx::sorted_keys;
///
/// assert_eq!(sorted_keys::<i32, &str>(None), None);
/// let empty: HashMap<i32, &str> = HashMap::new();
/// assert_eq!(sorted_keys(Some(&empty)), Some(vec![]));
/// ```
#[must_use]
pub fn sorted_keys<K, V>(m: Option<&HashMap<K, V>>) -> Option<Vec<K>>
where
    K: Ord + Clone,
{
    let m = m?;
    let mut keys: Vec<K> = m.keys().cloned().collect();
    keys.sort();
    Some(keys)
}

/// Copies `items`, sorts the copy with `less`, and returns at most `limit`
/// elements.
///
/// Returns `None` for a `None` (absent) slice. If `limit <= 0`, the limit is
/// ignored and all sorted items are returned. The original slice is never
/// mutated.
///
/// `less(a, b)` returns `true` when `a` should sort before `b`. The sort is
/// stable.
///
/// # Examples
///
/// ```
/// use cf_alg_mapx::sort_and_limit;
///
/// let descending = |a: &i32, b: &i32| a > b;
/// let got = sort_and_limit(Some(&[5, 1, 4, 2, 3]), descending, 3);
/// assert_eq!(got, Some(vec![5, 4, 3]));
///
/// // limit = 0 means "no limit".
/// let all = sort_and_limit(Some(&[3, 1, 2]), descending, 0);
/// assert_eq!(all, Some(vec![3, 2, 1]));
/// ```
#[must_use]
pub fn sort_and_limit<T, F>(items: Option<&[T]>, mut less: F, limit: i64) -> Option<Vec<T>>
where
    T: Clone,
    F: FnMut(&T, &T) -> bool,
{
    let items = items?;
    let mut sorted: Vec<T> = items.to_vec();
    sorted.sort_by(|a, b| {
        if less(a, b) {
            std::cmp::Ordering::Less
        } else if less(b, a) {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    });
    if limit > 0 && sorted.len() as i64 > limit {
        sorted.truncate(limit as usize);
    }
    Some(sorted)
}

/// Converts a slice into a lookup set.
///
/// Duplicate items are silently deduplicated. Returns `None` for a `None`
/// (absent) slice.
///
/// # Examples
///
/// ```
/// use cf_alg_mapx::build_lookup_set;
///
/// let set = build_lookup_set(Some(&[1, 2, 1, 3, 2])).unwrap();
/// assert_eq!(set.len(), 3);
/// assert!(set.contains(&1) && set.contains(&2) && set.contains(&3));
/// ```
#[must_use]
pub fn build_lookup_set<T>(items: Option<&[T]>) -> Option<HashSet<T>>
where
    T: Eq + Hash + Clone,
{
    let items = items?;
    let mut set = HashSet::with_capacity(items.len());
    for item in items {
        set.insert(item.clone());
    }
    Some(set)
}

/// Returns a new vector containing only the first occurrence of each element,
/// preserving insertion order.
///
/// Returns `None` for a `None` (absent) slice.
///
/// # Examples
///
/// ```
/// use cf_alg_mapx::unique;
///
/// let got = unique(Some(&[3, 1, 2, 1, 3, 4, 2]));
/// assert_eq!(got, Some(vec![3, 1, 2, 4]));
/// ```
#[must_use]
pub fn unique<T>(s: Option<&[T]>) -> Option<Vec<T>>
where
    T: Eq + Hash + Clone,
{
    let s = s?;
    let mut seen: HashSet<T> = HashSet::with_capacity(s.len());
    let mut result: Vec<T> = Vec::with_capacity(s.len());
    for v in s {
        if seen.contains(v) {
            continue;
        }
        seen.insert(v.clone());
        result.push(v.clone());
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- clone_func ----

    #[test]
    fn clone_func_nil_returns_nil() {
        let got = clone_func::<String, Vec<i32>, _>(None, |v: &Vec<i32>| v.clone());
        assert!(got.is_none());
    }

    #[test]
    fn clone_func_deep_copy_with_custom_cloner() {
        let mut src: HashMap<String, Vec<i32>> = HashMap::new();
        src.insert("x".into(), vec![1, 2, 3]);
        src.insert("y".into(), vec![4, 5]);

        let mut got = clone_func(Some(&src), |v: &Vec<i32>| v.clone()).unwrap();
        assert_eq!(got, src);

        // Inner slice mutation independence.
        got.get_mut("x").unwrap()[0] = 99;
        assert_eq!(src["x"][0], 1);
    }

    // ---- clone_nested ----

    #[test]
    fn clone_nested_nil_returns_nil() {
        let got = clone_nested::<String, i32, bool>(None);
        assert!(got.is_none());
    }

    #[test]
    fn clone_nested_empty_returns_empty() {
        let src: HashMap<String, Option<HashMap<i32, bool>>> = HashMap::new();
        let got = clone_nested(Some(&src)).unwrap();
        assert!(got.is_empty());
    }

    #[test]
    fn clone_nested_deep_independence() {
        let mut src: HashMap<i32, Option<HashMap<i32, i64>>> = HashMap::new();
        src.insert(1, Some([(10, 100i64), (20, 200)].into_iter().collect()));
        src.insert(2, Some([(30, 300i64)].into_iter().collect()));

        let mut got = clone_nested(Some(&src)).unwrap();
        assert_eq!(got, src);

        // Inner map mutation independence.
        got.get_mut(&1).unwrap().as_mut().unwrap().insert(10, 999);
        assert_eq!(src[&1].as_ref().unwrap()[&10], 100);

        // New key in clone does not appear in source.
        got.get_mut(&1).unwrap().as_mut().unwrap().insert(99, 1);
        assert!(!src[&1].as_ref().unwrap().contains_key(&99));
    }

    #[test]
    fn clone_nested_nil_inner_maps_preserved() {
        let mut src: HashMap<String, Option<HashMap<String, i32>>> = HashMap::new();
        src.insert("a".into(), None);
        src.insert(
            "b".into(),
            Some([("x".to_string(), 1)].into_iter().collect()),
        );

        let got = clone_nested(Some(&src)).unwrap();
        assert!(got["a"].is_none());
        assert_eq!(
            got["b"],
            Some(
                [("x".to_string(), 1)]
                    .into_iter()
                    .collect::<HashMap<_, _>>()
            )
        );
    }

    // ---- merge_additive ----

    #[test]
    fn merge_additive_nil_src_no_op() {
        let mut dst: HashMap<&str, i32> = [("a", 1)].into_iter().collect();
        let src: HashMap<&str, i32> = HashMap::new();
        merge_additive(Some(&mut dst), &src);
        assert_eq!(dst, [("a", 1)].into_iter().collect());
    }

    #[test]
    fn merge_additive_nil_dst_no_panic() {
        let src: HashMap<&str, i32> = [("a", 1)].into_iter().collect();
        merge_additive::<&str, i32>(None, &src); // must not panic
    }

    #[test]
    fn merge_additive_int() {
        let mut dst: HashMap<&str, i32> = [("a", 1), ("b", 2)].into_iter().collect();
        let src: HashMap<&str, i32> = [("b", 3), ("c", 4)].into_iter().collect();
        merge_additive(Some(&mut dst), &src);
        assert_eq!(dst["a"], 1);
        assert_eq!(dst["b"], 5);
        assert_eq!(dst["c"], 4);
    }

    #[test]
    fn merge_additive_int64() {
        let mut dst: HashMap<i32, i64> = [(1, 100i64)].into_iter().collect();
        let src: HashMap<i32, i64> = [(1, 50i64), (2, 200)].into_iter().collect();
        merge_additive(Some(&mut dst), &src);
        assert_eq!(dst[&1], 150);
        assert_eq!(dst[&2], 200);
    }

    #[test]
    fn merge_additive_float64() {
        let mut dst: HashMap<&str, f64> = [("x", 1.5)].into_iter().collect();
        let src: HashMap<&str, f64> = [("x", 2.5), ("y", 3.0)].into_iter().collect();
        merge_additive(Some(&mut dst), &src);
        assert!((dst["x"] - 4.0).abs() < 0.0001);
        assert!((dst["y"] - 3.0).abs() < 0.0001);
    }

    // ---- merge_nested_additive ----

    #[test]
    fn merge_nested_additive_nil_dst_no_panic() {
        let mut src: HashMap<i32, HashMap<i32, i64>> = HashMap::new();
        src.insert(1, [(10, 5i64)].into_iter().collect());
        merge_nested_additive::<i32, i32, i64>(None, &src); // must not panic
    }

    #[test]
    fn merge_nested_additive_nil_src_no_op() {
        let mut dst: HashMap<i32, HashMap<i32, i64>> = HashMap::new();
        dst.insert(1, [(10, 5i64)].into_iter().collect());
        let src: HashMap<i32, HashMap<i32, i64>> = HashMap::new();
        merge_nested_additive(Some(&mut dst), &src);
        assert_eq!(dst[&1][&10], 5);
    }

    #[test]
    fn merge_nested_additive_empty_inner_src_skipped() {
        let mut dst: HashMap<i32, HashMap<i32, i64>> = HashMap::new();
        let mut src: HashMap<i32, HashMap<i32, i64>> = HashMap::new();
        src.insert(1, HashMap::new());
        merge_nested_additive(Some(&mut dst), &src);
        assert!(
            !dst.contains_key(&1),
            "empty inner map should not allocate dst entry"
        );
    }

    #[test]
    fn merge_nested_additive_new_outer_key() {
        let mut dst: HashMap<i32, HashMap<i32, i64>> = HashMap::new();
        let mut src: HashMap<i32, HashMap<i32, i64>> = HashMap::new();
        src.insert(1, [(10, 100i64), (20, 200)].into_iter().collect());
        merge_nested_additive(Some(&mut dst), &src);
        assert_eq!(dst[&1][&10], 100);
        assert_eq!(dst[&1][&20], 200);
    }

    #[test]
    fn merge_nested_additive_existing_inner_keys() {
        let mut dst: HashMap<i32, HashMap<i32, i64>> = HashMap::new();
        dst.insert(1, [(10, 50i64)].into_iter().collect());
        let mut src: HashMap<i32, HashMap<i32, i64>> = HashMap::new();
        src.insert(1, [(10, 50i64), (20, 200)].into_iter().collect());
        merge_nested_additive(Some(&mut dst), &src);
        assert_eq!(dst[&1][&10], 100);
        assert_eq!(dst[&1][&20], 200);
    }

    #[test]
    fn merge_nested_additive_string_keys() {
        let mut dst: HashMap<&str, HashMap<&str, i32>> = HashMap::new();
        dst.insert("a", [("x", 1)].into_iter().collect());
        let mut src: HashMap<&str, HashMap<&str, i32>> = HashMap::new();
        src.insert("a", [("x", 2), ("y", 3)].into_iter().collect());
        src.insert("b", [("z", 4)].into_iter().collect());
        merge_nested_additive(Some(&mut dst), &src);
        assert_eq!(dst["a"]["x"], 3);
        assert_eq!(dst["a"]["y"], 3);
        assert_eq!(dst["b"]["z"], 4);
    }

    // ---- estimate_map_size ----

    #[test]
    fn estimate_map_size_nil_returns_zero() {
        assert_eq!(estimate_map_size::<&str, i32>(None, 56), 0);
    }

    #[test]
    fn estimate_map_size_empty_returns_zero() {
        let m: HashMap<&str, i32> = HashMap::new();
        assert_eq!(estimate_map_size(Some(&m), 56), 0);
    }

    #[test]
    fn estimate_map_size_single_entry() {
        let m: HashMap<&str, i32> = [("a", 1)].into_iter().collect();
        assert_eq!(estimate_map_size(Some(&m), 56), 56);
    }

    #[test]
    fn estimate_map_size_multiple_entries() {
        let m: HashMap<i32, &str> = [(1, "a"), (2, "b"), (3, "c")].into_iter().collect();
        assert_eq!(estimate_map_size(Some(&m), 56), 3 * 56);
    }

    #[test]
    fn estimate_map_size_zero_entry_bytes_returns_zero() {
        let m: HashMap<&str, i32> = [("a", 1), ("b", 2)].into_iter().collect();
        assert_eq!(estimate_map_size(Some(&m), 0), 0);
    }

    // ---- sorted_keys ----

    #[test]
    fn sorted_keys_nil_returns_nil() {
        assert_eq!(sorted_keys::<i32, ()>(None), None);
    }

    #[test]
    fn sorted_keys_empty_returns_empty() {
        let m: HashMap<i32, &str> = HashMap::new();
        assert_eq!(sorted_keys(Some(&m)), Some(vec![]));
    }

    #[test]
    fn sorted_keys_int_keys_sorted() {
        let m: HashMap<i32, &str> = [(3, "c"), (1, "a"), (2, "b")].into_iter().collect();
        assert_eq!(sorted_keys(Some(&m)), Some(vec![1, 2, 3]));
    }

    #[test]
    fn sorted_keys_string_keys_sorted() {
        let m: HashMap<&str, i32> = [("banana", 2), ("apple", 1), ("cherry", 3)]
            .into_iter()
            .collect();
        assert_eq!(
            sorted_keys(Some(&m)),
            Some(vec!["apple", "banana", "cherry"])
        );
    }

    #[test]
    fn sorted_keys_is_deterministic_across_insertion_orders() {
        let a: HashMap<i32, ()> = [3, 1, 4, 5, 9, 2, 6].into_iter().map(|k| (k, ())).collect();
        let b: HashMap<i32, ()> = [9, 6, 5, 4, 3, 2, 1].into_iter().map(|k| (k, ())).collect();
        assert_eq!(sorted_keys(Some(&a)), Some(vec![1, 2, 3, 4, 5, 6, 9]));
        assert_eq!(sorted_keys(Some(&b)), Some(vec![1, 2, 3, 4, 5, 6, 9]));
    }

    // ---- sort_and_limit ----

    fn descending(a: &i32, b: &i32) -> bool {
        a > b
    }

    #[test]
    fn sort_and_limit_nil_returns_nil() {
        assert_eq!(sort_and_limit(None, descending, 5), None);
    }

    #[test]
    fn sort_and_limit_empty_returns_empty() {
        let got = sort_and_limit(Some(&[]), descending, 5);
        assert_eq!(got, Some(vec![]));
    }

    #[test]
    fn sort_and_limit_limit_greater_than_length() {
        let got = sort_and_limit(Some(&[3, 1, 2]), descending, 10);
        assert_eq!(got, Some(vec![3, 2, 1]));
    }

    #[test]
    fn sort_and_limit_limit_less_than_length() {
        let got = sort_and_limit(Some(&[5, 1, 4, 2, 3]), descending, 3);
        assert_eq!(got, Some(vec![5, 4, 3]));
    }

    #[test]
    fn sort_and_limit_preserves_original() {
        let original = vec![3, 1, 2];
        let _ = sort_and_limit(Some(&original), descending, 2);
        assert_eq!(original, vec![3, 1, 2]);
    }

    #[test]
    fn sort_and_limit_limit_equal_to_length() {
        let got = sort_and_limit(Some(&[2, 1, 3]), descending, 3);
        assert_eq!(got, Some(vec![3, 2, 1]));
    }

    #[test]
    fn sort_and_limit_limit_zero_returns_all() {
        // limit=0 means "no limit" — returns all items sorted.
        let got = sort_and_limit(Some(&[3, 1, 2]), descending, 0);
        assert_eq!(got, Some(vec![3, 2, 1]));
    }

    // ---- build_lookup_set ----

    #[test]
    fn build_lookup_set_nil_returns_nil() {
        assert_eq!(build_lookup_set::<i32>(None), None);
    }

    #[test]
    fn build_lookup_set_empty_returns_empty() {
        let empty: &[i32] = &[];
        let got = build_lookup_set(Some(empty)).unwrap();
        assert!(got.is_empty());
    }

    #[test]
    fn build_lookup_set_no_duplicates() {
        let got = build_lookup_set(Some(&[1, 2, 3])).unwrap();
        assert_eq!(got.len(), 3);
        assert!(got.contains(&1) && got.contains(&2) && got.contains(&3));
    }

    #[test]
    fn build_lookup_set_with_duplicates() {
        let got = build_lookup_set(Some(&[1, 2, 1, 3, 2])).unwrap();
        assert_eq!(got.len(), 3);
        assert!(got.contains(&1) && got.contains(&2) && got.contains(&3));
    }

    #[test]
    fn build_lookup_set_single_element() {
        let got = build_lookup_set(Some(&[42])).unwrap();
        assert_eq!(got.len(), 1);
        assert!(got.contains(&42));
    }

    #[test]
    fn build_lookup_set_string_type() {
        let got = build_lookup_set(Some(&["alpha", "beta", "alpha"])).unwrap();
        assert_eq!(got.len(), 2);
        assert!(got.contains(&"alpha") && got.contains(&"beta"));
    }

    // ---- unique ----

    #[test]
    fn unique_nil_returns_nil() {
        assert_eq!(unique::<i32>(None), None);
    }

    #[test]
    fn unique_empty_returns_empty() {
        let empty: &[i32] = &[];
        let got = unique(Some(empty));
        assert_eq!(got, Some(vec![]));
    }

    #[test]
    fn unique_no_duplicates_unchanged() {
        let got = unique(Some(&[1, 2, 3]));
        assert_eq!(got, Some(vec![1, 2, 3]));
    }

    #[test]
    fn unique_removes_duplicates_preserves_order() {
        let got = unique(Some(&[3, 1, 2, 1, 3, 4, 2]));
        assert_eq!(got, Some(vec![3, 1, 2, 4]));
    }

    #[test]
    fn unique_all_same() {
        let got = unique(Some(&["a", "a", "a"]));
        assert_eq!(got, Some(vec!["a"]));
    }

    #[test]
    fn unique_single_element() {
        let got = unique(Some(&[42]));
        assert_eq!(got, Some(vec![42]));
    }
}
