//! Shared helpers for the history-analyzer plot sections: the reference display
//! formatters (`fmt.Sprintf("%.Nf")`, devs `formatNumber`, burndown
//! `formatInt64`) and a small stat-grid builder over the cf-plotpage
//! components.

use cf_plotpage::components::{BadgeColor, GridLayout, Renderable, Stat};

/// The reference `fmt.Sprintf("%.<prec>f", v)` (round-half-to-even is NOT what the reference implementation does —
/// `fmt` rounds half away from zero via strconv shortest-then-fixed; Rust's
/// `format!("{:.prec$}")` matches the reference implementation's fixed-precision decimal rounding for
/// the float64 values these stats carry).
#[must_use]
pub fn format_float(v: f64, prec: usize) -> String {
    format!("{v:.prec$}")
}

/// Reference devs `formatNumber`: `1.5K` / `2.3M` shorthand above the thousand /
/// million thresholds, plain integer below.
#[must_use]
pub fn format_number(n: i64) -> String {
    if n < 0 {
        return format!("-{}", format_number(-n));
    }
    const MILLION: i64 = 1_000_000;
    const THOUSAND: i64 = 1_000;
    if n >= MILLION {
        // Reference: strconv.FormatFloat(n/1e6, 'f', 1, 64) + "M".
        format!("{:.1}M", n as f64 / MILLION as f64)
    } else if n >= THOUSAND {
        format!("{:.1}K", n as f64 / THOUSAND as f64)
    } else {
        n.to_string()
    }
}

/// Reference devs `formatSignedNumber`: positive values carry a `+` prefix.
#[must_use]
pub fn format_signed_number(n: i64) -> String {
    if n > 0 {
        format!("+{}", format_number(n))
    } else {
        format_number(n)
    }
}

/// reference burndown `formatInt64`: thousands separated by commas (`2,703`).
#[must_use]
pub fn format_int64(n: i64) -> String {
    if n < 0 {
        return format!("-{}", format_uint64(n.unsigned_abs()));
    }
    format_uint64(n as u64)
}

/// reference burndown `formatUint64`.
#[must_use]
pub fn format_uint64(n: u64) -> String {
    const SEP: u64 = 1000;
    if n < SEP {
        return n.to_string();
    }
    format!("{},{:03}", format_uint64(n / SEP), n % SEP)
}

/// Builder for the `plotpage.NewGrid(cols, NewStat(..)...)` pattern the
/// summary sections use.
pub struct GridStats {
    columns: usize,
    items: Vec<Box<dyn Renderable>>,
}

impl GridStats {
    /// New stat grid with the given column count.
    #[must_use]
    pub fn new(columns: usize) -> Self {
        Self {
            columns,
            items: Vec::new(),
        }
    }

    /// Appends a plain stat.
    #[must_use]
    pub fn stat(mut self, label: &str, value: &str) -> Self {
        self.items.push(Box::new(Stat::new(label, value)));
        self
    }

    /// Appends a stat with a trend badge.
    #[must_use]
    pub fn stat_with_trend(
        mut self,
        label: &str,
        value: &str,
        trend: &str,
        color: BadgeColor,
    ) -> Self {
        self.items
            .push(Box::new(Stat::new(label, value).with_trend(trend, color)));
        self
    }

    /// Finishes the grid.
    #[must_use]
    pub fn into_grid(self) -> GridLayout {
        GridLayout::new(self.columns, self.items)
    }
}
