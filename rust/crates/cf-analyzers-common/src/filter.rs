//! Interface-cast filtering helper (`filter.go`).
//!
//! Go's `FilterByInterface` keeps the elements of a slice that successfully
//! type-assert to a target interface, preserving order. In Rust the
//! "cast that may fail" is expressed as a closure returning [`Option`].

/// Returns a new vector containing only those items for which `cast` returns
/// `Some`, preserving input order.
///
/// Mirrors `common.FilterByInterface[T, U]`. The closure plays the role of the
/// Go comma-ok type assertion `u, ok := item.(U)`.
pub fn filter_by_interface<T, U, F>(items: impl IntoIterator<Item = T>, mut cast: F) -> Vec<U>
where
    F: FnMut(T) -> Option<U>,
{
    let mut result = Vec::new();
    for item in items {
        if let Some(u) = cast(item) {
            result.push(u);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    enum Sample {
        Impl(i32),
        Other,
    }

    #[test]
    fn filters_matching_items() {
        let items = vec![
            Sample::Impl(1),
            Sample::Other,
            Sample::Impl(2),
            Sample::Other,
            Sample::Impl(3),
        ];
        let result = filter_by_interface(items, |item| match item {
            Sample::Impl(v) => Some(v),
            Sample::Other => None,
        });
        assert_eq!(result, vec![1, 2, 3]);
    }

    #[test]
    fn empty_input() {
        let items: Vec<Sample> = vec![];
        let result = filter_by_interface(items, |item| match item {
            Sample::Impl(v) => Some(v),
            Sample::Other => None,
        });
        assert!(result.is_empty());
    }

    #[test]
    fn no_matches() {
        let items = vec![Sample::Other, Sample::Other];
        let result = filter_by_interface(items, |item| match item {
            Sample::Impl(v) => Some(v),
            Sample::Other => None,
        });
        assert!(result.is_empty());
    }
}
