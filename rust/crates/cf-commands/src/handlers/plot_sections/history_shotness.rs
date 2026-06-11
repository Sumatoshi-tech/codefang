//! `history/shotness` plot sections — port of Go
//! `internal/analyzers/shotness/store_reader.go` + `plot.go`
//! (`GenerateStoreSections` over the `node_data` store kind, which is exactly
//! the run report's key-sorted `(NodeSummary, Counter)` pairs).

use std::collections::HashMap;

use cf_gojson::GoValue;
use cf_plotpage::echarts::{
    AxisLabel, AxisLine, BarData, Chart, ChartKind, HeatMapData, ItemStyle, Label, LineStyle,
    SplitArea, TextStyle, TreeMapLevel, TreeMapNode, UpperLabel, VisualMap, VisualMapInRange,
    XAxis, YAxis,
};
use cf_plotpage::{get_chart_palette, ChartOpts, Hint, Section, Theme};
use cf_shotness::NodeSummary;

use crate::handlers::go_sort;

/// shotness plot.go constants.
const TOP_N_NODES: usize = 20;
const MAX_FILES: usize = 30;
const ROTATE_DEGREES: f64 = 60.0;
const LABEL_FONT_SIZE: i64 = 10;
const INNER_LABEL_SIZE: f64 = 9.0;
const TREE_MAP_HEIGHT: &str = "550px";
const HEAT_MAP_HEIGHT: &str = "650px";
const TREE_MAP_LEAF_DEPTH: i64 = 2;
const BORDER_WIDTH_1: f64 = 1.0;
const BORDER_WIDTH_2: f64 = 2.0;
const MIN_HEAT_MAP_NODES: usize = 2;

/// Go `GenerateStoreSections`: zero nodes yield zero sections.
pub fn sections(nodes: &[NodeSummary], counters: &[HashMap<usize, i64>]) -> Vec<Section> {
    if nodes.is_empty() {
        return Vec::new();
    }

    let co = ChartOpts::default_dark();
    let palette = get_chart_palette(Theme::Dark);

    vec![
        tree_map_section(nodes, counters, &co),
        heat_map_section(nodes, counters, &co),
        bar_chart_section(nodes, counters, &co, &palette),
    ]
}

/// Go `treeMapSection`.
fn tree_map_section(
    nodes: &[NodeSummary],
    counters: &[HashMap<usize, i64>],
    co: &ChartOpts,
) -> Section {
    Section {
        title: "Code Hotness TreeMap".to_string(),
        subtitle: "Hierarchical view: Files -> Functions. Rectangle size = change frequency."
            .to_string(),
        chart: Some(Box::new(create_tree_map(nodes, counters, co))),
        hint: Hint {
            title: "How to interpret:".to_string(),
            items: vec![
                "Large rectangles = frequently changed code (potential maintenance burden)"
                    .to_string(),
                "Color intensity = relative hotness within the file".to_string(),
                "Click on a file to drill down and see individual functions".to_string(),
                "Look for: Small files with many hot functions".to_string(),
            ],
        },
    }
}

/// Go `heatMapSection`.
fn heat_map_section(
    nodes: &[NodeSummary],
    counters: &[HashMap<usize, i64>],
    co: &ChartOpts,
) -> Section {
    let chart: Option<Box<dyn cf_plotpage::Renderable>> =
        create_heat_map(nodes, counters, co).map(|c| Box::new(c) as Box<dyn cf_plotpage::Renderable>);
    Section {
        title: "Function Coupling Matrix".to_string(),
        subtitle: "Co-change frequency between functions. Diagonal = self, off-diagonal = coupled."
            .to_string(),
        chart,
        hint: Hint {
            title: "How to interpret:".to_string(),
            items: vec![
                "Diagonal (dark green) = how often each function changes independently".to_string(),
                "Off-diagonal cells = functions that change together in same commits".to_string(),
                "High off-diagonal = tight coupling (may indicate hidden dependency)".to_string(),
                "Look for: Functions from different files changing together".to_string(),
            ],
        },
    }
}

/// Go `barChartSection`.
fn bar_chart_section(
    nodes: &[NodeSummary],
    counters: &[HashMap<usize, i64>],
    co: &ChartOpts,
    palette: &cf_plotpage::ChartPalette,
) -> Section {
    Section {
        title: "Top Hot Functions".to_string(),
        subtitle: "Ranking of most frequently changed functions with coupling information."
            .to_string(),
        chart: Some(Box::new(create_bar_chart(nodes, counters, co, palette))),
        hint: Hint {
            title: "How to interpret:".to_string(),
            items: vec![
                "Blue bars (Self Changes) = direct modifications to this function".to_string(),
                "Green bars (Coupled Changes) = changes alongside other functions".to_string(),
                "High blue + low green = isolated changes (frequently bugfixed)".to_string(),
                "High blue + high green = central/core function affecting many others".to_string(),
                "Action: Top functions are candidates for additional test coverage".to_string(),
            ],
        },
    }
}

/// Go `filepath.Base`.
fn go_filepath_base(path: &str) -> &str {
    if path.is_empty() {
        return ".";
    }
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return "/";
    }
    match trimmed.rfind('/') {
        Some(pos) => &trimmed[pos + 1..],
        None => trimmed,
    }
}

/// Go `createTreeMap` (+ `buildFileHierarchy` / `buildRootNodes`).
fn create_tree_map(
    nodes: &[NodeSummary],
    counters: &[HashMap<usize, i64>],
    co: &ChartOpts,
) -> Chart {
    // buildFileHierarchy: per-file child nodes + totals. Go iterates the maps
    // randomly when building rootNodes; file-name-ascending is the
    // deterministic stand-in (ties at equal Value are Go-variant).
    let mut file_map: std::collections::BTreeMap<&str, Vec<TreeMapNode>> =
        std::collections::BTreeMap::new();
    let mut file_totals: std::collections::BTreeMap<&str, i64> = std::collections::BTreeMap::new();
    for (idx, node) in nodes.iter().enumerate() {
        let count = counters.get(idx).and_then(|c| c.get(&idx)).copied().unwrap_or(0);
        file_map.entry(node.file.as_str()).or_default().push(TreeMapNode {
            name: node.name.clone(),
            value: count,
            children: Vec::new(),
        });
        *file_totals.entry(node.file.as_str()).or_insert(0) += count;
    }

    let mut root_nodes: Vec<TreeMapNode> = Vec::with_capacity(file_map.len());
    for (file, mut children) in file_map {
        go_sort::slice(&mut children, |a, b| a.value > b.value);
        root_nodes.push(TreeMapNode {
            name: go_filepath_base(file).to_string(),
            value: file_totals[file],
            children,
        });
    }
    go_sort::slice(&mut root_nodes, |a, b| a.value > b.value);
    root_nodes.truncate(MAX_FILES);

    let mut tm = Chart::new(ChartKind::TreeMap);
    let (w, h, bg, theme) = co.init("100%", TREE_MAP_HEIGHT);
    tm.set_init(&w, &h, &bg, &theme);
    tm.tooltip = co.tooltip("item");

    let data = GoValue::Array(root_nodes.iter().map(TreeMapNode::value).collect());
    let series = tm.add_series("Hotness", data);
    series.animation = Some(true);
    series.roam = Some(true);
    series.leaf_depth = TREE_MAP_LEAF_DEPTH;
    // NOTE: Go's WithTreeMapOpts drops both `Label` and `ColorMappingBy` from
    // opts.TreeMapChart — only Animation/LeafDepth/Roam/Levels/UpperLabel and
    // the four offsets reach the series.
    series.levels = vec![
        TreeMapLevel {
            upper_label: Some(UpperLabel {
                show: Some(true),
                ..UpperLabel::default()
            }),
            item_style: Some(ItemStyle {
                border_color: co.grid_color().to_string(),
                border_width: BORDER_WIDTH_2,
                gap_width: BORDER_WIDTH_2,
                ..ItemStyle::default()
            }),
            ..TreeMapLevel::default()
        },
        TreeMapLevel {
            color_saturation: vec![0.3, 0.6],
            item_style: Some(ItemStyle {
                border_color: co.axis_color().to_string(),
                border_width: BORDER_WIDTH_1,
                gap_width: BORDER_WIDTH_1,
                ..ItemStyle::default()
            }),
            ..TreeMapLevel::default()
        },
    ];
    series.upper_label = Some(UpperLabel {
        show: Some(true),
        color: co.text_color().to_string(),
    });
    series.left = "2%".to_string();
    series.right = "2%".to_string();
    series.top = "10".to_string();
    series.bottom = "2%".to_string();

    tm
}

/// One active node (Go `activeNode`).
struct ActiveNode {
    idx: usize,
    name: String,
    count: i64,
}

/// Go `createHeatMap` (+ `getActiveNodes` / `buildHeatMapData`); `None` below
/// two active nodes.
fn create_heat_map(
    nodes: &[NodeSummary],
    counters: &[HashMap<usize, i64>],
    co: &ChartOpts,
) -> Option<Chart> {
    let mut actives: Vec<ActiveNode> = Vec::new();
    for (idx, counter) in counters.iter().enumerate() {
        let self_count = counter.get(&idx).copied().unwrap_or(0);
        if self_count > 0 {
            actives.push(ActiveNode {
                idx,
                name: nodes[idx].name.clone(),
                count: self_count,
            });
        }
    }
    go_sort::slice(&mut actives, |a, b| a.count > b.count);
    actives.truncate(TOP_N_NODES);

    if actives.len() < MIN_HEAT_MAP_NODES {
        return None;
    }

    let names: Vec<String> = actives.iter().map(|a| a.name.clone()).collect();

    let mut data: Vec<GoValue> = Vec::with_capacity(actives.len() * actives.len());
    let mut max_val: f64 = 0.0;
    for (row, row_active) in actives.iter().enumerate() {
        for (col, col_active) in actives.iter().enumerate() {
            let val = if row == col {
                counters[row_active.idx].get(&row_active.idx).copied().unwrap_or(0)
            } else {
                counters[row_active.idx].get(&col_active.idx).copied().unwrap_or(0)
            };
            data.push(
                HeatMapData {
                    value: Some(GoValue::Array(vec![
                        GoValue::Int(row as i64),
                        GoValue::Int(col as i64),
                        GoValue::Int(val),
                    ])),
                    ..HeatMapData::default()
                }
                .value(),
            );
            if val as f64 > max_val {
                max_val = val as f64;
            }
        }
    }
    if max_val == 0.0 {
        max_val = 1.0;
    }

    Some(build_heat_map_chart(&names, max_val, data, co, HEAT_MAP_HEIGHT, true))
}

/// The shared heatmap frame (Go `createHeatMap` global options — also the
/// shape couples' heatmap uses).
fn build_heat_map_chart(
    names: &[String],
    max_val: f64,
    data: Vec<GoValue>,
    co: &ChartOpts,
    height: &str,
    rotate_x: bool,
) -> Chart {
    let names_value = GoValue::Array(names.iter().map(|n| GoValue::Str(n.clone())).collect());

    let mut hm = Chart::new(ChartKind::HeatMap);
    let (w, h, bg, theme) = co.init("100%", height);
    hm.set_init(&w, &h, &bg, &theme);
    hm.tooltip = co.tooltip("item");
    hm.x_axis = XAxis {
        type_: "category".to_string(),
        data: Some(names_value.clone()),
        split_area: Some(SplitArea { show: Some(true) }),
        axis_label: Some(AxisLabel {
            rotate: if rotate_x { ROTATE_DEGREES } else { 0.0 },
            interval: if rotate_x { "0".to_string() } else { String::new() },
            font_size: LABEL_FONT_SIZE,
            color: co.text_muted_color().to_string(),
            ..AxisLabel::default()
        }),
        ..XAxis::default()
    };
    hm.y_axis = YAxis {
        type_: "category".to_string(),
        data: Some(names_value),
        split_area: Some(SplitArea { show: Some(true) }),
        axis_label: Some(AxisLabel {
            font_size: LABEL_FONT_SIZE,
            color: co.text_muted_color().to_string(),
            ..AxisLabel::default()
        }),
        ..YAxis::default()
    };
    hm.visual_maps = vec![VisualMap {
        calculable: Some(true),
        min: 0.0,
        max: max_val,
        in_range: Some(VisualMapInRange {
            color: ["#ebedf0", "#9be9a8", "#40c463", "#30a14e", "#216e39"]
                .iter()
                .map(|c| (*c).to_string())
                .collect(),
        }),
        orient: "horizontal".to_string(),
        left: "center".to_string(),
        bottom: "2%".to_string(),
        text_style: Some(TextStyle {
            color: co.text_muted_color().to_string(),
            ..TextStyle::default()
        }),
    }];
    hm.grid = vec![cf_plotpage::echarts::Grid {
        left: "20%".to_string(),
        top: "40".to_string(),
        right: "5%".to_string(),
        bottom: "20%".to_string(),
        ..cf_plotpage::echarts::Grid::default()
    }];

    let series = hm.add_series("Coupling", GoValue::Array(data));
    series.label = Some(Label {
        show: Some(true),
        position: "inside".to_string(),
        color: "black".to_string(),
        font_size: INNER_LABEL_SIZE,
        ..Label::default()
    });

    hm
}

/// One scored node (Go `nodeScore`).
struct NodeScore {
    name: String,
    self_count: i64,
    coupled: i64,
}

/// Go `createBarChart` (+ `computeScores` / `buildBarData`).
fn create_bar_chart(
    nodes: &[NodeSummary],
    counters: &[HashMap<usize, i64>],
    co: &ChartOpts,
    palette: &cf_plotpage::ChartPalette,
) -> Chart {
    let mut scores: Vec<NodeScore> = counters
        .iter()
        .enumerate()
        .map(|(idx, counter)| {
            let mut coupled: i64 = 0;
            // Go ranges the counter map randomly; addition is
            // order-independent.
            for (other, val) in counter {
                if *other != idx && *val > 0 {
                    coupled += *val;
                }
            }
            NodeScore {
                name: nodes[idx].name.clone(),
                self_count: counter.get(&idx).copied().unwrap_or(0),
                coupled,
            }
        })
        .collect();
    go_sort::slice(&mut scores, |a, b| a.self_count > b.self_count);
    scores.truncate(TOP_N_NODES);

    let labels: Vec<String> = scores.iter().map(|s| s.name.clone()).collect();

    let mut bar = Chart::new(ChartKind::Bar);
    let (w, h, bg, theme) = co.init("100%", "500px");
    bar.set_init(&w, &h, &bg, &theme);
    bar.tooltip = co.tooltip("axis");
    bar.legend = co.legend();
    bar.grid = vec![co.grid()];
    bar.data_zoom = co.data_zoom();
    bar.x_axis = XAxis {
        axis_label: Some(AxisLabel {
            rotate: ROTATE_DEGREES,
            interval: "0".to_string(),
            color: co.text_muted_color().to_string(),
            ..AxisLabel::default()
        }),
        axis_line: Some(AxisLine {
            line_style: Some(LineStyle {
                color: co.axis_color().to_string(),
                ..LineStyle::default()
            }),
            ..AxisLine::default()
        }),
        ..XAxis::default()
    };
    bar.y_axis = co.y_axis("Count");
    bar.set_x_axis_labels(&labels);

    let bar_data = |values: Vec<i64>| -> GoValue {
        GoValue::Array(
            values
                .into_iter()
                .map(|v| {
                    BarData {
                        value: Some(GoValue::Int(v)),
                        ..BarData::default()
                    }
                    .value()
                })
                .collect(),
        )
    };

    let self_data = bar_data(scores.iter().map(|s| s.self_count).collect());
    let coupled_data = bar_data(scores.iter().map(|s| s.coupled).collect());

    let series = bar.add_series("Self Changes", self_data);
    series.item_style = Some(ItemStyle {
        color: palette.primary[1].to_string(),
        ..ItemStyle::default()
    });
    let series = bar.add_series("Coupled Changes", coupled_data);
    series.item_style = Some(ItemStyle {
        color: palette.semantic.good.to_string(),
        ..ItemStyle::default()
    });

    bar
}
