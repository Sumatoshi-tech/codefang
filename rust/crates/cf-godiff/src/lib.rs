//! Line-mode diff engine implementing the `diffmatchpatch` pipeline, scoped to
//! exactly what the burndown analyzer's `FileDiff` provider drives:
//!
//! ```text
//! 1. encode each text as one index per line   (DiffLinesToRunes)
//! 2. Myers diff over the index sequences      (DiffMainRunes, checklines=false)
//! 3. boundary cleanup                         (DiffCleanupSemanticLossless)
//! 4. merge cleanup                            (DiffCleanupMerge)
//! ```
//!
//! The downstream consumer (burndown's diff application) only counts encoded
//! characters per insert/delete segment, where one character == one source
//! line. Therefore this crate operates on the encoded line-index domain (`u32`
//! per line) instead of materializing encoded characters, and emits segments
//! carrying only their *line count*.
//!
//! Compatibility: this is a frozen compatibility implementation — the segment
//! stream must match the reference diff engine exactly, because the resulting
//! added/removed line counts flow into reports. Output bytes are pinned against
//! the reference binary by `rust/tests/compat`; do not alter the algorithms.
//!
//! # Pipeline notes
//!
//! `DiffCleanupSemanticLossless` shifts characters between an edit and its
//! surrounding equalities to land on word boundaries. It never converts an
//! insert into a delete (or vice versa) and never changes the total number of
//! inserted vs. deleted characters on its own — but it repositions edits so the
//! following `DiffCleanupMerge` can factor shared prefix/suffix lines of a
//! delete+insert pair back into equalities (which *does* change the add/remove
//! counts), so both passes are reproduced faithfully.
//!
//! `checklines` is always `false` here (`FileDiff` passes `false`), so the
//! line-mode / `DiffCleanupSemantic` branch of the compute step is unreachable
//! and not implemented. The half-match speedup only fires when a diff timeout
//! is configured; `FileDiff` sets a 1000 ms default timeout, so half-match is
//! active and is implemented. The Myers bisect deadline bail-out is a no-op for
//! the small diffs exercised here and is therefore not time-gated.

#![forbid(unsafe_code)]

/// A diff operation kind, mirroring `diffmatchpatch.Operation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// Lines deleted from the old side.
    Delete,
    /// Lines inserted on the new side.
    Insert,
    /// Lines common to both sides.
    Equal,
}

/// One diff segment carrying its operation and the encoded line indices it
/// covers (each `u32` is one source line). The number of lines is `lines.len()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// The operation kind.
    pub op: Op,
    /// The encoded line indices spanned by this segment.
    pub lines: Vec<u32>,
}

/// Encodes a text into a vector of per-line indices, splitting on `\n` and
/// keeping the trailing newline on each line (the final line keeps no newline
/// if the text does not end in one). Mirrors `diffLinesToStringsMunge`, which
/// reserves index 0 for the empty string and assigns subsequent indices in
/// first-seen order. `next_index` and `line_hash` are shared across both texts.
fn lines_to_indices<'a>(
    text: &'a [u8],
    line_array: &mut Vec<&'a [u8]>,
    line_hash: &mut std::collections::HashMap<&'a [u8], u32>,
) -> Vec<u32> {
    let mut out = Vec::new();
    let mut line_start = 0usize;
    // The reference loop runs while `lineEnd < len(text) - 1` with `lineEnd`
    // starting at -1; tracking the start cursor instead, that condition is
    // exactly `line_start < n`.
    let n = text.len();
    loop {
        if line_start >= n {
            break;
        }
        let line_end = match find_newline(text, line_start) {
            Some(idx) => idx,
            None => n - 1,
        };
        let line = &text[line_start..line_end + 1];
        line_start = line_end + 1;
        let idx = if let Some(&v) = line_hash.get(line) {
            v
        } else {
            line_array.push(line);
            let v = (line_array.len() - 1) as u32;
            line_hash.insert(line, v);
            v
        };
        out.push(idx);
    }
    out
}

/// Index of the next `\n` at or after `from`, mirroring `indexOf(text, "\n", from)`.
fn find_newline(text: &[u8], from: usize) -> Option<usize> {
    text[from..].iter().position(|&b| b == b'\n').map(|p| p + from)
}

/// Computes the line-level diff between `old` and `new`, returning the merged
/// diff segments. `timeout_active` mirrors `DiffTimeout > 0` (controls whether
/// `diffHalfMatch` may fire); FileDiff's default is `true`.
pub fn line_diff(old: &[u8], new: &[u8], timeout_active: bool) -> Vec<Segment> {
    let mut line_array: Vec<&[u8]> = vec![&b""[..]];
    let mut line_hash: std::collections::HashMap<&[u8], u32> = std::collections::HashMap::new();
    let a = lines_to_indices(old, &mut line_array, &mut line_hash);
    let b = lines_to_indices(new, &mut line_array, &mut line_hash);
    // FileDiff: diffs = DiffCleanupMerge(DiffCleanupSemanticLossless(
    //     DiffMainRunes(src, dst, false))). DiffMainRunes already ends with a
    // DiffCleanupMerge; the analyzer then applies lossless + merge again.
    let diffs = diff_main(&a, &b, timeout_active);
    cleanup_merge(cleanup_semantic_lossless(diffs))
}

/// Returns `(lines_added, lines_removed)` for a line diff: the total inserted
/// and deleted line counts. This is how burndown derives per-file deltas —
/// every inserted line contributes +1 added, every deleted line contributes +1
/// removed.
pub fn added_removed(old: &[u8], new: &[u8], timeout_active: bool) -> (i64, i64) {
    let mut added = 0i64;
    let mut removed = 0i64;
    for seg in line_diff(old, new, timeout_active) {
        match seg.op {
            Op::Insert => added += seg.lines.len() as i64,
            Op::Delete => removed += seg.lines.len() as i64,
            Op::Equal => {}
        }
    }
    (added, removed)
}

// ---------------------------------------------------------------------------
// Core diff (operates on encoded line-index slices): DiffMainRunes.
// ---------------------------------------------------------------------------

/// Constants of the diffmatchpatch index-to-character encoding.
const UNICODE_INVALID_RANGE_START: u32 = 0xD800;
const UNICODE_INVALID_RANGE_END: u32 = 0xDFFF;
const UNICODE_INVALID_RANGE_DELTA: u32 = UNICODE_INVALID_RANGE_END - UNICODE_INVALID_RANGE_START + 1;

const ONE_BYTE_BITS: u32 = 7;
const TWO_BYTE_BITS: u32 = 11;
const THREE_BYTE_BITS: u32 = 16;
const FOUR_BYTE_BITS: u32 = 21;

/// Converts a line index (0..~1112060) into the character the reference
/// `intToRune` encoding produces, so `DiffCleanupSemanticLossless`'s
/// boundary-character scoring inspects the same characters the reference
/// implementation does.
fn int_to_rune(i: u32) -> char {
    if i < (1 << ONE_BYTE_BITS) {
        return char::from_u32(i).unwrap_or('\u{FFFD}');
    }
    if i < (1 << TWO_BYTE_BITS) {
        return char::from_u32(i).unwrap_or('\u{FFFD}');
    }
    let mut v = i;
    if i < ((1 << THREE_BYTE_BITS) - UNICODE_INVALID_RANGE_DELTA - 3) {
        if v >= UNICODE_INVALID_RANGE_START {
            v += UNICODE_INVALID_RANGE_DELTA;
        }
        return char::from_u32(v).unwrap_or('\u{FFFD}');
    }
    if i < ((1 << FOUR_BYTE_BITS) - UNICODE_INVALID_RANGE_DELTA - 3) {
        v += UNICODE_INVALID_RANGE_DELTA + 3;
        return char::from_u32(v).unwrap_or('\u{FFFD}');
    }
    '\u{FFFD}'
}

/// Decodes a slice of line indices into a `String` of `int_to_rune` characters,
/// the encoded string the cleanup passes operate on.
fn decode(lines: &[u32]) -> String {
    lines.iter().map(|&i| int_to_rune(i)).collect()
}

fn diff_main(text1: &[u32], text2: &[u32], timeout_active: bool) -> Vec<Segment> {
    if text1 == text2 {
        if text1.is_empty() {
            return Vec::new();
        }
        return vec![Segment { op: Op::Equal, lines: text1.to_vec() }];
    }
    // Trim common prefix.
    let common_prefix = common_prefix_length(text1, text2);
    let prefix = &text1[..common_prefix];
    let t1 = &text1[common_prefix..];
    let t2 = &text2[common_prefix..];
    // Trim common suffix.
    let common_suffix = common_suffix_length(t1, t2);
    let suffix = &t1[t1.len() - common_suffix..];
    let t1 = &t1[..t1.len() - common_suffix];
    let t2 = &t2[..t2.len() - common_suffix];

    let mut diffs = diff_compute(t1, t2, timeout_active);

    if !prefix.is_empty() {
        let mut v = Vec::with_capacity(diffs.len() + 1);
        v.push(Segment { op: Op::Equal, lines: prefix.to_vec() });
        v.append(&mut diffs);
        diffs = v;
    }
    if !suffix.is_empty() {
        diffs.push(Segment { op: Op::Equal, lines: suffix.to_vec() });
    }

    cleanup_merge(diffs)
}

fn diff_compute(text1: &[u32], text2: &[u32], timeout_active: bool) -> Vec<Segment> {
    if text1.is_empty() {
        return vec![Segment { op: Op::Insert, lines: text2.to_vec() }];
    }
    if text2.is_empty() {
        return vec![Segment { op: Op::Delete, lines: text1.to_vec() }];
    }

    let (longtext, shorttext) = if text1.len() > text2.len() {
        (text1, text2)
    } else {
        (text2, text1)
    };

    if let Some(i) = runes_index(longtext, shorttext) {
        let op = if text1.len() > text2.len() { Op::Delete } else { Op::Insert };
        return vec![
            Segment { op, lines: longtext[..i].to_vec() },
            Segment { op: Op::Equal, lines: shorttext.to_vec() },
            Segment { op, lines: longtext[i + shorttext.len()..].to_vec() },
        ];
    }
    if shorttext.len() == 1 {
        return vec![
            Segment { op: Op::Delete, lines: text1.to_vec() },
            Segment { op: Op::Insert, lines: text2.to_vec() },
        ];
    }

    if timeout_active {
        if let Some(hm) = diff_half_match(text1, text2) {
            let (t1a, t1b, t2a, t2b, mid) = hm;
            let mut diffs_a = diff_main(&t1a, &t2a, timeout_active);
            let mut diffs_b = diff_main(&t1b, &t2b, timeout_active);
            diffs_a.push(Segment { op: Op::Equal, lines: mid });
            diffs_a.append(&mut diffs_b);
            return diffs_a;
        }
    }

    diff_bisect(text1, text2)
}

fn diff_bisect(runes1: &[u32], runes2: &[u32]) -> Vec<Segment> {
    let runes1_len = runes1.len() as isize;
    let runes2_len = runes2.len() as isize;
    let max_d = (runes1_len + runes2_len + 1) / 2;
    let v_offset = max_d;
    let v_length = (2 * max_d) as usize;

    let mut v1 = vec![-1isize; v_length];
    let mut v2 = vec![-1isize; v_length];
    v1[(v_offset + 1) as usize] = 0;
    v2[(v_offset + 1) as usize] = 0;

    let delta = runes1_len - runes2_len;
    let front = delta % 2 != 0;
    let mut k1start = 0isize;
    let mut k1end = 0isize;
    let mut k2start = 0isize;
    let mut k2end = 0isize;

    let mut d = 0isize;
    while d < max_d {
        // Walk the front path one step.
        let mut k1 = -d + k1start;
        while k1 <= d - k1end {
            let k1_offset = v_offset + k1;
            let mut x1: isize;
            if k1 == -d || (k1 != d && v1[(k1_offset - 1) as usize] < v1[(k1_offset + 1) as usize]) {
                x1 = v1[(k1_offset + 1) as usize];
            } else {
                x1 = v1[(k1_offset - 1) as usize] + 1;
            }
            let mut y1 = x1 - k1;
            while x1 < runes1_len && y1 < runes2_len && runes1[x1 as usize] == runes2[y1 as usize] {
                x1 += 1;
                y1 += 1;
            }
            v1[k1_offset as usize] = x1;
            if x1 > runes1_len {
                k1end += 2;
            } else if y1 > runes2_len {
                k1start += 2;
            } else if front {
                let k2_offset = v_offset + delta - k1;
                if k2_offset >= 0 && (k2_offset as usize) < v_length && v2[k2_offset as usize] != -1 {
                    let x2 = runes1_len - v2[k2_offset as usize];
                    if x1 >= x2 {
                        return diff_bisect_split(runes1, runes2, x1 as usize, y1 as usize);
                    }
                }
            }
            k1 += 2;
        }
        // Walk the reverse path one step.
        let mut k2 = -d + k2start;
        while k2 <= d - k2end {
            let k2_offset = v_offset + k2;
            let mut x2: isize;
            if k2 == -d || (k2 != d && v2[(k2_offset - 1) as usize] < v2[(k2_offset + 1) as usize]) {
                x2 = v2[(k2_offset + 1) as usize];
            } else {
                x2 = v2[(k2_offset - 1) as usize] + 1;
            }
            let mut y2 = x2 - k2;
            while x2 < runes1_len
                && y2 < runes2_len
                && runes1[(runes1_len - x2 - 1) as usize] == runes2[(runes2_len - y2 - 1) as usize]
            {
                x2 += 1;
                y2 += 1;
            }
            v2[k2_offset as usize] = x2;
            if x2 > runes1_len {
                k2end += 2;
            } else if y2 > runes2_len {
                k2start += 2;
            } else if !front {
                let k1_offset = v_offset + delta - k2;
                if k1_offset >= 0 && (k1_offset as usize) < v_length && v1[k1_offset as usize] != -1 {
                    let x1 = v1[k1_offset as usize];
                    let y1 = v_offset + x1 - k1_offset;
                    let x2m = runes1_len - x2;
                    if x1 >= x2m {
                        return diff_bisect_split(runes1, runes2, x1 as usize, y1 as usize);
                    }
                }
            }
            k2 += 2;
        }
        d += 1;
    }

    vec![
        Segment { op: Op::Delete, lines: runes1.to_vec() },
        Segment { op: Op::Insert, lines: runes2.to_vec() },
    ]
}

fn diff_bisect_split(runes1: &[u32], runes2: &[u32], x: usize, y: usize) -> Vec<Segment> {
    let mut diffs = diff_main(&runes1[..x], &runes2[..y], true);
    let mut diffs_b = diff_main(&runes1[x..], &runes2[y..], true);
    diffs.append(&mut diffs_b);
    diffs
}

// ---------------------------------------------------------------------------
// Half-match speedup: diffHalfMatch.
// ---------------------------------------------------------------------------

type HalfMatch = (Vec<u32>, Vec<u32>, Vec<u32>, Vec<u32>, Vec<u32>);

fn diff_half_match(text1: &[u32], text2: &[u32]) -> Option<HalfMatch> {
    let (longtext, shorttext) = if text1.len() > text2.len() {
        (text1, text2)
    } else {
        (text2, text1)
    };
    if longtext.len() < 4 || shorttext.len() * 2 < longtext.len() {
        return None;
    }

    let hm1 = diff_half_match_i(longtext, shorttext, longtext.len().div_ceil(4));
    let hm2 = diff_half_match_i(longtext, shorttext, longtext.len().div_ceil(2));

    let hm = match (hm1, hm2) {
        (None, None) => return None,
        (Some(h), None) => h,
        (None, Some(h)) => h,
        (Some(h1), Some(h2)) => {
            if h1.4.len() > h2.4.len() {
                h1
            } else {
                h2
            }
        }
    };

    if text1.len() > text2.len() {
        Some(hm)
    } else {
        // Swap halves.
        Some((hm.2, hm.3, hm.0, hm.1, hm.4))
    }
}

fn diff_half_match_i(l: &[u32], s: &[u32], i: usize) -> Option<HalfMatch> {
    let seed = &l[i..i + l.len() / 4];
    let mut best_common_len = 0usize;
    let mut best: Option<HalfMatch> = None;

    let mut j = runes_index_of(s, seed, 0);
    while let Some(jj) = j {
        let prefix_length = common_prefix_length(&l[i..], &s[jj..]);
        let suffix_length = common_suffix_length(&l[..i], &s[..jj]);
        if best_common_len < suffix_length + prefix_length {
            let best_common_a = &s[jj - suffix_length..jj];
            let best_common_b = &s[jj..jj + prefix_length];
            best_common_len = best_common_a.len() + best_common_b.len();
            let mut mid = best_common_a.to_vec();
            mid.extend_from_slice(best_common_b);
            best = Some((
                l[..i - suffix_length].to_vec(),
                l[i + prefix_length..].to_vec(),
                s[..jj - suffix_length].to_vec(),
                s[jj + prefix_length..].to_vec(),
                mid,
            ));
        }
        j = runes_index_of(s, seed, jj + 1);
    }

    if best_common_len * 2 < l.len() {
        return None;
    }
    best
}

// ---------------------------------------------------------------------------
// DiffCleanupMerge (count-affecting cleanup).
// ---------------------------------------------------------------------------

fn cleanup_merge(mut diffs: Vec<Segment>) -> Vec<Segment> {
    diffs.push(Segment { op: Op::Equal, lines: Vec::new() });
    let mut pointer = 0usize;
    let mut count_delete = 0usize;
    let mut count_insert = 0usize;
    let mut text_delete: Vec<u32> = Vec::new();
    let mut text_insert: Vec<u32> = Vec::new();

    while pointer < diffs.len() {
        match diffs[pointer].op {
            Op::Insert => {
                count_insert += 1;
                text_insert.extend_from_slice(&diffs[pointer].lines);
                pointer += 1;
            }
            Op::Delete => {
                count_delete += 1;
                text_delete.extend_from_slice(&diffs[pointer].lines);
                pointer += 1;
            }
            Op::Equal => {
                if count_delete + count_insert > 1 {
                    if count_delete != 0 && count_insert != 0 {
                        // Factor out common prefix.
                        let commonlength = common_prefix_length(&text_insert, &text_delete);
                        if commonlength != 0 {
                            let x = pointer - count_delete - count_insert;
                            if x > 0 && diffs[x - 1].op == Op::Equal {
                                diffs[x - 1]
                                    .lines
                                    .extend_from_slice(&text_insert[..commonlength]);
                            } else {
                                diffs.insert(
                                    0,
                                    Segment {
                                        op: Op::Equal,
                                        lines: text_insert[..commonlength].to_vec(),
                                    },
                                );
                                pointer += 1;
                            }
                            text_insert = text_insert[commonlength..].to_vec();
                            text_delete = text_delete[commonlength..].to_vec();
                        }
                        // Factor out common suffix.
                        let commonlength = common_suffix_length(&text_insert, &text_delete);
                        if commonlength != 0 {
                            let insert_index = text_insert.len() - commonlength;
                            let delete_index = text_delete.len() - commonlength;
                            let mut new_lines = text_insert[insert_index..].to_vec();
                            new_lines.extend_from_slice(&diffs[pointer].lines);
                            diffs[pointer].lines = new_lines;
                            text_insert.truncate(insert_index);
                            text_delete.truncate(delete_index);
                        }
                    }
                    let start = pointer - count_delete - count_insert;
                    let amount = count_delete + count_insert;
                    let mut replacement: Vec<Segment> = Vec::new();
                    if count_delete == 0 {
                        replacement.push(Segment { op: Op::Insert, lines: text_insert.clone() });
                    } else if count_insert == 0 {
                        replacement.push(Segment { op: Op::Delete, lines: text_delete.clone() });
                    } else {
                        replacement.push(Segment { op: Op::Delete, lines: text_delete.clone() });
                        replacement.push(Segment { op: Op::Insert, lines: text_insert.clone() });
                    }
                    let repl_start = if count_insert == 0 {
                        pointer - count_delete
                    } else if count_delete == 0 {
                        pointer - count_insert
                    } else {
                        start
                    };
                    splice(&mut diffs, repl_start, amount, replacement);

                    pointer = pointer - count_delete - count_insert + 1;
                    if count_delete != 0 {
                        pointer += 1;
                    }
                    if count_insert != 0 {
                        pointer += 1;
                    }
                } else if pointer != 0 && diffs[pointer - 1].op == Op::Equal {
                    let merged = diffs[pointer].lines.clone();
                    diffs[pointer - 1].lines.extend_from_slice(&merged);
                    diffs.remove(pointer);
                } else {
                    pointer += 1;
                }
                count_insert = 0;
                count_delete = 0;
                text_delete.clear();
                text_insert.clear();
            }
        }
    }

    if diffs.last().map(|d| d.lines.is_empty()).unwrap_or(false) {
        diffs.pop();
    }

    // Second pass: shift single edits surrounded by equalities.
    let mut changes = false;
    let mut pointer = 1usize;
    while diffs.len() >= 2 && pointer < diffs.len() - 1 {
        if diffs[pointer - 1].op == Op::Equal && diffs[pointer + 1].op == Op::Equal {
            let prev = diffs[pointer - 1].lines.clone();
            let next = diffs[pointer + 1].lines.clone();
            let cur = diffs[pointer].lines.clone();
            if ends_with(&cur, &prev) {
                // Shift edit over the previous equality.
                let mut new_cur = prev.clone();
                new_cur.extend_from_slice(&cur[..cur.len() - prev.len()]);
                diffs[pointer].lines = new_cur;
                let mut new_next = prev.clone();
                new_next.extend_from_slice(&next);
                diffs[pointer + 1].lines = new_next;
                splice(&mut diffs, pointer - 1, 1, Vec::new());
                changes = true;
            } else if starts_with(&cur, &next) {
                diffs[pointer - 1].lines.extend_from_slice(&next);
                let mut new_cur = cur[next.len()..].to_vec();
                new_cur.extend_from_slice(&next);
                diffs[pointer].lines = new_cur;
                splice(&mut diffs, pointer + 1, 1, Vec::new());
                changes = true;
            }
        }
        pointer += 1;
    }

    if changes {
        return cleanup_merge(diffs);
    }
    diffs
}

// ---------------------------------------------------------------------------
// DiffCleanupSemanticLossless (boundary shifting; count-invariant on its own,
// but it repositions edits so the following DiffCleanupMerge can factor out
// shared lines — which DOES change the add/remove totals).
// ---------------------------------------------------------------------------

/// `diffCleanupSemanticScore`: scores the boundary between two encoded-line
/// strings by inspecting the last char of `one` and first char of `two`.
fn cleanup_semantic_score(one: &[u32], two: &[u32]) -> i32 {
    if one.is_empty() || two.is_empty() {
        return 6;
    }
    let char1 = int_to_rune(*one.last().unwrap());
    let char2 = int_to_rune(two[0]);

    let non_alnum1 = !char1.is_ascii_alphanumeric();
    let non_alnum2 = !char2.is_ascii_alphanumeric();
    let whitespace1 = non_alnum1 && is_go_whitespace(char1);
    let whitespace2 = non_alnum2 && is_go_whitespace(char2);
    let linebreak1 = whitespace1 && (char1 == '\r' || char1 == '\n');
    let linebreak2 = whitespace2 && (char2 == '\r' || char2 == '\n');
    // blanklineEndRegex = `\n\r?\n$` on `one`; blanklineStartRegex `^\r?\n\r?\n`
    // on `two`. Operates on the decoded char string.
    let blank_line1 = linebreak1 && blankline_end(&decode(one));
    let blank_line2 = linebreak2 && blankline_start(&decode(two));

    if blank_line1 || blank_line2 {
        5
    } else if linebreak1 || linebreak2 {
        4
    } else if non_alnum1 && !whitespace1 && whitespace2 {
        3
    } else if whitespace1 || whitespace2 {
        2
    } else if non_alnum1 || non_alnum2 {
        1
    } else {
        0
    }
}

/// The reference whitespace class `\s`: ASCII `[\t\n\f\r ]` plus the Unicode
/// whitespace property. For the encoded line-index domain only these matter.
fn is_go_whitespace(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ')
        || c.is_whitespace()
}

/// `\n\r?\n$`: ends with `\n\n` or `\n\r\n`.
fn blankline_end(s: &str) -> bool {
    s.ends_with("\n\n") || s.ends_with("\n\r\n")
}

/// `^\r?\n\r?\n`: starts with `\n\n`, `\r\n\n`, `\n\r\n`, or `\r\n\r\n`.
fn blankline_start(s: &str) -> bool {
    let b = s.as_bytes();
    let starts = |p: &[u8]| b.starts_with(p);
    starts(b"\n\n") || starts(b"\r\n\n") || starts(b"\n\r\n") || starts(b"\r\n\r\n")
}

fn cleanup_semantic_lossless(mut diffs: Vec<Segment>) -> Vec<Segment> {
    let mut pointer = 1usize;
    while diffs.len() >= 2 && pointer < diffs.len() - 1 {
        if diffs[pointer - 1].op == Op::Equal && diffs[pointer + 1].op == Op::Equal {
            let mut equality1 = diffs[pointer - 1].lines.clone();
            let mut edit = diffs[pointer].lines.clone();
            let mut equality2 = diffs[pointer + 1].lines.clone();

            // Shift the edit as far left as possible.
            let common_offset = common_suffix_length(&equality1, &edit);
            if common_offset > 0 {
                let common: Vec<u32> = edit[edit.len() - common_offset..].to_vec();
                equality1.truncate(equality1.len() - common_offset);
                let mut new_edit = common.clone();
                new_edit.extend_from_slice(&edit[..edit.len() - common_offset]);
                edit = new_edit;
                let mut new_eq2 = common;
                new_eq2.extend_from_slice(&equality2);
                equality2 = new_eq2;
            }

            // Step right char-by-char, looking for the best fit.
            let mut best_equality1 = equality1.clone();
            let mut best_edit = edit.clone();
            let mut best_equality2 = equality2.clone();
            let mut best_score =
                cleanup_semantic_score(&equality1, &edit) + cleanup_semantic_score(&edit, &equality2);

            while !edit.is_empty() && !equality2.is_empty() && edit[0] == equality2[0] {
                equality1.push(edit[0]);
                edit.remove(0);
                edit.push(equality2[0]);
                equality2.remove(0);
                let score = cleanup_semantic_score(&equality1, &edit)
                    + cleanup_semantic_score(&edit, &equality2);
                if score >= best_score {
                    best_score = score;
                    best_equality1 = equality1.clone();
                    best_edit = edit.clone();
                    best_equality2 = equality2.clone();
                }
            }

            if diffs[pointer - 1].lines != best_equality1 {
                if !best_equality1.is_empty() {
                    diffs[pointer - 1].lines = best_equality1;
                } else {
                    diffs.remove(pointer - 1);
                    pointer -= 1;
                }
                diffs[pointer].lines = best_edit;
                if !best_equality2.is_empty() {
                    diffs[pointer + 1].lines = best_equality2;
                } else {
                    diffs.remove(pointer + 1);
                    pointer = pointer.saturating_sub(1);
                }
            }
        }
        pointer += 1;
    }
    diffs
}

// ---------------------------------------------------------------------------
// Helpers.
// ---------------------------------------------------------------------------

fn common_prefix_length(a: &[u32], b: &[u32]) -> usize {
    let mut n = 0;
    while n < a.len() && n < b.len() && a[n] == b[n] {
        n += 1;
    }
    n
}

fn common_suffix_length(a: &[u32], b: &[u32]) -> usize {
    let mut n = 0;
    let mut i1 = a.len() as isize;
    let mut i2 = b.len() as isize;
    loop {
        i1 -= 1;
        i2 -= 1;
        if i1 < 0 || i2 < 0 || a[i1 as usize] != b[i2 as usize] {
            return n;
        }
        n += 1;
    }
}

fn runes_index(r1: &[u32], r2: &[u32]) -> Option<usize> {
    if r2.len() > r1.len() {
        return None;
    }
    let last = r1.len() - r2.len();
    for i in 0..=last {
        if &r1[i..i + r2.len()] == r2 {
            return Some(i);
        }
    }
    None
}

fn runes_index_of(target: &[u32], pattern: &[u32], i: usize) -> Option<usize> {
    if i > target.len().saturating_sub(1) && !target.is_empty() {
        return None;
    }
    if target.is_empty() {
        return None;
    }
    if i == 0 {
        return runes_index(target, pattern);
    }
    runes_index(&target[i..], pattern).map(|ind| ind + i)
}

fn ends_with(text: &[u32], suffix: &[u32]) -> bool {
    suffix.len() <= text.len() && &text[text.len() - suffix.len()..] == suffix
}

fn starts_with(text: &[u32], prefix: &[u32]) -> bool {
    prefix.len() <= text.len() && &text[..prefix.len()] == prefix
}

/// Replaces `amount` elements of `slice` starting at `index` with `elements`
/// (the reference `splice` helper).
fn splice(slice: &mut Vec<Segment>, index: usize, amount: usize, elements: Vec<Segment>) {
    slice.splice(index..index + amount, elements);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_text_no_diff() {
        assert_eq!(added_removed(b"a\nb\nc\n", b"a\nb\nc\n", true), (0, 0));
    }

    #[test]
    fn pure_insertion() {
        assert_eq!(added_removed(b"a\nb\n", b"a\nx\nb\n", true), (1, 0));
    }

    #[test]
    fn pure_deletion() {
        assert_eq!(added_removed(b"a\nb\nc\n", b"a\nc\n", true), (0, 1));
    }

    #[test]
    fn replacement() {
        // One line replaced by one line: 1 added, 1 removed.
        assert_eq!(added_removed(b"a\nb\nc\n", b"a\nX\nc\n", true), (1, 1));
    }

    #[test]
    fn append_lines() {
        assert_eq!(added_removed(b"a\n", b"a\nb\nc\n", true), (2, 0));
    }

    #[test]
    fn from_empty() {
        assert_eq!(added_removed(b"", b"a\nb\n", true), (2, 0));
    }

    #[test]
    fn to_empty() {
        assert_eq!(added_removed(b"a\nb\n", b"", true), (0, 2));
    }

    #[test]
    fn many_lines_few_changes() {
        let mut old = String::new();
        let mut new = String::new();
        for i in 0..20 {
            old.push_str(&format!("line{i}\n"));
        }
        for i in 0..20 {
            if i == 5 || i == 12 {
                new.push_str(&format!("CHANGED{i}\n"));
            } else {
                new.push_str(&format!("line{i}\n"));
            }
        }
        assert_eq!(added_removed(old.as_bytes(), new.as_bytes(), true), (2, 2));
    }

    #[test]
    fn many_lines_insert_middle() {
        let mut old = String::new();
        let mut new = String::new();
        for i in 0..50 {
            old.push_str(&format!("line{i}\n"));
        }
        for i in 0..50 {
            new.push_str(&format!("line{i}\n"));
            if i == 25 {
                new.push_str("INSERTED\n");
            }
        }
        assert_eq!(added_removed(old.as_bytes(), new.as_bytes(), true), (1, 0));
    }
}
