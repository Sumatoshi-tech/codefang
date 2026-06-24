//! `history/couples` plot sections.
//! (`GenerateStoreSections` → `buildStoreSections` over the `file_coupling`,
//! `dev_matrix` and `ownership` store kinds).
//!
//! The dev-coupling heatmap section is gated on the `dev_matrix` names, which
//! every `run` pipeline leaves EMPTY (the aggregator's `reversedNames` comes
//! only from a preloaded people dict) — so, like the live reference pages, only the
//! file-couples bar and the ownership pie can render.

use cf_couples::{bucket_ownership, FileCouplingData, FileOwnershipData};
use cf_gojson::GoValue;
use cf_plotpage::echarts::PieData;
use cf_plotpage::echarts::{AxisLabel, BarData, Chart, ChartKind, ItemStyle, Label, XAxis, YAxis};
use cf_plotpage::{build_pie_chart, get_chart_palette, ChartOpts, Hint, Section, Theme};

/// Reference couples plot-section constants.
const LABEL_FONT_SIZE: i64 = 10;
const INNER_LABEL_SIZE: f64 = 9.0;
const BAR_CHART_HEIGHT: &str = "500px";
const PIE_RADIUS: &str = "65%";
const MAX_FILE_COUPLES: usize = 20;
const MAX_PATH_LEN: usize = 30;

/// The reference `buildStoreSections` over the store kinds. `dev_names` is the
/// `dev_matrix` record's name list (empty on every run pipeline; the heatmap
/// section is skipped exactly as the reference implementation skips it).
pub fn sections(
    file_coupling: &[FileCouplingData],
    dev_names: &[String],
    ownership: &[FileOwnershipData],
) -> Vec<Section> {
    let mut result = Vec::new();

    if let Some(chart) = build_file_coupling_bar_chart(file_coupling) {
        result.push(Section {
            title: "Top File Couples".to_string(),
            subtitle: "Most frequently co-changed file pairs across commit history.".to_string(),
            chart: Some(Box::new(chart)),
            hint: Hint {
                title: "How to interpret:".to_string(),
                items: vec![
                    "Tall bars = file pairs that frequently change together".to_string(),
                    "Cross-package coupling may indicate architectural issues".to_string(),
                    "Test files coupled with implementation is expected and healthy".to_string(),
                    "Action: Consider extracting shared logic or merging tightly coupled files"
                        .to_string(),
                ],
            },
        });
    }

    // Section 2 (developer coupling heatmap) requires dev_matrix names; the
    // run pipelines never populate them, so this is unreachable in practice —
    // the gate is kept for reference parity.
    debug_assert!(dev_names.is_empty(), "run pipelines never carry dev names");

    if let Some(chart) = build_ownership_pie_chart(ownership) {
        result.push(Section {
            title: "File Ownership Distribution".to_string(),
            subtitle: "How files are distributed by number of contributors.".to_string(),
            chart: Some(Box::new(chart)),
            hint: Hint {
                title: "How to interpret:".to_string(),
                items: vec![
                    "Single owner = bus factor risk if that person leaves".to_string(),
                    "Many owners = potential coordination overhead".to_string(),
                    "2-3 owners is often the healthy sweet spot".to_string(),
                    "Action: Review single-owner files for knowledge sharing opportunities"
                        .to_string(),
                ],
            },
        });
    }

    result
}

/// The reference `truncatePath`.
fn truncate_path(path: &str) -> String {
    if path.len() <= MAX_PATH_LEN {
        return path.to_string();
    }
    format!("...{}", &path[path.len() - MAX_PATH_LEN + 3..])
}

/// The reference `buildFileCouplingBarChartFromData`: horizontal bar of the top couples
/// (reversed into ascending display order); `None` when there are no pairs.
fn build_file_coupling_bar_chart(couples: &[FileCouplingData]) -> Option<Chart> {
    if couples.is_empty() {
        return None;
    }

    let shown = couples.len().min(MAX_FILE_COUPLES);
    let mut labels: Vec<String> = vec![String::new(); shown];
    let mut values: Vec<GoValue> = vec![GoValue::Null; shown];
    for (i, cp) in couples[..shown].iter().enumerate() {
        labels[shown - 1 - i] = format!(
            "{} \u{2194} {}",
            truncate_path(&cp.file1),
            truncate_path(&cp.file2)
        );
        values[shown - 1 - i] = BarData {
            value: Some(GoValue::Int(cp.co_changes)),
            ..BarData::default()
        }
        .value();
    }

    let co = ChartOpts::default_dark();
    let palette = get_chart_palette(Theme::Dark);

    let mut bar = Chart::new(ChartKind::Bar);
    let (w, h, bg, theme) = co.init("100%", BAR_CHART_HEIGHT);
    bar.set_init(&w, &h, &bg, &theme);
    bar.tooltip = co.tooltip("axis");
    bar.grid = vec![cf_plotpage::echarts::Grid {
        left: "35%".to_string(),
        top: "40".to_string(),
        right: "5%".to_string(),
        bottom: "10%".to_string(),
        ..cf_plotpage::echarts::Grid::default()
    }];
    bar.x_axis = XAxis {
        type_: "value".to_string(),
        axis_label: Some(AxisLabel {
            font_size: LABEL_FONT_SIZE,
            color: co.text_muted_color().to_string(),
            ..AxisLabel::default()
        }),
        ..XAxis::default()
    };
    bar.y_axis = YAxis {
        type_: "category".to_string(),
        data: Some(GoValue::Array(
            labels.iter().map(|l| GoValue::Str(l.clone())).collect(),
        )),
        axis_label: Some(AxisLabel {
            font_size: LABEL_FONT_SIZE,
            color: co.text_muted_color().to_string(),
            ..AxisLabel::default()
        }),
        ..YAxis::default()
    };

    let series = bar.add_series("Co-changes", GoValue::Array(values));
    series.item_style = Some(ItemStyle {
        color: palette.primary[0].to_string(),
        ..ItemStyle::default()
    });
    series.label = Some(Label {
        show: Some(true),
        position: "right".to_string(),
        color: co.text_muted_color().to_string(),
        font_size: INNER_LABEL_SIZE,
        ..Label::default()
    });

    Some(bar)
}

/// The reference `buildOwnershipPieChartFromData`: bucketed contributor counts; `None`
/// when there is no ownership data.
fn build_ownership_pie_chart(ownership: &[FileOwnershipData]) -> Option<Chart> {
    if ownership.is_empty() {
        return None;
    }

    let buckets = bucket_ownership(ownership);
    let palette = get_chart_palette(Theme::Dark);

    let bucket_colors = [
        palette.semantic.bad,
        palette.semantic.good,
        palette.semantic.warning,
        palette.primary[0],
    ];

    let pie_data: Vec<PieData> = buckets
        .iter()
        .zip(bucket_colors.iter())
        .map(|(b, color)| PieData {
            name: b.label.clone(),
            value: Some(GoValue::Int(i64::from(b.count))),
            item_style: Some(ItemStyle {
                color: (*color).to_string(),
                ..ItemStyle::default()
            }),
            ..PieData::default()
        })
        .collect();

    Some(build_pie_chart(None, "Ownership", pie_data, PIE_RADIUS))
}
