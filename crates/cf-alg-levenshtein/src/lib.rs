// Copyright (c) 2015, Arbo von Monkiewitsch All rights reserved.
// Use of this source code is governed by a BSD-style
// license.

//! `cf-alg-levenshtein` calculates the Levenshtein edit distance between
//! strings.
//!
//! Used by the `typos` analyzer.
//!
//! * A [`Context`] owns reusable scratch buffers so that repeated calls
//!   perform no heap allocation after the buffers have grown to a sufficient
//!   size.
//! * For strings whose first operand is at most [`MAX_MYERS_LEN`] runes the
//!   bit-parallel Myers algorithm is used (SIMD-within-a-register). The Myers
//!   algorithm is asymmetric, so when the first string is too long but the
//!   second fits the operands are swapped (Levenshtein distance is symmetric).
//! * For longer strings the classic optimised dynamic-programming algorithm is
//!   used as a fallback.
//!
//! Distances are computed over Unicode scalar values ([`char`]s): a multi-byte
//! UTF-8 character counts as a single edit unit.

/// The maximum length, in runes, of the first string for which the
/// bit-parallel Myers algorithm is used. It is limited by the 64-bit word size.
pub const MAX_MYERS_LEN: usize = 64;

/// The exclusive upper bound for runes stored directly in the pattern-match
/// table. Runes below this value are indexed into [`Context::peq`]; runes at or
/// above it fall back to a linear scan.
///
/// Note: although named after ASCII, the value is 256, so the table covers
/// the entire Latin-1 range, not just 7-bit ASCII.
const ASCII_MAX: u32 = 256;

/// `Context` allows calculating the Levenshtein distance via [`Context::distance`].
///
/// It owns reusable scratch buffers, so a single `Context` reused across many
/// calls performs no allocation once its buffers have grown large enough. A
/// `Context` is **not** safe for concurrent use; create one per thread.
#[derive(Debug)]
pub struct Context {
    /// Scratch column buffer for the dynamic-programming fallback.
    int_slice: Vec<i32>,
    /// Scratch rune buffer for the first string.
    rune_buf1: Vec<char>,
    /// Scratch rune buffer for the second string.
    rune_buf2: Vec<char>,
    /// Pattern-match bit-vectors, one `u64` per rune value in `0..ASCII_MAX`.
    ///
    /// `[u64; 256]` is too long for the derived [`Default`] (arrays only derive
    /// it up to length 32), so [`Default`] is implemented by hand below.
    peq: [u64; ASCII_MAX as usize],
}

impl Default for Context {
    fn default() -> Self {
        Self {
            int_slice: Vec::new(),
            rune_buf1: Vec::new(),
            rune_buf2: Vec::new(),
            peq: [0u64; ASCII_MAX as usize],
        }
    }
}

impl Context {
    /// Creates a new, empty `Context`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a scratch `i32` slice of exactly `length` elements, growing the
    /// backing buffer if required.
    fn get_int_slice(&mut self, length: usize) -> &mut [i32] {
        if self.int_slice.len() < length {
            self.int_slice.resize(length, 0);
        }

        &mut self.int_slice[..length]
    }

    /// Calculates the Levenshtein distance between two strings.
    ///
    /// Uses the bit-parallel Myers algorithm for strings up to
    /// [`MAX_MYERS_LEN`] runes and falls back to the optimised
    /// dynamic-programming algorithm for longer strings.
    ///
    /// # Examples
    ///
    /// ```
    /// use cf_alg_levenshtein::Context;
    ///
    /// let mut ctx = Context::new();
    /// assert_eq!(ctx.distance("kitten", "sitting"), 3);
    /// assert_eq!(ctx.distance("Fön", "Föm"), 1);
    /// assert_eq!(ctx.distance("", "abc"), 3);
    /// ```
    pub fn distance(&mut self, str1: &str, str2: &str) -> i32 {
        // Optimization: check simple equality/empty cases first.
        if str1 == str2 {
            return 0;
        }

        if str1.is_empty() {
            return rune_count(str2);
        }

        if str2.is_empty() {
            return rune_count(str1);
        }

        // Fill the context's rune buffers cheaply (no allocation after warm-up).
        fill_runes(&mut self.rune_buf1, str1);
        fill_runes(&mut self.rune_buf2, str2);

        // If s1 fits in 64 bits, use Myers.
        if self.rune_buf1.len() <= MAX_MYERS_LEN {
            return self.distance_myers64_buf1();
        }

        // Myers algorithm is asymmetric. If s2 fits, we can swap.
        // distance(s1, s2) == distance(s2, s1).
        if self.rune_buf2.len() <= MAX_MYERS_LEN {
            return self.distance_myers64_buf2();
        }

        // Fallback to standard DP logic.
        self.distance_dp()
    }

    /// Dynamic-programming fallback over the rune buffers (single column, two
    /// diagonals).
    fn distance_dp(&mut self) -> i32 {
        let len_s1 = self.rune_buf1.len();

        // Snapshot the rune buffers into locals so the column borrow does not
        // conflict with the rune-buffer borrows.
        let s1 = std::mem::take(&mut self.rune_buf1);
        let s2 = std::mem::take(&mut self.rune_buf2);

        let column = self.get_int_slice(len_s1 + 1);

        // column[0] is set at the start of each outer iteration before it is
        // read (len_s2 is non-zero here, handled by the empty checks above).
        for (idx, slot) in column.iter_mut().enumerate().take(len_s1 + 1).skip(1) {
            *slot = idx as i32;
        }

        for (col, &s2_rune) in s2.iter().enumerate() {
            column[0] = (col + 1) as i32;
            let mut lastdiag = col as i32;

            // The inner loop indexes `column` at both `row` and `row + 1` and
            // `s1` at `row`, so a plain index loop is the clearest form here.
            for row in 0..len_s1 {
                let olddiag = column[row + 1];

                let cost = if s1[row] != s2_rune { 1 } else { 0 };

                column[row + 1] = min3(column[row + 1] + 1, column[row] + 1, lastdiag + cost);
                lastdiag = olddiag;
            }
        }

        let result = column[len_s1];

        // Restore the rune buffers so their capacity is retained for reuse.
        self.rune_buf1 = s1;
        self.rune_buf2 = s2;

        result
    }

    /// Myers path when the first rune buffer is the (short) pattern.
    fn distance_myers64_buf1(&mut self) -> i32 {
        let s1 = std::mem::take(&mut self.rune_buf1);
        let s2 = std::mem::take(&mut self.rune_buf2);
        let result = self.distance_myers64(&s1, &s2);
        self.rune_buf1 = s1;
        self.rune_buf2 = s2;
        result
    }

    /// Myers path when the second rune buffer is the (short) pattern (operands
    /// swapped because Myers is asymmetric).
    fn distance_myers64_buf2(&mut self) -> i32 {
        let s1 = std::mem::take(&mut self.rune_buf1);
        let s2 = std::mem::take(&mut self.rune_buf2);
        let result = self.distance_myers64(&s2, &s1);
        self.rune_buf1 = s1;
        self.rune_buf2 = s2;
        result
    }

    /// Calculates the Levenshtein distance using Myers' bit-vector algorithm,
    /// optimised for a first string of at most 64 runes.
    ///
    /// Reference: Hyyrö, H. (2001). "Explaining and extending the bit-parallel
    /// approximate string matching algorithm of Myers".
    fn distance_myers64(&mut self, s1: &[char], s2: &[char]) -> i32 {
        let len1 = s1.len();

        self.init_peq(s1);

        // VP and VN: Vertical Positive and Vertical Negative deltas.
        // Initial VP is all 1s (D[i,j] = i initially, so diff is +1).
        let mut vp: u64 = !0u64;
        let mut vn: u64 = 0;

        // Score is currently len1 (distance of s1 prefix to empty s2 prefix).
        let mut score = len1 as i32;

        // Mask has high bit set at len1-1.
        let mask: u64 = 1u64 << (len1 - 1);

        for &ch in s2 {
            let pm = self.pattern_match(s1, ch);

            // Myers' step update:
            //   D0 = (((PM & VP) + VP) ^ VP) | PM | VN
            //   HP = VN | ~(D0 | VP)
            //   HN = VP & D0
            //
            // The addition deliberately wraps (the algorithm relies on carry
            // propagation); wrapping_add keeps it panic-free under the
            // workspace's overflow-checks profile.
            let mut x_val = pm | vn;
            let d0 = ((vp.wrapping_add(x_val & vp)) ^ vp) | x_val;
            let hn = vp & d0;
            let hp = vn | !(d0 | vp);

            x_val = (hp << 1) | 1;
            vn = x_val & d0;
            vp = (hn << 1) | !(x_val | d0);

            if (hp & mask) != 0 {
                score += 1;
            }

            if (hn & mask) != 0 {
                score -= 1;
            }
        }

        self.clear_peq(s1);

        score
    }

    /// Initializes the pattern-match bit-vectors for `s1`.
    fn init_peq(&mut self, s1: &[char]) {
        for (i, &r) in s1.iter().enumerate() {
            let code = r as u32;
            if code < ASCII_MAX {
                self.peq[code as usize] |= 1u64 << i;
            }
        }
    }

    /// Resets the pattern-match bit-vectors modified by `s1`.
    fn clear_peq(&mut self, s1: &[char]) {
        for &r in s1 {
            let code = r as u32;
            if code < ASCII_MAX {
                self.peq[code as usize] = 0;
            }
        }
    }

    /// Returns a bit-vector with a 1 at each position where `s1[i] == ch`.
    fn pattern_match(&self, s1: &[char], ch: char) -> u64 {
        let code = ch as u32;
        if code < ASCII_MAX {
            return self.peq[code as usize];
        }

        // Fallback for non-ASCII: scan s1.
        let mut pm: u64 = 0;
        for (i, &r) in s1.iter().enumerate() {
            if r == ch {
                pm |= 1u64 << i;
            }
        }

        pm
    }
}

/// Returns the number of Unicode scalar values in `s`.
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
fn rune_count(s: &str) -> i32 {
    s.chars().count() as i32
}

/// Fills `buf` with the runes of `s`, reusing the buffer's existing capacity.
fn fill_runes(buf: &mut Vec<char>, s: &str) {
    buf.clear();
    buf.extend(s.chars());
}

/// Returns the minimum of three `i32` values.
fn min3(a: i32, b: i32, c: i32) -> i32 {
    a.min(b).min(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DISTANCE_TESTS: &[(&str, &str, i32)] = &[
        ("a", "a", 0),
        ("ab", "ab", 0),
        ("ab", "aa", 1),
        ("ab", "aa", 1),
        ("ab", "aaa", 2),
        ("bbb", "a", 3),
        ("kitten", "sitting", 3),
        ("a", "", 1),
        ("", "a", 1),
        ("aa", "aü", 1),
        ("Fön", "Föm", 1),
    ];

    #[test]
    fn test_distance() {
        let mut lev = Context::new();

        for (index, &(first, second, wanted)) in DISTANCE_TESTS.iter().enumerate() {
            let result = lev.distance(first, second);
            assert_eq!(
                result, wanted,
                "{index} \t distance of {first} and {second} should be {wanted} but was {result}."
            );
        }
    }

    const MYERS_TEST_CASES: &[(&str, &str, i32)] = &[
        ("", "a", 1),
        ("a", "", 1),
        ("a", "a", 0),
        ("a", "b", 1),
        ("ab", "ab", 0),
        ("ab", "aa", 1),
        ("ab", "aaa", 2),
        ("kitten", "sitting", 3),
        ("sitting", "kitten", 3),
        ("aaa", "ab", 2),
        ("aa", "aü", 1),
        ("Fön", "Föm", 1),
        ("abc", "def", 3),
        ("x", "xyz", 2),
        ("xyz", "x", 2),
        ("same", "same", 0),
        ("insert", "inser", 1),
        ("inser", "insert", 1),
    ];

    #[test]
    fn test_distance_myers_path() {
        let mut ctx = Context::new();

        for &(s1, s2, wanted) in MYERS_TEST_CASES {
            let got = ctx.distance(s1, s2);
            assert_eq!(
                got, wanted,
                "distance({s1:?}, {s2:?}) = {got}, want {wanted}"
            );
        }
    }

    #[test]
    fn test_distance_myers_path_symmetry() {
        let mut ctx = Context::new();
        let pairs = ["kitten", "sitting", "ab", "aaa", "Fön", "Föm", "a", "xyz"];

        for (i, a) in pairs.iter().enumerate() {
            for (j, b) in pairs.iter().enumerate() {
                if i == j {
                    continue;
                }

                let d1 = ctx.distance(a, b);
                let d2 = ctx.distance(b, a);

                assert_eq!(
                    d1, d2,
                    "distance({a:?}, {b:?}) = {d1} but distance({b:?}, {a:?}) = {d2} (should be equal)"
                );
            }
        }
    }

    #[test]
    fn test_distance_myers_path_at_64_runes() {
        let mut ctx = Context::new();

        // Exactly 64 runes: Myers path.
        let s64 = "a".repeat(64);
        let s64alt = format!("{}b", "a".repeat(63));

        let got = ctx.distance(&s64, &s64alt);
        assert_eq!(got, 1, "distance(64×a, 63×a+b) = {got}, want 1");

        let got = ctx.distance(&s64, &s64);
        assert_eq!(got, 0, "distance(64×a, 64×a) = {got}, want 0");
    }

    #[test]
    fn test_distance_myers_path_non_ascii() {
        let mut ctx = Context::new();

        // Non-ASCII runes exercise the fallback PM scan in distance_myers64.
        let tests: &[(&str, &str, i32)] = &[
            ("αβγ", "αβγ", 0),
            ("αβγ", "αβδ", 1),
            ("Fön", "Föm", 1),
            ("aa", "aü", 1),
        ];

        for &(s1, s2, wanted) in tests {
            let got = ctx.distance(s1, s2);
            assert_eq!(
                got, wanted,
                "distance({s1:?}, {s2:?}) = {got}, want {wanted}"
            );
        }
    }

    #[test]
    fn test_distance_myers_vs_dp_consistency() {
        let mut ctx = Context::new();

        // Strings <= 64 runes use Myers; > 64 use DP. Compare at the boundary.
        let s_short = "kitten";
        let s_long = "x".repeat(100);

        let d_short = ctx.distance(s_short, "sitting");
        assert_eq!(d_short, 3, "short distance = {d_short}, want 3");

        let _ = ctx.distance(&s_long, &s_long);
        // After using long strings, context buffers are grown; the next short
        // call still uses Myers.
        let d_short_again = ctx.distance(s_short, "sitting");
        assert_eq!(
            d_short_again, 3,
            "short distance after long = {d_short_again}, want 3"
        );
    }

    /// Exercises the DP fallback directly: both strings exceed `MAX_MYERS_LEN`
    /// runes, so neither operand fits the Myers path. Cross-checks against a
    /// naive reference implementation.
    #[test]
    fn test_dp_fallback_long_strings() {
        let mut ctx = Context::new();

        let a = format!("{}xyz", "a".repeat(70));
        let b = format!("{}xqz", "a".repeat(70));
        assert!(a.chars().count() > MAX_MYERS_LEN);
        assert!(b.chars().count() > MAX_MYERS_LEN);

        let got = ctx.distance(&a, &b);
        assert_eq!(got, naive_distance(&a, &b));
        assert_eq!(got, 1);

        // Distance to empty must equal the rune count even on the long path.
        let empty = "";
        assert_eq!(ctx.distance(&a, empty), a.chars().count() as i32);
    }

    /// A naive O(n·m) reference Levenshtein implementation over runes, used to
    /// cross-check the optimised paths.
    fn naive_distance(a: &str, b: &str) -> i32 {
        let a: Vec<char> = a.chars().collect();
        let b: Vec<char> = b.chars().collect();
        let n = a.len();
        let m = b.len();
        let mut prev: Vec<i32> = (0..=m as i32).collect();
        let mut cur = vec![0i32; m + 1];

        for i in 1..=n {
            cur[0] = i as i32;
            for j in 1..=m {
                let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
                cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
            }
            std::mem::swap(&mut prev, &mut cur);
        }

        prev[m]
    }

    /// Differential fuzz against the naive reference over a deterministic set of
    /// small ASCII strings spanning both the Myers and DP paths.
    #[test]
    fn test_differential_against_naive() {
        let mut ctx = Context::new();
        let samples = [
            "", "a", "ab", "abc", "abcd", "kitten", "sitting", "flaw", "lawn", "gumbo", "gambol",
            "saturday", "sunday",
        ];

        for &a in &samples {
            for &b in &samples {
                let got = ctx.distance(a, b);
                let want = naive_distance(a, b);
                assert_eq!(got, want, "distance({a:?}, {b:?}) = {got}, naive = {want}");
            }
        }
    }
}
