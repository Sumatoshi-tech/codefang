//! `history/typos` plot sections  (`GenerateStoreSections` →
//! `buildStoreSections` over the `file_typos` store kind).
//!
//! The store records are the pre-sorted [`cf_typos::FileTypoData`] list (reference:
//! `computeFileTypos`, typo-count descending), exactly what
//! `cf_typos::compute_file_typos` produces over the run's report data.

use cf_plotpage::{
    build_bar_chart, get_chart_palette, BarSeries, Hint, Section, SeriesValue, Theme,
};

/// The reference `topFilesLimit`.
const TOP_FILES_LIMIT: usize = 20;

/// The reference `GenerateStoreSections`: `None` is never returned — an empty typo list
/// yields zero sections (the reference `buildStoreSections` returns `nil, nil`).
pub fn sections(file_typos: &[cf_typos::metrics::FileTypoData]) -> Vec<Section> {
    if file_typos.is_empty() {
        return Vec::new();
    }

    let chart = build_bar_chart_from_file_typos(file_typos);

    vec![Section {
        title: "Typo-Prone Files".to_string(),
        subtitle: "Files ranked by number of typo fixes detected in commit history.".to_string(),
        chart: Some(Box::new(chart)),
        hint: Hint {
            title: "How to interpret:".to_string(),
            items: vec![
                "Tall bars = files where typos are frequently fixed".to_string(),
                "Documentation files = expected to have more text-related fixes".to_string(),
                "Code files = typos may indicate hasty commits".to_string(),
                "Look for: Code files with unusually high typo rates".to_string(),
                "Action: Consider adding spell-checking to pre-commit hooks".to_string(),
            ],
        },
    }]
}

/// The reference `buildBarChartFromFileTypos`: top-20 of the pre-sorted records, one
/// warning-colored "Typos" bar series through `BuildBarChart`.
fn build_bar_chart_from_file_typos(
    file_typos: &[cf_typos::metrics::FileTypoData],
) -> cf_plotpage::Chart {
    let limit = file_typos.len().min(TOP_FILES_LIMIT);
    let top = &file_typos[..limit];

    let labels: Vec<String> = top.iter().map(|t| t.file.clone()).collect();
    let series = vec![BarSeries {
        name: "Typos".to_string(),
        data: top.iter().map(|t| SeriesValue::Int(t.typo_count)).collect(),
        color: get_chart_palette(Theme::Dark).semantic.warning.to_string(),
        ..BarSeries::default()
    }];

    build_bar_chart(None, &labels, &series, "Typo Count")
}
