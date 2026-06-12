//! Terminal rendering utilities for analyzer CLI output.
//!
//! Helpers used by the renderer, the `analyze` command, and many analyzers to
//! draw headers, separators, progress / percentage bars, score badges, and to
//! colorize and pad text.
//!
//! # Human output only (non-binding)
//!
//! Per the rewrite design (`specs/rust-rewrite/DESIGN.md` §2.7), everything in
//! this crate produces **human-facing** terminal output and is explicitly
//! **non-binding / cosmetic**. It is never used to build machine-format report
//! bytes (json, yaml, ndjson, timeseries, compact, bin); those go through the
//! shared `cf-gojson` / `cf-goyaml` crates. Consequently this crate does not
//! depend on any serialization crate.
//!
//! # Byte length
//!
//! [`pad_right`], [`truncate_with_ellipsis`], and [`draw_header`] all measure
//! string length in **bytes**, not in chars or display columns. This is a
//! deliberate compatibility choice (reference-implementation behavior) and is
//! preserved exactly.

#![forbid(unsafe_code)]

use std::env;

/// Default terminal width when none can be detected.
pub const DEFAULT_WIDTH: i64 = 80;
/// Minimum sensible terminal width.
pub const MIN_WIDTH: i64 = 60;
/// Maximum sensible terminal width.
pub const MAX_WIDTH: i64 = 120;

// ---------------------------------------------------------------------------
// Box-drawing glyphs
// ---------------------------------------------------------------------------

/// Light box horizontal line `─` (U+2500). Used by [`draw_separator`].
pub const BOX_HORIZONTAL: &str = "\u{2500}";
/// Light box vertical line `│` (U+2502).
pub const BOX_VERTICAL: &str = "\u{2502}";
/// Light box top-left corner `┌` (U+250C).
pub const BOX_TOP_LEFT: &str = "\u{250C}";
/// Light box top-right corner `┐` (U+2510).
pub const BOX_TOP_RIGHT: &str = "\u{2510}";
/// Light box bottom-left corner `└` (U+2514).
pub const BOX_BOTTOM_LEFT: &str = "\u{2514}";
/// Light box bottom-right corner `┘` (U+2518).
pub const BOX_BOTTOM_RIGHT: &str = "\u{2518}";
/// Light box cross `┼` (U+253C).
pub const BOX_CROSS: &str = "\u{253C}";
/// Light box vertical-and-left `┤` (U+2524).
pub const BOX_VERTICAL_LEFT: &str = "\u{2524}";

/// Heavy box horizontal line `━` (U+2501).
pub const BOX_HEAVY_HORIZONTAL: &str = "\u{2501}";
/// Heavy box vertical line `┃` (U+2503).
pub const BOX_HEAVY_VERTICAL: &str = "\u{2503}";
/// Heavy box top-left corner `┏` (U+250F).
pub const BOX_HEAVY_TOP_LEFT: &str = "\u{250F}";
/// Heavy box top-right corner `┓` (U+2513).
pub const BOX_HEAVY_TOP_RIGHT: &str = "\u{2513}";
/// Heavy box bottom-left corner `┗` (U+2517).
pub const BOX_HEAVY_BOTTOM_LEFT: &str = "\u{2517}";
/// Heavy box bottom-right corner `┛` (U+251B).
pub const BOX_HEAVY_BOTTOM_RIGHT: &str = "\u{251B}";

/// Rounded box top-left corner `╭` (U+256D).
pub const BOX_ROUND_TOP_LEFT: &str = "\u{256D}";
/// Rounded box top-right corner `╮` (U+256E).
pub const BOX_ROUND_TOP_RIGHT: &str = "\u{256E}";
/// Rounded box bottom-left corner `╰` (U+2570).
pub const BOX_ROUND_BOTTOM_LEFT: &str = "\u{2570}";
/// Rounded box bottom-right corner `╯` (U+256F).
pub const BOX_ROUND_BOTTOM_RIGHT: &str = "\u{256F}";

/// Space around header content.
pub const HEADER_PADDING: i64 = 1;

// ---------------------------------------------------------------------------
// Progress glyphs
// ---------------------------------------------------------------------------

/// Filled progress cell `█` (U+2588).
pub const PROGRESS_FILLED: &str = "\u{2588}";
/// Empty progress cell `░` (U+2591).
pub const PROGRESS_EMPTY: &str = "\u{2591}";

/// Maximum score value for the `N/10` display.
pub const SCORE_MAX: i64 = 10;
/// Multiplier converting a `0..1` fraction to `0..100`.
pub const PERCENT_MULTIPLIER: i64 = 100;

/// Score threshold (inclusive) for "good" coloring.
pub const SCORE_THRESHOLD_GOOD: f64 = 0.8;
/// Score threshold (inclusive) for "fair" coloring.
pub const SCORE_THRESHOLD_FAIR: f64 = 0.5;

// ---------------------------------------------------------------------------
// Text constants
// ---------------------------------------------------------------------------

/// Suffix appended to truncated strings.
pub const ELLIPSIS: &str = "...";
/// Byte length of [`ELLIPSIS`].
pub const ELLIPSIS_LEN: i64 = 3;

// ---------------------------------------------------------------------------
// Config / width detection
// ---------------------------------------------------------------------------

/// Terminal rendering configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Rendering width in columns.
    pub width: i64,
    /// Whether colored output is disabled.
    pub no_color: bool,
}

impl Config {
    /// Create a [`Config`] with defaults derived from the environment.
    ///
    /// `width` is [`detect_width`] and `no_color` is `true` when the `NO_COLOR`
    /// environment variable is set to a non-empty value.
    #[must_use]
    pub fn new() -> Self {
        Self {
            width: detect_width(),
            no_color: env::var("NO_COLOR").is_ok_and(|v| !v.is_empty()),
        }
    }

    /// Wrap `text` in the ANSI codes for `color`, unless [`Config::no_color`]
    /// is set, in which case `text` is returned unchanged.
    ///
    /// [`Color::None`] also returns `text` unchanged regardless of
    /// [`Config::no_color`].
    #[must_use]
    pub fn colorize(&self, text: &str, color: Color) -> String {
        if self.no_color {
            return text.to_string();
        }
        match color {
            Color::None => text.to_string(),
            other => format!("{}{text}{ANSI_RESET}", other.code()),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

/// Detect the terminal width from the `COLUMNS` environment variable, falling
/// back to [`DEFAULT_WIDTH`] when unset or unparseable.
///
/// A value such as `"120"` parses to `120` — it is deliberately **not** clamped
/// to [`MIN_WIDTH`]/[`MAX_WIDTH`]. An invalid value such as `"invalid"` and an
/// empty/unset `COLUMNS` both yield [`DEFAULT_WIDTH`].
#[must_use]
pub fn detect_width() -> i64 {
    match env::var("COLUMNS") {
        Ok(s) if !s.is_empty() => atoi(&s).unwrap_or(DEFAULT_WIDTH),
        _ => DEFAULT_WIDTH,
    }
}

/// Parse a base-10 integer: an optional leading sign followed by ASCII digits,
/// with no surrounding whitespace allowed (`" 12"` fails). Returns `None` on
/// any deviation (e.g. `"invalid"`). These are the exact acceptance rules the
/// CLI has always applied to `COLUMNS`.
fn atoi(s: &str) -> Option<i64> {
    if s.is_empty() {
        return None;
    }
    let bytes = s.as_bytes();
    let (neg, digits) = match bytes[0] {
        b'+' => (false, &bytes[1..]),
        b'-' => (true, &bytes[1..]),
        _ => (false, bytes),
    };
    if digits.is_empty() {
        return None;
    }
    let mut acc: i64 = 0;
    for &b in digits {
        if !b.is_ascii_digit() {
            return None;
        }
        acc = acc.checked_mul(10)?.checked_add(i64::from(b - b'0'))?;
    }
    Some(if neg { -acc } else { acc })
}

// ---------------------------------------------------------------------------
// Color
// ---------------------------------------------------------------------------

/// ANSI reset sequence.
const ANSI_RESET: &str = "\u{1b}[0m";

/// A named terminal color. The discriminants are stable:
/// `None=0, Green=1, Yellow=2, Red=3, Blue=4, Gray=5`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i64)]
pub enum Color {
    /// No color; [`Config::colorize`] returns the text unchanged.
    None = 0,
    /// Green foreground `\x1b[32m`.
    Green = 1,
    /// Yellow foreground `\x1b[33m`.
    Yellow = 2,
    /// Red foreground `\x1b[31m`.
    Red = 3,
    /// Blue foreground `\x1b[34m`.
    Blue = 4,
    /// Gray (bright black) foreground `\x1b[90m`.
    Gray = 5,
}

impl Color {
    /// The raw ANSI escape sequence for this color. [`Color::None`] has no code
    /// (empty string): colorizing with it emits the text with no prefix or
    /// reset.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Green => "\u{1b}[32m",
            Self::Yellow => "\u{1b}[33m",
            Self::Red => "\u{1b}[31m",
            Self::Blue => "\u{1b}[34m",
            Self::Gray => "\u{1b}[90m",
        }
    }
}

/// Pick a color for a normalized score in `[0, 1]`.
///
/// `score >= 0.8` → [`Color::Green`]; `score >= 0.5` → [`Color::Yellow`];
/// otherwise [`Color::Red`].
#[must_use]
pub fn color_for_score(score: f64) -> Color {
    if score >= SCORE_THRESHOLD_GOOD {
        Color::Green
    } else if score >= SCORE_THRESHOLD_FAIR {
        Color::Yellow
    } else {
        Color::Red
    }
}

// ---------------------------------------------------------------------------
// Progress / scores
// ---------------------------------------------------------------------------

/// Draw a progress bar of `width` cells for a fraction `value` in `[0, 1]`.
///
/// `value` is clamped to `[0, 1]` (but `width` is deliberately **not**
/// clamped). The number of filled cells is `value * width` truncated toward
/// zero. Filled cells use [`PROGRESS_FILLED`] and the remaining
/// `width - filled` cells use [`PROGRESS_EMPTY`].
///
/// # Panics
///
/// A negative `width` is out of contract; callers always pass a non-negative
/// width.
#[must_use]
pub fn draw_progress_bar(value: f64, width: i64) -> String {
    // Two sequential comparisons rather than f64::clamp: clamp handles NaN
    // differently, and this output layout is pinned to the long-standing CLI
    // behavior (NaN passes through untouched).
    #[allow(clippy::manual_clamp)]
    let v = {
        let mut v = value;
        if v < 0.0 {
            v = 0.0;
        }
        if v > 1.0 {
            v = 1.0;
        }
        v
    };
    let filled = (v * width as f64) as i64;
    let empty = width - filled;
    let filled = filled.max(0) as usize;
    let empty = empty.max(0) as usize;
    format!(
        "{}{}",
        PROGRESS_FILLED.repeat(filled),
        PROGRESS_EMPTY.repeat(empty)
    )
}

/// Format a normalized score in `[0, 1]` as `"N/10"`.
///
/// `N = round(score * 10)` using round-half-away-from-zero, so
/// `0.75 → "8/10"`.
#[must_use]
pub fn format_score(score: f64) -> String {
    let scaled = (score * SCORE_MAX as f64).round() as i64;
    format!("{scaled}/{SCORE_MAX}")
}

/// Format a score as a bracketed bar followed by the `"N/10"` badge, e.g.
/// `"[████████░░] 8/10"`.
#[must_use]
pub fn format_score_bar(score: f64, bar_width: i64) -> String {
    let bar = draw_progress_bar(score, bar_width);
    let label = format_score(score);
    format!("[{bar}] {label}")
}

/// Draw a labeled percentage bar row.
///
/// Layout (printf-style `"%s %s %3d%%  (%d)"`):
/// * `label` is right-padded to `label_width` **bytes** via [`pad_right`];
/// * `bar` is [`draw_progress_bar`] of `bar_width` cells;
/// * the percentage is `percent * 100` truncated to an integer, printed
///   right-aligned in a field of width 3;
/// * `count` is shown in parentheses, preceded by **two** spaces.
///
/// Example: `"Simple (1-5)     ████████████████░░░░  68%  (68)"`. This is
/// **cosmetic / non-binding** output (DESIGN.md §2.7).
#[must_use]
pub fn draw_percent_bar(
    label: &str,
    percent: f64,
    count: i64,
    label_width: i64,
    bar_width: i64,
) -> String {
    let padded_label = pad_right(label, label_width);
    let bar = draw_progress_bar(percent, bar_width);
    let pct_value = (percent * PERCENT_MULTIPLIER as f64) as i64;
    format!("{padded_label} {bar} {pct_value:>3}%  ({count})")
}

// ---------------------------------------------------------------------------
// Text helpers
// ---------------------------------------------------------------------------

/// Truncate `s` to at most `max_width` **bytes**, appending `"..."` when it is
/// shortened.
///
/// * If `s` has `<= max_width` bytes it is returned unchanged.
/// * If `max_width <= 3`, the result is `max_width` `'.'` characters.
/// * Otherwise the result is the first `max_width - 3` **bytes** of `s`
///   followed by `"..."` (total `max_width` bytes).
///
/// # Panics
///
/// The byte index `max_width - 3` must fall on a UTF-8 character boundary;
/// slicing mid-character panics. Callers pass ASCII-ish labels where this does
/// not occur.
#[must_use]
pub fn truncate_with_ellipsis(s: &str, max_width: i64) -> String {
    let len = s.len() as i64;
    if len <= max_width {
        return s.to_string();
    }
    if max_width <= ELLIPSIS_LEN {
        let n = if max_width < 0 { 0 } else { max_width as usize };
        return ".".repeat(n);
    }
    let keep = (max_width - ELLIPSIS_LEN) as usize;
    format!("{}{ELLIPSIS}", &s[..keep])
}

/// Pad `s` on the right with spaces to reach `width` **bytes**.
///
/// If the byte length of `s` is `>= width`, `s` is returned unchanged (never
/// truncated). Length is measured in bytes, not display columns.
#[must_use]
pub fn pad_right(s: &str, width: i64) -> String {
    let len = s.len() as i64;
    if len >= width {
        return s.to_string();
    }
    format!("{s}{}", " ".repeat((width - len) as usize))
}

// ---------------------------------------------------------------------------
// Box drawing
// ---------------------------------------------------------------------------

/// Draw a thin horizontal separator line of `width` light box-drawing
/// characters ([`BOX_HORIZONTAL`], `─`). A `width <= 0` yields an empty string.
#[must_use]
pub fn draw_separator(width: i64) -> String {
    if width <= 0 {
        return String::new();
    }
    BOX_HORIZONTAL.repeat(width as usize)
}

/// Draw a heavy-bordered three-line section header:
///
/// ```text
/// ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
/// ┃ TITLE                     rightText ┃
/// ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
/// ```
///
/// The requested `width` is first raised to a minimum of
/// `len(title) + len(right_text) + 4 + (HEADER_PADDING * 2)` **bytes** so the
/// content always fits. The inner width is `width - 2` (the two vertical
/// borders). The content region is `inner_width - HEADER_PADDING*2` bytes wide:
/// when `right_text` is empty the title is left-padded via [`pad_right`];
/// otherwise the title and right text are separated by a gap of at least one
/// space. All lengths are byte lengths.
///
/// This is **cosmetic / non-binding** output (DESIGN.md §2.7).
#[must_use]
pub fn draw_header(title: &str, right_text: &str, width: i64) -> String {
    const HEADER_EXTRA_CHARS: i64 = 4; // borders + spacing around title/rightText
    const BORDER_COUNT: i64 = 2; // left and right borders
    const CONTENT_WIDTH_VALUE: i64 = 2;

    let title_len = title.len() as i64;
    let right_len = right_text.len() as i64;

    let min_required = title_len + right_len + HEADER_EXTRA_CHARS + (HEADER_PADDING * 2);
    let width = if width < min_required {
        min_required
    } else {
        width
    };

    let inner_width = width - BORDER_COUNT;

    let top_border = format!(
        "{}{}{}",
        BOX_HEAVY_TOP_LEFT,
        BOX_HEAVY_HORIZONTAL.repeat(inner_width.max(0) as usize),
        BOX_HEAVY_TOP_RIGHT
    );

    let content_width = inner_width - (HEADER_PADDING * CONTENT_WIDTH_VALUE);

    let content = if right_text.is_empty() {
        pad_right(title, content_width)
    } else {
        let gap = (content_width - title_len - right_len).max(1) as usize;
        format!("{title}{}{right_text}", " ".repeat(gap))
    };

    let pad = " ".repeat(HEADER_PADDING.max(0) as usize);
    let content_line = format!("{BOX_HEAVY_VERTICAL}{pad}{content}{pad}{BOX_HEAVY_VERTICAL}");

    let bottom_border = format!(
        "{}{}{}",
        BOX_HEAVY_BOTTOM_LEFT,
        BOX_HEAVY_HORIZONTAL.repeat(inner_width.max(0) as usize),
        BOX_HEAVY_BOTTOM_RIGHT
    );

    format!("{top_border}\n{content_line}\n{bottom_border}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Serialize env-mutating tests so they do not race (the reference test
    // suite isolates env changes per test; Rust tests share the process env).
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Ports reference test `TestDetectWidth_Default`.
    #[test]
    fn test_detect_width_default() {
        let _g = ENV_LOCK.lock().unwrap();
        env::remove_var("COLUMNS");
        assert_eq!(detect_width(), 80);
    }

    /// Ports reference test `TestDetectWidth_FromEnv`.
    #[test]
    fn test_detect_width_from_env() {
        let _g = ENV_LOCK.lock().unwrap();
        env::set_var("COLUMNS", "120");
        assert_eq!(detect_width(), 120);
        env::remove_var("COLUMNS");
    }

    /// Ports reference test `TestDetectWidth_InvalidEnv`.
    #[test]
    fn test_detect_width_invalid_env() {
        let _g = ENV_LOCK.lock().unwrap();
        env::set_var("COLUMNS", "invalid");
        assert_eq!(detect_width(), 80);
        env::remove_var("COLUMNS");
    }

    /// Ports reference test `TestNewConfig_Defaults`.
    #[test]
    fn test_new_config_defaults() {
        let _g = ENV_LOCK.lock().unwrap();
        env::remove_var("COLUMNS");
        env::remove_var("NO_COLOR");
        let cfg = Config::new();
        assert_eq!(cfg.width, 80);
        assert!(!cfg.no_color);
    }

    /// Ports reference test `TestNewConfig_NoColorFromEnv`.
    #[test]
    fn test_new_config_no_color_from_env() {
        let _g = ENV_LOCK.lock().unwrap();
        env::set_var("NO_COLOR", "1");
        let cfg = Config::new();
        assert!(cfg.no_color);
        env::remove_var("NO_COLOR");
    }

    /// Ports reference test `TestDrawProgressBar_Zero`.
    #[test]
    fn test_draw_progress_bar_zero() {
        assert_eq!(draw_progress_bar(0.0, 10), "░░░░░░░░░░");
    }

    /// Ports reference test `TestDrawProgressBar_Full`.
    #[test]
    fn test_draw_progress_bar_full() {
        assert_eq!(draw_progress_bar(1.0, 10), "██████████");
    }

    /// Ports reference test `TestDrawProgressBar_Partial`.
    #[test]
    fn test_draw_progress_bar_partial() {
        assert_eq!(draw_progress_bar(0.7, 10), "███████░░░");
    }

    /// Ports reference test `TestDrawProgressBar_Clamps`.
    #[test]
    fn test_draw_progress_bar_clamps() {
        assert_eq!(draw_progress_bar(-0.5, 10), "░░░░░░░░░░");
        assert_eq!(draw_progress_bar(1.5, 10), "██████████");
    }

    /// Ports reference test `TestFormatScore`.
    #[test]
    fn test_format_score() {
        assert_eq!(format_score(0.0), "0/10");
        assert_eq!(format_score(0.5), "5/10");
        assert_eq!(format_score(0.8), "8/10");
        assert_eq!(format_score(1.0), "10/10");
        assert_eq!(format_score(0.75), "8/10"); // rounds half away from zero
    }

    /// Ports reference test `TestFormatScoreBar`.
    #[test]
    fn test_format_score_bar() {
        assert_eq!(format_score_bar(0.8, 10), "[████████░░] 8/10");
    }

    /// Ports reference test `TestTruncateWithEllipsis_Short`.
    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate_with_ellipsis("hello", 10), "hello");
    }

    /// Ports reference test `TestTruncateWithEllipsis_Exact`.
    #[test]
    fn test_truncate_exact() {
        assert_eq!(truncate_with_ellipsis("hello", 5), "hello");
    }

    /// Ports reference test `TestTruncateWithEllipsis_Long`.
    #[test]
    fn test_truncate_long() {
        assert_eq!(truncate_with_ellipsis("hello world", 8), "hello...");
    }

    /// Ports reference test `TestTruncateWithEllipsis_TooSmall`.
    #[test]
    fn test_truncate_too_small() {
        assert_eq!(truncate_with_ellipsis("hello", 2), "..");
    }

    /// Ports reference test `TestPadRight`.
    #[test]
    fn test_pad_right() {
        assert_eq!(pad_right("hello", 10), "hello     ");
        assert_eq!(pad_right("hello", 5), "hello");
        assert_eq!(pad_right("hello", 3), "hello"); // longer than width: no truncation
        assert_eq!(pad_right("", 5), "     ");
    }

    /// Ports reference test `TestDrawSeparator`.
    #[test]
    fn test_draw_separator() {
        assert_eq!(draw_separator(10), "──────────");
    }

    /// Ports reference test `TestDrawSeparator_Zero`.
    #[test]
    fn test_draw_separator_zero() {
        assert_eq!(draw_separator(0), "");
    }

    /// Ports reference test `TestDrawHeader`.
    #[test]
    fn test_draw_header() {
        let result = draw_header("COMPLEXITY", "Score: 8/10", 40);
        assert!(result.contains("COMPLEXITY"));
        assert!(result.contains("Score: 8/10"));
        assert!(result.contains(BOX_HEAVY_TOP_LEFT));
        assert!(result.contains(BOX_HEAVY_BOTTOM_LEFT));
    }

    /// Ports reference test `TestDrawHeader_TitleOnly`.
    #[test]
    fn test_draw_header_title_only() {
        let result = draw_header("IMPORTS", "", 30);
        assert!(result.contains("IMPORTS"));
    }

    /// Ports reference test `TestColorize_Enabled`.
    #[test]
    fn test_colorize_enabled() {
        let cfg = Config {
            width: 80,
            no_color: false,
        };
        let result = cfg.colorize("hello", Color::Green);
        assert!(result.contains("\u{1b}["));
        assert!(result.contains("hello"));
    }

    /// Ports reference test `TestColorize_Disabled`.
    #[test]
    fn test_colorize_disabled() {
        let cfg = Config {
            width: 80,
            no_color: true,
        };
        let result = cfg.colorize("hello", Color::Green);
        assert!(!result.contains("\u{1b}["));
        assert_eq!(result, "hello");
    }

    /// Ports reference test `TestColorForScore`.
    #[test]
    fn test_color_for_score() {
        assert_eq!(color_for_score(0.9), Color::Green);
        assert_eq!(color_for_score(0.7), Color::Yellow);
        assert_eq!(color_for_score(0.3), Color::Red);
    }

    /// Ports reference test `TestDrawPercentBar`.
    #[test]
    fn test_draw_percent_bar() {
        let result = draw_percent_bar("Simple (1-5)", 0.68, 68, 15, 20);
        assert!(result.contains("Simple (1-5)"));
        assert!(result.contains("68%"));
        assert!(result.contains("(68)"));
        assert!(result.contains(PROGRESS_FILLED));
    }

    // ---- Additional byte-exact tests pinning behavior the reference tests
    //      leave to substring checks (DESIGN.md treats these as non-binding
    //      cosmetics, but exact tests guard against accidental regressions). ----

    /// Color enum codes match the documented ANSI sequences exactly.
    #[test]
    fn test_color_codes_exact() {
        assert_eq!(Color::None.code(), "");
        assert_eq!(Color::Green.code(), "\u{1b}[32m");
        assert_eq!(Color::Yellow.code(), "\u{1b}[33m");
        assert_eq!(Color::Red.code(), "\u{1b}[31m");
        assert_eq!(Color::Blue.code(), "\u{1b}[34m");
        assert_eq!(Color::Gray.code(), "\u{1b}[90m");
    }

    /// Colorize green produces the exact green + text + reset bytes.
    #[test]
    fn test_colorize_exact_bytes() {
        let cfg = Config {
            width: 80,
            no_color: false,
        };
        assert_eq!(cfg.colorize("hi", Color::Green), "\u{1b}[32mhi\u{1b}[0m");
    }

    /// `Color::None` returns text unchanged even when color is enabled.
    #[test]
    fn test_colorize_none_passthrough() {
        let cfg = Config {
            width: 80,
            no_color: false,
        };
        assert_eq!(cfg.colorize("hi", Color::None), "hi");
    }

    /// `draw_percent_bar` exact layout: `"%s %s %3d%%  (%d)"` (two spaces
    /// before `(`, percent right-aligned in width 3).
    #[test]
    fn test_draw_percent_bar_exact() {
        let result = draw_percent_bar("Simple (1-5)", 0.68, 68, 15, 20);
        let expected = format!(
            "{} {} {:>3}%  ({})",
            pad_right("Simple (1-5)", 15),
            draw_progress_bar(0.68, 20),
            68,
            68
        );
        assert_eq!(result, expected);
        // Percent < 100 is space-padded to width 3.
        assert!(result.contains("  68%  (68)"));
    }

    /// `draw_header` full byte-exact layout for title-only.
    #[test]
    fn test_draw_header_exact_title_only() {
        // width=30, title="IMPORTS" (7 bytes), rightText="".
        // min_required = 7 + 0 + 4 + 2 = 13 < 30, so width stays 30.
        // inner_width = 28; content_width = 26; content = pad_right("IMPORTS",26).
        let top = format!(
            "{}{}{}",
            BOX_HEAVY_TOP_LEFT,
            BOX_HEAVY_HORIZONTAL.repeat(28),
            BOX_HEAVY_TOP_RIGHT
        );
        let content = format!(
            "{} {} {}",
            BOX_HEAVY_VERTICAL,
            pad_right("IMPORTS", 26),
            BOX_HEAVY_VERTICAL
        );
        let bottom = format!(
            "{}{}{}",
            BOX_HEAVY_BOTTOM_LEFT,
            BOX_HEAVY_HORIZONTAL.repeat(28),
            BOX_HEAVY_BOTTOM_RIGHT
        );
        let expected = format!("{top}\n{content}\n{bottom}");
        assert_eq!(draw_header("IMPORTS", "", 30), expected);
    }

    /// `draw_header` raises a too-small width to the minimum required.
    #[test]
    fn test_draw_header_min_width_expand() {
        // width=0 forces expansion; title "AB" (2) -> min_required = 2+0+4+2 = 8.
        // inner_width = 6; content_width = 4; pad_right("AB",4) = "AB  ".
        let result = draw_header("AB", "", 0);
        let top = format!(
            "{}{}{}",
            BOX_HEAVY_TOP_LEFT,
            BOX_HEAVY_HORIZONTAL.repeat(6),
            BOX_HEAVY_TOP_RIGHT
        );
        let content = format!("{} {} {}", BOX_HEAVY_VERTICAL, "AB  ", BOX_HEAVY_VERTICAL);
        let bottom = format!(
            "{}{}{}",
            BOX_HEAVY_BOTTOM_LEFT,
            BOX_HEAVY_HORIZONTAL.repeat(6),
            BOX_HEAVY_BOTTOM_RIGHT
        );
        assert_eq!(result, format!("{top}\n{content}\n{bottom}"));
    }

    /// `atoi` acceptance rules for `COLUMNS` parsing.
    #[test]
    fn test_atoi() {
        assert_eq!(atoi("120"), Some(120));
        assert_eq!(atoi("-7"), Some(-7));
        assert_eq!(atoi("+9"), Some(9));
        assert_eq!(atoi("invalid"), None);
        assert_eq!(atoi("12x"), None);
        assert_eq!(atoi(""), None);
        assert_eq!(atoi(" 12"), None); // no whitespace trimming
    }
}
