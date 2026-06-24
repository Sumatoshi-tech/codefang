//! Faithful port of the reference implementation's `sort.Slice` (pattern-defeating quicksort, `pdqsort`).
//!
//! The reference `sort.Slice` is **not** a stable sort; its exact element
//! ordering for equal keys is an artifact of the pdqsort algorithm. Several
//! codefang report metrics
//! (`complexity.FunctionComplexityMetric.Compute`, `HighRiskFunctionMetric`)
//! call `sort.Slice` with comparators that have ties, so reproducing the
//! emitted byte order requires reproducing pdqsort exactly rather than using a
//! stable sort. This module is a line-for-line port operating on a slice of `T`
//! via a caller-supplied `less(&T, &T) -> bool`.

/// Sorts `data` using the same algorithm and pivot choices as the reference implementation's
/// `sort.Slice(data, |i, j| less(data[i], data[j]))`.
pub fn slice<T, F>(data: &mut [T], mut less: F)
where
    F: FnMut(&T, &T) -> bool,
{
    let n = data.len();
    // limit := bits.Len(uint(length))
    let limit = bits_len(n as u64);
    pdqsort(data, 0, n, limit, &mut less);
}

/// `bits.Len(uint)` — number of bits needed to represent `x` (0 for x==0).
fn bits_len(x: u64) -> i32 {
    (64 - x.leading_zeros()) as i32
}

const MAX_INSERTION: usize = 12;

// sortedHint values.
const UNKNOWN_HINT: i32 = 0;
const INCREASING_HINT: i32 = 1;
const DECREASING_HINT: i32 = 2;

fn insertion_sort<T, F: FnMut(&T, &T) -> bool>(data: &mut [T], a: usize, b: usize, less: &mut F) {
    for i in (a + 1)..b {
        let mut j = i;
        while j > a && less(&data[j], &data[j - 1]) {
            data.swap(j, j - 1);
            j -= 1;
        }
    }
}

fn sift_down<T, F: FnMut(&T, &T) -> bool>(
    data: &mut [T],
    lo: usize,
    hi: usize,
    first: usize,
    less: &mut F,
) {
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

fn heap_sort<T, F: FnMut(&T, &T) -> bool>(data: &mut [T], a: usize, b: usize, less: &mut F) {
    let first = a;
    let lo = 0usize;
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

#[allow(clippy::too_many_lines)]
fn pdqsort<T, F: FnMut(&T, &T) -> bool>(
    data: &mut [T],
    mut a: usize,
    mut b: usize,
    mut limit: i32,
    less: &mut F,
) {
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
        if hint == DECREASING_HINT {
            reverse_range(data, a, b);
            pivot = (b - 1) - (pivot - a);
            hint = INCREASING_HINT;
        }

        if was_balanced
            && was_partitioned
            && hint == INCREASING_HINT
            && partial_insertion_sort(data, a, b, less)
        {
            return;
        }

        // a > 0 && !data.Less(a-1, pivot)
        if a > 0 && !less_idx(data, a - 1, pivot, less) {
            let mid = partition_equal(data, a, b, pivot, less);
            a = mid;
            continue;
        }

        let (mid, already_partitioned) = partition(data, a, b, pivot, less);
        was_partitioned = already_partitioned;

        let left_len = mid - a;
        let right_len = b - mid;
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

#[inline]
fn less_idx<T, F: FnMut(&T, &T) -> bool>(data: &[T], i: usize, j: usize, less: &mut F) -> bool {
    less(&data[i], &data[j])
}

fn partition<T, F: FnMut(&T, &T) -> bool>(
    data: &mut [T],
    a: usize,
    b: usize,
    pivot: usize,
    less: &mut F,
) -> (usize, bool) {
    data.swap(a, pivot);
    let mut i = a + 1;
    let mut j = b - 1;

    while i <= j && less_idx(data, i, a, less) {
        i += 1;
    }
    while i <= j && !less_idx(data, j, a, less) {
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
        while i <= j && less_idx(data, i, a, less) {
            i += 1;
        }
        while i <= j && !less_idx(data, j, a, less) {
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

fn partition_equal<T, F: FnMut(&T, &T) -> bool>(
    data: &mut [T],
    a: usize,
    b: usize,
    pivot: usize,
    less: &mut F,
) -> usize {
    data.swap(a, pivot);
    let mut i = a + 1;
    let mut j = b - 1;

    loop {
        while i <= j && !less_idx(data, a, i, less) {
            i += 1;
        }
        while i <= j && less_idx(data, a, j, less) {
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

fn partial_insertion_sort<T, F: FnMut(&T, &T) -> bool>(
    data: &mut [T],
    a: usize,
    b: usize,
    less: &mut F,
) -> bool {
    const MAX_STEPS: usize = 5;
    const SHORTEST_SHIFTING: usize = 50;
    let mut i = a + 1;
    for _ in 0..MAX_STEPS {
        while i < b && !less_idx(data, i, i - 1, less) {
            i += 1;
        }

        if i == b {
            return true;
        }

        if b - a < SHORTEST_SHIFTING {
            return false;
        }

        data.swap(i, i - 1);

        if i - a >= 2 {
            let mut j = i - 1;
            while j >= 1 {
                if !less_idx(data, j, j - 1, less) {
                    break;
                }
                data.swap(j, j - 1);
                j -= 1;
            }
        }
        if b - i >= 2 {
            let mut j = i + 1;
            while j < b {
                if !less_idx(data, j, j - 1, less) {
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
        let mut random = Xorshift(length as u64);
        let modulus = next_power_of_two(length);

        let base = a + (length / 4) * 2;
        // idx in [base-1, base+1]
        for idx in (base - 1)..=(base + 1) {
            let mut other = (random.next() as usize) & (modulus - 1);
            if other >= length {
                other -= length;
            }
            data.swap(idx, a + other);
        }
    }
}

fn choose_pivot<T, F: FnMut(&T, &T) -> bool>(
    data: &[T],
    a: usize,
    b: usize,
    less: &mut F,
) -> (usize, i32) {
    const SHORTEST_NINTHER: usize = 50;
    const MAX_SWAPS: i32 = 4 * 3;

    let l = b - a;

    let mut swaps: i32 = 0;
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
        0 => (j, INCREASING_HINT),
        MAX_SWAPS => (j, DECREASING_HINT),
        _ => (j, UNKNOWN_HINT),
    }
}

fn order2<T, F: FnMut(&T, &T) -> bool>(
    data: &[T],
    a: usize,
    b: usize,
    swaps: &mut i32,
    less: &mut F,
) -> (usize, usize) {
    if less(&data[b], &data[a]) {
        *swaps += 1;
        (b, a)
    } else {
        (a, b)
    }
}

fn median<T, F: FnMut(&T, &T) -> bool>(
    data: &[T],
    a: usize,
    b: usize,
    c: usize,
    swaps: &mut i32,
    less: &mut F,
) -> usize {
    let (a, b) = order2(data, a, b, swaps, less);
    let (b, c) = order2(data, b, c, swaps, less);
    let (_a, b) = order2(data, a, b, swaps, less);
    let _ = c;
    b
}

fn median_adjacent<T, F: FnMut(&T, &T) -> bool>(
    data: &[T],
    a: usize,
    swaps: &mut i32,
    less: &mut F,
) -> usize {
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
    let shift = bits_len(length as u64) as u32;
    1usize << shift
}

/// The reference implementation's `xorshift uint64` PRNG used by `breakPatterns`.
struct Xorshift(u64);

impl Xorshift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::slice;

    #[test]
    fn matches_basic_descending() {
        let mut v = vec![3, 1, 2, 3, 1];
        slice(&mut v, |a, b| a > b);
        assert_eq!(v, vec![3, 3, 2, 1, 1]);
    }

    #[test]
    fn empty_and_single() {
        let mut v: Vec<i32> = vec![];
        slice(&mut v, |a, b| a < b);
        assert!(v.is_empty());
        let mut v = vec![42];
        slice(&mut v, |a, b| a < b);
        assert_eq!(v, vec![42]);
    }
}
