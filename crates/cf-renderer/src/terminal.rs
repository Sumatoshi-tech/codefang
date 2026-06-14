//! The layout and color helpers the renderer depends on.
//!
//! Terminal output is non-binding/cosmetic per DESIGN.md §2.7, but the helpers
//! reproduce the reference layout byte-for-byte so the renderer tests stay
//! meaningful. See `Cargo.toml` for the consolidation plan with `cf-terminal`.

/// Standard terminal width for rendering.
pub const DEFAULT_WIDTH: usize = 80;

const SCORE_SCALE: f64 = 10.0;
const PERCENT_SCALE: f64 = 100.0;

const SCORE_THRESHOLD_GOOD: f64 = 0.7;
const SCORE_THRESHOLD_FAIR: f64 = 0.4;

/// A terminal color code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    /// Reset to default.
    Reset,
    /// Red (poor).
    Red,
    /// Green (good).
    Green,
    /// Yellow (fair).
    Yellow,
    /// Blue (titles / info).
    Blue,
    /// Gray (muted headers).
    Gray,
}

const ANSI_RESET: &str = "\u{001b}[0m";

fn color_code(color: Color) -> &'static str {
    match color {
        Color::Reset => ANSI_RESET,
        Color::Red => "\u{001b}[31m",
        Color::Green => "\u{001b}[32m",
        Color::Yellow => "\u{001b}[33m",
        Color::Blue => "\u{001b}[34m",
        Color::Gray => "\u{001b}[90m",
    }
}

/// Returns the color for a given `0..1` score.
pub fn color_for_score(score: f64) -> Color {
    if score >= SCORE_THRESHOLD_GOOD {
        Color::Green
    } else if score >= SCORE_THRESHOLD_FAIR {
        Color::Yellow
    } else {
        Color::Red
    }
}

/// Rendering configuration for terminal output.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// Render width in columns.
    pub width: usize,
    /// When `true`, [`Config::colorize`] is a no-op.
    pub no_color: bool,
}

impl Config {
    /// Returns a config initialized with default values.
    pub fn new() -> Self {
        Self {
            width: DEFAULT_WIDTH,
            no_color: false,
        }
    }

    /// Wraps text in ANSI color codes unless `no_color` is set.
    pub fn colorize(&self, text: &str, color: Color) -> String {
        if self.no_color {
            return text.to_string();
        }
        format!("{}{}{}", color_code(color), text, ANSI_RESET)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

/// Formats a `0..1` score as an `"N/10"` label (`{:.0}` rounding).
pub fn format_score(score: f64) -> String {
    format!("{:.0}/10", score * SCORE_SCALE)
}

/// Pads a string with spaces on the right to reach the given width, measured
/// by BYTE length (not rune count) — reference-layout behavior the rendered
/// columns depend on.
pub fn pad_right(s: &str, width: usize) -> String {
    let len = s.len();
    if len >= width {
        return s.to_string();
    }
    let mut out = String::from(s);
    out.extend(std::iter::repeat(' ').take(width - len));
    out
}

/// Returns a horizontal line of `width` box-drawing chars.
pub fn draw_separator(width: usize) -> String {
    "\u{2500}".repeat(width)
}

const BOX_BORDER_WIDTH: usize = 4;
const BOX_CORNER_WIDTH: usize = 2;
const MIN_GAP: usize = 1;


/// Draws a header box (heavy box-drawing characters) with a left-aligned title
/// and right-aligned score.
pub fn draw_header(left: &str, right: &str, width: usize) -> String {
    // Title/right are measured by RAW byte length, which INCLUDES any ANSI
    // color escapes the caller already wrapped them in (e.g.
    // `\x1b[34mHALSTEAD\x1b[0m`). Those escape bytes are part of the width
    // budget, so the visible content is narrower than the border; a
    // visible-width measure here would over-pad colored headers
    // (reference-layout behavior).
    let left_len = left.len();
    let right_len = right.len();

    // `width` expands when the text cannot fit (min required = title bytes +
    // right bytes + border chars (4) + padding (2)), so the box grows rather
    // than overflowing.
    let width = width.max(left_len + right_len + BOX_BORDER_WIDTH + 2);

    let inner_width = width.saturating_sub(BOX_BORDER_WIDTH);
    let mut gap = inner_width
        .saturating_sub(left_len)
        .saturating_sub(right_len);
    if gap < MIN_GAP {
        gap = MIN_GAP;
    }

    let horiz = "\u{2501}".repeat(width.saturating_sub(BOX_CORNER_WIDTH));

    let mut sb = String::new();
    // Top border.
    sb.push('\u{250f}');
    sb.push_str(&horiz);
    sb.push('\u{2513}');
    sb.push('\n');
    // Content line.
    sb.push('\u{2503}');
    sb.push(' ');
    sb.push_str(left);
    sb.extend(std::iter::repeat(' ').take(gap));
    sb.push_str(right);
    sb.push(' ');
    sb.push('\u{2503}');
    sb.push('\n');
    // Bottom border.
    sb.push('\u{2517}');
    sb.push_str(&horiz);
    sb.push('\u{251b}');
    sb
}

const BAR_FILLED: &str = "\u{2588}";
const BAR_EMPTY: &str = "\u{2591}";

fn clamp_filled(value: f64, bar_width: usize) -> usize {
    let mut filled = (value * bar_width as f64) as i64;
    if filled > bar_width as i64 {
        filled = bar_width as i64;
    }
    if filled < 0 {
        filled = 0;
    }
    filled as usize
}

/// Renders a score as a visual progress bar with an `"N/10"` suffix.
pub fn format_score_bar(score: f64, bar_width: usize) -> String {
    let filled = clamp_filled(score, bar_width);
    let empty = bar_width - filled;
    let bar = format!("{}{}", BAR_FILLED.repeat(filled), BAR_EMPTY.repeat(empty));
    format!("[{}] {:.0}/10", bar, score * SCORE_SCALE)
}

/// Renders a labeled percentage bar.
pub fn draw_percent_bar(
    label: &str,
    percent: f64,
    count: i64,
    label_width: usize,
    bar_width: usize,
) -> String {
    let filled = clamp_filled(percent, bar_width);
    let empty = bar_width - filled;
    let bar = format!("{}{}", BAR_FILLED.repeat(filled), BAR_EMPTY.repeat(empty));
    let padded_label = pad_right(label, label_width);
    // Layout contract: the percent value is TRUNCATED (not rounded),
    // right-aligned to width 3, followed by a literal `%`, then TWO spaces
    // before the parenthesized count.
    let pct_value = (percent * PERCENT_SCALE) as i64;
    format!("{padded_label} {bar} {pct_value:>3}%  ({count})")
}

/// Truncates a string to `max_len` runes, appending `"..."` if needed.
pub fn truncate_with_ellipsis(s: &str, max_len: usize) -> String {
    let runes: Vec<char> = s.chars().collect();
    if runes.len() <= max_len {
        return s.to_string();
    }
    const ELLIPSIS: &str = "...";
    if max_len <= ELLIPSIS.len() {
        return runes[..max_len].iter().collect();
    }
    let head: String = runes[..max_len - ELLIPSIS.len()].iter().collect();
    format!("{head}{ELLIPSIS}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_score_rounds_to_n_over_10() {
        assert_eq!(format_score(0.8), "8/10");
        assert_eq!(format_score(0.75), "8/10");
    }

    #[test]
    fn pad_right_pads_and_passes_through() {
        assert_eq!(pad_right("ab", 4), "ab  ");
        assert_eq!(pad_right("abcd", 2), "abcd");
    }

    #[test]
    fn colorize_respects_no_color() {
        let on = Config {
            width: 80,
            no_color: false,
        };
        let off = Config {
            width: 80,
            no_color: true,
        };
        assert_eq!(on.colorize("x", Color::Green), "\u{001b}[32mx\u{001b}[0m");
        assert_eq!(off.colorize("x", Color::Green), "x");
    }

    #[test]
    fn draw_header_has_box_chars() {
        let h = draw_header("T", "S", 40);
        assert!(h.contains('\u{250f}'));
        assert!(h.contains('\u{2517}'));
    }

    #[test]
    fn percent_bar_has_fill_and_percent() {
        let b = draw_percent_bar("L", 0.68, 106, 18, 40);
        assert!(b.contains(BAR_FILLED));
        assert!(b.contains("68%"));
    }
}
