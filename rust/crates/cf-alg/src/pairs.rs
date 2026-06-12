//! Unordered-pair enumeration.

/// Calls `visit` for every unique pair `(i, j)` where `0 <= i < j < n`.
///
/// The total number of calls is `C(n, 2) = n*(n-1)/2`. Does nothing when
/// `n < 2`. Pairs are visited in ascending `i`, then ascending `j` order;
/// downstream callers rely on this order.
pub fn for_each_pair<F: FnMut(usize, usize)>(n: usize, mut visit: F) {
    for i in 0..n {
        for j in (i + 1)..n {
            visit(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_elements() {
        let mut count = 0;
        for_each_pair(0, |_, _| count += 1);
        assert_eq!(count, 0);
    }

    #[test]
    fn one_element() {
        let mut count = 0;
        for_each_pair(1, |_, _| count += 1);
        assert_eq!(count, 0);
    }

    #[test]
    fn two_elements() {
        let mut pairs = Vec::new();
        for_each_pair(2, |i, j| pairs.push((i, j)));
        assert_eq!(pairs, vec![(0, 1)]);
    }

    #[test]
    fn three_elements() {
        let mut pairs = Vec::new();
        for_each_pair(3, |i, j| pairs.push((i, j)));
        assert_eq!(pairs, vec![(0, 1), (0, 2), (1, 2)]);
    }

    #[test]
    fn five_elements_count() {
        const N: usize = 5;
        const EXPECTED: usize = N * (N - 1) / 2;
        let mut count = 0;
        for_each_pair(N, |_, _| count += 1);
        assert_eq!(count, EXPECTED);
    }

    #[test]
    fn ordering_invariant() {
        const N: usize = 4;
        for_each_pair(N, |i, j| {
            assert!(i < j, "i must be less than j");
            assert!(j < N, "j must be less than n");
        });
    }
}
