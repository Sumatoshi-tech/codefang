//! Unstable comparator sort (pattern-defeating quicksort, `pdqsort`).
//!
//! This sort is **not stable**: equal elements may be reordered. Several report
//! surfaces (e.g. the static complexity report's `function_complexity` /
//! `high_risk_functions` orderings) expose the exact output permutation —
//! including its tie handling — so the algorithm here is part of the report
//! compatibility contract and is frozen: pdqsort with these exact cutoffs,
//! pivot selection, and `xorshift` pattern-breaking reproduces the reference
//! implementation's permutation bit-for-bit.
//!
//! Compatibility: output ordering is pinned against the reference binary by
//! `rust/tests/compat`; do not swap in `slice::sort_unstable` or alter the
//! constants.
//!
//! Usage: `go_sort_slice(&mut v, |a, b| a.key > b.key)` sorts `v` in place,
//! where `less(&a, &b)` answers "must `a` sort before `b`?".

/// Number of bits required to represent `x` (0 for 0).
fn bit_len(x: u64) -> u32 {
    64 - x.leading_zeros()
}

const MAX_INSERTION: usize = 12;

#[derive(PartialEq, Clone, Copy)]
enum Hint {
    Unknown,
    Increasing,
    Decreasing,
}

/// Sorts `data` in place with the contract-frozen pdqsort using comparator
/// `less` (`less(&a, &b)` true when `a` must sort before `b`).
pub fn go_sort_slice<T, F>(data: &mut [T], less: F)
where
    F: Fn(&T, &T) -> bool,
{
    let length = data.len();
    let limit = bit_len(length as u64) as i32;
    pdqsort(data, 0, length, limit, &less);
}

fn pdqsort<T, F>(data: &mut [T], mut a: usize, mut b: usize, mut limit: i32, less: &F)
where
    F: Fn(&T, &T) -> bool,
{
    let mut was_balanced = true;
    let mut was_partitioned = true;

    loop {
        let length = b - a;

        if length <= MAX_INSERTION {
            insertion_sort(data, a, b, less);
            return;
        }

        if limit == 0 {
            heap_sort(data, a, b, less);
            return;
        }

        if !was_balanced {
            break_patterns(data, a, b);
            limit -= 1;
        }

        let (mut pivot, mut hint) = choose_pivot(data, a, b, less);
        if hint == Hint::Decreasing {
            reverse_range(data, a, b);
            pivot = (b - 1) - (pivot - a);
            hint = Hint::Increasing;
        }

        if was_balanced
            && was_partitioned
            && hint == Hint::Increasing
            && partial_insertion_sort(data, a, b, less)
        {
            return;
        }

        // The element left of the range is >= the pivot, so the pivot is the
        // minimum of the range: split off the run equal to it.
        if a > 0 && !less(&data[a - 1], &data[pivot]) {
            let mid = partition_equal(data, a, b, pivot, less);
            a = mid;
            continue;
        }

        let (mid, already_partitioned) = partition(data, a, b, pivot, less);
        was_partitioned = already_partitioned;

        let (left_len, right_len) = (mid - a, b - mid);
        let balance_threshold = length / 8;
        if left_len < right_len {
            was_balanced = left_len >= balance_threshold;
            pdqsort(data, a, mid, limit, less);
            a = mid + 1;
        } else {
            was_balanced = right_len >= balance_threshold;
            pdqsort(data, mid + 1, b, limit, less);
            b = mid;
        }
    }
}

fn insertion_sort<T, F>(data: &mut [T], a: usize, b: usize, less: &F)
where
    F: Fn(&T, &T) -> bool,
{
    for i in (a + 1)..b {
        let mut j = i;
        while j > a && less(&data[j], &data[j - 1]) {
            data.swap(j, j - 1);
            j -= 1;
        }
    }
}

fn sift_down<T, F>(data: &mut [T], lo: usize, hi: usize, first: usize, less: &F)
where
    F: Fn(&T, &T) -> bool,
{
    let mut root = lo;
    loop {
        let mut child = 2 * root + 1;
        if child >= hi {
            break;
        }
        if child + 1 < hi && less(&data[first + child], &data[first + child + 1]) {
            child += 1;
        }
        if !less(&data[first + root], &data[first + child]) {
            return;
        }
        data.swap(first + root, first + child);
        root = child;
    }
}

fn heap_sort<T, F>(data: &mut [T], a: usize, b: usize, less: &F)
where
    F: Fn(&T, &T) -> bool,
{
    let first = a;
    let lo = 0;
    let hi = b - a;

    let mut i = (hi as isize - 1) / 2;
    while i >= 0 {
        sift_down(data, i as usize, hi, first, less);
        i -= 1;
    }

    let mut i = hi as isize - 1;
    while i >= 0 {
        data.swap(first, first + i as usize);
        sift_down(data, lo, i as usize, first, less);
        i -= 1;
    }
}

fn partition<T, F>(data: &mut [T], a: usize, b: usize, pivot: usize, less: &F) -> (usize, bool)
where
    F: Fn(&T, &T) -> bool,
{
    data.swap(a, pivot);
    let mut i = a + 1;
    let mut j = b - 1;

    while i <= j && less(&data[i], &data[a]) {
        i += 1;
    }
    while i <= j && !less(&data[j], &data[a]) {
        j -= 1;
    }
    if i > j {
        data.swap(j, a);
        return (j, true);
    }
    data.swap(i, j);
    i += 1;
    j -= 1;

    loop {
        while i <= j && less(&data[i], &data[a]) {
            i += 1;
        }
        while i <= j && !less(&data[j], &data[a]) {
            j -= 1;
        }
        if i > j {
            break;
        }
        data.swap(i, j);
        i += 1;
        j -= 1;
    }
    data.swap(j, a);
    (j, false)
}

fn partition_equal<T, F>(data: &mut [T], a: usize, b: usize, pivot: usize, less: &F) -> usize
where
    F: Fn(&T, &T) -> bool,
{
    data.swap(a, pivot);
    let mut i = a + 1;
    let mut j = b - 1;

    loop {
        while i <= j && !less(&data[a], &data[i]) {
            i += 1;
        }
        while i <= j && less(&data[a], &data[j]) {
            j -= 1;
        }
        if i > j {
            break;
        }
        data.swap(i, j);
        i += 1;
        j -= 1;
    }
    i
}

fn partial_insertion_sort<T, F>(data: &mut [T], a: usize, b: usize, less: &F) -> bool
where
    F: Fn(&T, &T) -> bool,
{
    const MAX_STEPS: usize = 5;
    const SHORTEST_SHIFTING: usize = 50;
    let mut i = a + 1;
    for _ in 0..MAX_STEPS {
        while i < b && !less(&data[i], &data[i - 1]) {
            i += 1;
        }
        if i == b {
            return true;
        }
        if b - a < SHORTEST_SHIFTING {
            return false;
        }
        data.swap(i, i - 1);

        // Shift the smaller one to the left.
        if i - a >= 2 {
            let mut j = i - 1;
            while j >= 1 {
                if !less(&data[j], &data[j - 1]) {
                    break;
                }
                data.swap(j, j - 1);
                j -= 1;
            }
        }
        // Shift the greater one to the right.
        if b - i >= 2 {
            let mut j = i + 1;
            while j < b {
                if !less(&data[j], &data[j - 1]) {
                    break;
                }
                data.swap(j, j - 1);
                j += 1;
            }
        }
    }
    false
}

fn break_patterns<T>(data: &mut [T], a: usize, b: usize) {
    let length = b - a;
    if length >= 8 {
        let mut random: u64 = length as u64;
        let modulus = next_power_of_two(length);

        let base = a + (length / 4) * 2;
        for idx in (base - 1)..=(base + 1) {
            // One xorshift step of the deterministic PRNG (seeded with the
            // range length, so the permutation is reproducible).
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            let mut other = (random as usize) & (modulus - 1);
            if other >= length {
                other -= length;
            }
            data.swap(idx, a + other);
        }
    }
}

fn choose_pivot<T, F>(data: &[T], a: usize, b: usize, less: &F) -> (usize, Hint)
where
    F: Fn(&T, &T) -> bool,
{
    const SHORTEST_NINTHER: usize = 50;
    const MAX_SWAPS: i32 = 4 * 3;

    let l = b - a;

    let mut swaps = 0i32;
    let mut i = a + l / 4;
    let mut j = a + l / 4 * 2;
    let mut k = a + l / 4 * 3;

    if l >= 8 {
        if l >= SHORTEST_NINTHER {
            i = median_adjacent(data, i, &mut swaps, less);
            j = median_adjacent(data, j, &mut swaps, less);
            k = median_adjacent(data, k, &mut swaps, less);
        }
        j = median(data, i, j, k, &mut swaps, less);
    }

    match swaps {
        0 => (j, Hint::Increasing),
        MAX_SWAPS => (j, Hint::Decreasing),
        _ => (j, Hint::Unknown),
    }
}

fn order2<T, F>(data: &[T], a: usize, b: usize, swaps: &mut i32, less: &F) -> (usize, usize)
where
    F: Fn(&T, &T) -> bool,
{
    if less(&data[b], &data[a]) {
        *swaps += 1;
        (b, a)
    } else {
        (a, b)
    }
}

fn median<T, F>(
    data: &[T],
    a: usize,
    b: usize,
    c: usize,
    swaps: &mut i32,
    less: &F,
) -> usize
where
    F: Fn(&T, &T) -> bool,
{
    let (a, b) = order2(data, a, b, swaps, less);
    let (b, c) = order2(data, b, c, swaps, less);
    let (_a, b) = order2(data, a, b, swaps, less);
    let _ = c;
    b
}

fn median_adjacent<T, F>(data: &[T], a: usize, swaps: &mut i32, less: &F) -> usize
where
    F: Fn(&T, &T) -> bool,
{
    median(data, a - 1, a, a + 1, swaps, less)
}

fn reverse_range<T>(data: &mut [T], a: usize, b: usize) {
    let mut i = a;
    let mut j = b - 1;
    while i < j {
        data.swap(i, j);
        i += 1;
        j -= 1;
    }
}

fn next_power_of_two(length: usize) -> usize {
    let shift = bit_len(length as u64);
    1usize << shift
}

#[cfg(test)]
mod tests {
    use super::go_sort_slice;

    #[test]
    fn matches_descending_by_key() {
        // Smoke test: sorts descending; ties unspecified but deterministic.
        let mut v = vec![3, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5];
        go_sort_slice(&mut v, |a, b| a > b);
        let mut sorted = v.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        assert_eq!(v, sorted);
    }
}

