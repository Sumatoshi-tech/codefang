//! `history/devs` plot sections + the dashboard tab
//! builders (`GenerateStoreSections` over the developer/language/bus-factor/
//! activity/churn/aggregate store kinds — the run's `ComputedMetrics`).

use cf_devs::model::{ComputedMetrics, DeveloperData};
use cf_gojson::GoValue;
use cf_plotpage::components::{
    Alert, Badge, BadgeColor, Card, GridLayout, Renderable, Stat, TabItem, Table, Tabs, Text,
};
use cf_plotpage::ChartIdGen;
use cf_plotpage::echarts::{
    AreaStyle, Chart, ChartKind, ItemStyle, LineStyle, RadarData, TreeMapLevel, TreeMapNode,
    UpperLabel,
};
use cf_plotpage::{
    build_bar_chart, build_line_chart, BarSeries, ChartOpts, Hint, LineSeries, Section,
    SeriesValue,
};

use crate::handlers::go_sort;
use crate::handlers::plot_sections::history_shared::{format_number, format_signed_number};

/// Reference devs plot/dashboard constants.
const MAX_DEVS: usize = 20;
const TOP_DEVS_FOR_RADAR: usize = 5;
const TOP_DEVS_FOR_TREEMAP: usize = 30;
const TOP_LANGUAGES_FOR_RADAR: usize = 8;
const TREEMAP_HEIGHT: &str = "600px";
const RADAR_HEIGHT: &str = "500px";
const RISK_TABLE_MAX_ROWS: usize = 20;
const OVERVIEW_TABLE_LIMIT: usize = 10;
const AREA_OPACITY_NORMAL: f64 = 0.6;
const AREA_OPACITY_OTHER: f64 = 0.4;
const RADAR_SPLIT_NUM: i64 = 5;
const RADAR_AREA_OPACITY: f64 = 0.2;
const RADAR_INDICATOR_MAX: f64 = 100.0;
const LINE_WIDTH: f64 = 2.0;
const TREEMAP_LEAF_DEPTH: i64 = 2;
const BORDER_WIDTH: f64 = 2.0;
const GAP_WIDTH: f64 = 2.0;
const STATS_GRID_COLS: usize = 4;
const LANG_OTHER: &str = "Other";

/// A sequence of renderables interleaved with raw HTML (the reference tab contents
/// write components and literal `<div class="mt-6">` wrappers to one writer).
struct Composite(Vec<Box<dyn Renderable>>);

impl Renderable for Composite {
    fn render(&self, out: &mut String, ids: &mut ChartIdGen) {
        for item in &self.0 {
            item.render(out, ids);
        }
    }
}

/// Literal HTML fragment.
struct Raw(&'static str);

impl Renderable for Raw {
    fn render(&self, out: &mut String, _ids: &mut ChartIdGen) {
        out.push_str(self.0);
    }
}

/// The reference `GenerateStoreSections`: no developers AND no activity yield zero
/// sections; otherwise one "Developer Analytics" section holding the
/// six-tab dashboard.
pub fn sections(metrics: &ComputedMetrics) -> Vec<Section> {
    if metrics.developers.is_empty() && metrics.activity.is_empty() {
        return Vec::new();
    }

    let top_langs: Vec<String> = metrics
        .languages
        .iter()
        .take(TOP_LANGUAGES_FOR_RADAR)
        .map(|l| l.name.clone())
        .collect();

    let tabs = Tabs::new(
        "dashboard",
        vec![
            TabItem {
                id: "overview".to_string(),
                label: "Overview".to_string(),
                content: Some(create_overview_tab(metrics)),
            },
            TabItem {
                id: "activity".to_string(),
                label: "Activity Trends".to_string(),
                content: Some(create_activity_tab(metrics)),
            },
            TabItem {
                id: "workload".to_string(),
                label: "Workload Distribution".to_string(),
                content: Some(create_workload_tab(metrics)),
            },
            TabItem {
                id: "languages".to_string(),
                label: "Language Expertise".to_string(),
                content: Some(create_languages_tab(metrics, &top_langs)),
            },
            TabItem {
                id: "busfactor".to_string(),
                label: "Bus Factor".to_string(),
                content: Some(create_busfactor_tab(metrics)),
            },
            TabItem {
                id: "churn".to_string(),
                label: "Code Churn".to_string(),
                content: Some(create_churn_tab(metrics)),
            },
        ],
    );

    vec![Section {
        title: "Developer Analytics".to_string(),
        subtitle: "Multi-dimensional view of team contributions and codebase ownership".to_string(),
        chart: Some(Box::new(tabs)),
        hint: Hint::default(),
    }]
}

// ---------------------------------------------------------------------------
// Overview tab.
// ---------------------------------------------------------------------------

fn create_overview_tab(metrics: &ComputedMetrics) -> Box<dyn Renderable> {
    let agg = &metrics.aggregate;
    let mut parts: Vec<Box<dyn Renderable>> = Vec::new();

    // renderStats.
    parts.push(Box::new(GridLayout::new(
        STATS_GRID_COLS,
        vec![
            Box::new(Stat::new("Total Commits", &format_number(agg.total_commits))),
            Box::new(Stat::new("Total Developers", &agg.total_developers.to_string())),
            Box::new(Stat::new("Active Developers", &agg.active_developers.to_string())),
            Box::new(Stat::new("Project Bus Factor", &agg.project_bus_factor.to_string())),
        ],
    )));

    // renderContributorsTable.
    parts.push(Box::new(Raw(r#"<div class="mt-6">"#)));
    let mut table = Table::new(
        ["Rank", "Developer", "Commits", "Lines Added", "Lines Removed", "Net Lines"]
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
    );
    for (i, dev) in metrics.developers.iter().take(OVERVIEW_TABLE_LIMIT).enumerate() {
        table.add_row(vec![
            (i + 1).to_string(),
            dev.name.clone(),
            format_number(dev.commits),
            format_number(dev.added),
            format_number(dev.removed),
            format_signed_number(dev.net_lines),
        ]);
    }
    parts.push(Box::new(
        Card::new("Top Contributors", "Developers ranked by total commits")
            .with_content(Box::new(table)),
    ));
    parts.push(Box::new(Raw("</div>")));

    // renderRiskAlert.
    let critical = metrics.busfactor.iter().filter(|b| b.risk_level == "CRITICAL").count();
    let high = metrics.busfactor.iter().filter(|b| b.risk_level == "HIGH").count();
    if critical > 0 || high > 0 {
        parts.push(Box::new(Raw(r#"<div class="mt-6">"#)));
        parts.push(Box::new(Alert::new(
            "Bus Factor Warning",
            &format!(
                "{critical} languages have CRITICAL bus factor risk, {high} have HIGH risk. See Bus Factor tab for details.",
            ),
            BadgeColor::Warning,
        )));
        parts.push(Box::new(Raw("</div>")));
    }

    Box::new(Composite(parts))
}

// ---------------------------------------------------------------------------
// Activity tab.
// ---------------------------------------------------------------------------

fn create_activity_tab(metrics: &ComputedMetrics) -> Box<dyn Renderable> {
    if metrics.activity.is_empty() {
        return Box::new(Text::new("No activity data available"));
    }

    let top_devs: Vec<i64> = metrics.developers.iter().take(MAX_DEVS).map(|d| d.id).collect();
    let labels: Vec<String> = metrics.activity.iter().map(|a| a.tick.to_string()).collect();

    let name_by_id: std::collections::HashMap<i64, &str> =
        metrics.developers.iter().map(|d| (d.id, d.name.as_str())).collect();

    let commits_for = |entries: &[cf_devs::model::DeveloperCommits], dev_id: i64| -> i64 {
        entries
            .iter()
            .find(|dc| dc.dev_id == dev_id)
            .map_or(0, |dc| dc.commits)
    };

    let mut series: Vec<LineSeries> = Vec::with_capacity(top_devs.len() + 1);
    for dev_id in &top_devs {
        series.push(LineSeries {
            name: name_by_id.get(dev_id).copied().unwrap_or("").to_string(),
            data: metrics
                .activity
                .iter()
                .map(|a| SeriesValue::Int(commits_for(&a.by_developer, *dev_id)))
                .collect(),
            stack: "total".to_string(),
            area_opacity: AREA_OPACITY_NORMAL,
            ..LineSeries::default()
        });
    }
    if metrics.developers.len() > MAX_DEVS {
        series.push(LineSeries {
            name: "Others".to_string(),
            data: metrics
                .activity
                .iter()
                .map(|a| {
                    SeriesValue::Int(
                        a.by_developer
                            .iter()
                            .filter(|dc| !top_devs.contains(&dc.dev_id))
                            .map(|dc| dc.commits)
                            .sum(),
                    )
                })
                .collect(),
            stack: "total".to_string(),
            area_opacity: AREA_OPACITY_OTHER,
            ..LineSeries::default()
        });
    }

    let co = ChartOpts::default_dark();
    let mut line = build_line_chart(Some(&co), &labels, &series, "Commits");
    line.title = co.title(
        "Developer Activity Over Time",
        "Stacked area showing contribution velocity (commits per tick)",
    );

    Box::new(line)
}

// ---------------------------------------------------------------------------
// Workload tab.
// ---------------------------------------------------------------------------

fn find_primary_language(dev: &DeveloperData) -> &str {
    let mut primary = LANG_OTHER;
    let mut max_lines = 0i64;
    for entry in &dev.languages {
        if entry.added > max_lines {
            max_lines = entry.added;
            primary = if entry.language.is_empty() { LANG_OTHER } else { &entry.language };
        }
    }
    primary
}

fn create_workload_tab(metrics: &ComputedMetrics) -> Box<dyn Renderable> {
    if metrics.developers.is_empty() {
        return Box::new(Text::new("No workload data available"));
    }

    // buildTreemapNodes. the reference implementation iterates the langDevs map randomly before the
    // value-descending sorts; language-name-ascending is the deterministic
    // stand-in (ties at equal Value are reference-variant).
    let mut lang_devs: std::collections::BTreeMap<&str, Vec<TreeMapNode>> =
        std::collections::BTreeMap::new();
    let mut lang_totals: std::collections::BTreeMap<&str, i64> = std::collections::BTreeMap::new();
    for dev in metrics.developers.iter().take(TOP_DEVS_FOR_TREEMAP) {
        let lang = find_primary_language(dev);
        lang_devs.entry(lang).or_default().push(TreeMapNode {
            name: dev.name.clone(),
            value: dev.commits,
            children: Vec::new(),
        });
        *lang_totals.entry(lang).or_insert(0) += dev.commits;
    }

    let mut root_nodes: Vec<TreeMapNode> = Vec::with_capacity(lang_devs.len());
    for (lang, mut dev_nodes) in lang_devs {
        go_sort::slice(&mut dev_nodes, |a, b| a.value > b.value);
        root_nodes.push(TreeMapNode {
            name: lang.to_string(),
            value: lang_totals[lang],
            children: dev_nodes,
        });
    }
    go_sort::slice(&mut root_nodes, |a, b| a.value > b.value);

    let co = ChartOpts::default_dark();
    let mut tm = Chart::new(ChartKind::TreeMap);
    let (w, h, bg, theme) = co.init("100%", TREEMAP_HEIGHT);
    tm.set_init(&w, &h, &bg, &theme);
    tm.title = co.title(
        "Workload Distribution",
        "Developers grouped by primary language, sized by commits",
    );
    tm.tooltip = co.tooltip("item");

    let data = GoValue::Array(root_nodes.iter().map(TreeMapNode::value).collect());
    let series = tm.add_series("Workload", data);
    series.animation = Some(true);
    series.roam = Some(true);
    series.leaf_depth = TREEMAP_LEAF_DEPTH;
    // The reference implementation's WithTreeMapOpts drops `Label` and `ColorMappingBy`.
    series.levels = vec![
        TreeMapLevel {
            upper_label: Some(UpperLabel {
                show: Some(true),
                ..UpperLabel::default()
            }),
            item_style: Some(ItemStyle {
                border_color: "#555".to_string(),
                border_width: BORDER_WIDTH,
                gap_width: GAP_WIDTH,
                ..ItemStyle::default()
            }),
            ..TreeMapLevel::default()
        },
        TreeMapLevel {
            color_saturation: vec![0.3, 0.6],
            item_style: Some(ItemStyle {
                border_color: "#999".to_string(),
                border_width: 1.0,
                gap_width: 1.0,
                ..ItemStyle::default()
            }),
            ..TreeMapLevel::default()
        },
    ];
    series.upper_label = Some(UpperLabel {
        show: Some(true),
        ..UpperLabel::default()
    });
    series.left = "2%".to_string();
    series.right = "2%".to_string();
    series.top = "15%".to_string();
    series.bottom = "2%".to_string();

    Box::new(tm)
}

// ---------------------------------------------------------------------------
// Languages tab.
// ---------------------------------------------------------------------------

fn create_languages_tab(metrics: &ComputedMetrics, top_langs: &[String]) -> Box<dyn Renderable> {
    if top_langs.is_empty() || metrics.developers.is_empty() {
        return Box::new(Text::new("No language data available"));
    }

    // topDevsByContribution: contribution-descending over the top languages.
    let contribution = |dev: &DeveloperData| -> i64 {
        let by_lang: std::collections::HashMap<&str, i64> = dev
            .languages
            .iter()
            .map(|e| (e.language.as_str(), e.added + e.removed))
            .collect();
        top_langs
            .iter()
            .filter_map(|lang| by_lang.get(lang.as_str()))
            .sum()
    };
    let mut scored: Vec<(&DeveloperData, i64)> =
        metrics.developers.iter().map(|d| (d, contribution(d))).collect();
    go_sort::slice(&mut scored, |a, b| a.1 > b.1);
    scored.truncate(TOP_DEVS_FOR_RADAR);

    let indicators: Vec<cf_plotpage::echarts::Indicator> = top_langs
        .iter()
        .map(|lang| cf_plotpage::echarts::Indicator {
            name: lang.clone(),
            max: RADAR_INDICATOR_MAX,
        })
        .collect();

    let co = ChartOpts::default_dark();
    let mut radar = Chart::new(ChartKind::Radar);
    let (w, h, bg, theme) = co.init("100%", RADAR_HEIGHT);
    radar.set_init(&w, &h, &bg, &theme);
    radar.title = co.title(
        "Language Expertise",
        "Relative expertise profile per developer (strongest language = 100%)",
    );
    radar.tooltip = co.tooltip("item");
    radar.legend = co.legend();
    radar.radar = Some(co.radar_component(indicators, RADAR_SPLIT_NUM));

    // buildRadarData: each developer normalized to their own strongest
    // language.
    for (dev, _) in &scored {
        let by_lang: std::collections::HashMap<&str, i64> = dev
            .languages
            .iter()
            .map(|e| (e.language.as_str(), e.added + e.removed))
            .collect();
        let contribs: Vec<i64> = top_langs
            .iter()
            .map(|lang| by_lang.get(lang.as_str()).copied().unwrap_or(0))
            .collect();
        let max_contrib = contribs.iter().copied().max().unwrap_or(0);
        let values: Vec<GoValue> = contribs
            .iter()
            .map(|c| {
                if max_contrib > 0 {
                    GoValue::Float(*c as f64 / max_contrib as f64 * RADAR_INDICATOR_MAX)
                } else {
                    GoValue::Float(0.0)
                }
            })
            .collect();

        let data = GoValue::Array(vec![
            RadarData {
                value: Some(GoValue::Array(values)),
                ..RadarData::default()
            }
            .value(),
        ]);
        let series = radar.add_series(&dev.name, data);
        series.area_style = Some(AreaStyle {
            opacity: Some(RADAR_AREA_OPACITY),
            ..AreaStyle::default()
        });
        series.line_style = Some(LineStyle {
            width: LINE_WIDTH,
            ..LineStyle::default()
        });
    }

    Box::new(radar)
}

// ---------------------------------------------------------------------------
// Bus factor tab.
// ---------------------------------------------------------------------------

fn risk_badge_html(level: &str) -> String {
    let color = match level {
        "CRITICAL" => BadgeColor::Error,
        "HIGH" => BadgeColor::Warning,
        "MEDIUM" => BadgeColor::Info,
        _ => BadgeColor::Success,
    };
    let badge = Badge::new(level).with_color(color);
    let mut out = String::new();
    let mut ids = ChartIdGen::new();
    badge.render(&mut out, &mut ids);
    out
}

fn format_percent(pct: f64) -> String {
    format!("{pct:.1}%")
}

fn create_busfactor_tab(metrics: &ComputedMetrics) -> Box<dyn Renderable> {
    if metrics.busfactor.is_empty() {
        return Box::new(Text::new("No bus factor data available"));
    }

    let count = |level: &str| -> usize {
        metrics.busfactor.iter().filter(|b| b.risk_level == level).count()
    };

    let mut parts: Vec<Box<dyn Renderable>> = Vec::new();

    // renderSummary.
    parts.push(Box::new(GridLayout::new(
        STATS_GRID_COLS,
        vec![
            Box::new(
                Stat::new("Critical Risk", &count("CRITICAL").to_string())
                    .with_trend("", BadgeColor::Error),
            ),
            Box::new(
                Stat::new("High Risk", &count("HIGH").to_string())
                    .with_trend("", BadgeColor::Warning),
            ),
            Box::new(
                Stat::new("Medium Risk", &count("MEDIUM").to_string())
                    .with_trend("", BadgeColor::Info),
            ),
            Box::new(
                Stat::new("Low Risk", &count("LOW").to_string())
                    .with_trend("", BadgeColor::Success),
            ),
        ],
    )));

    // renderTable.
    parts.push(Box::new(Raw(r#"<div class="mt-6">"#)));
    let mut table = Table::new(
        [
            "Language",
            "Risk Level",
            "Bus Factor",
            "Primary Owner",
            "Primary %",
            "Secondary Owner",
            "Secondary %",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect(),
    );
    for bfd in metrics.busfactor.iter().take(RISK_TABLE_MAX_ROWS) {
        table.add_row(vec![
            bfd.language.clone(),
            risk_badge_html(&bfd.risk_level),
            format!("{}/{}", bfd.bus_factor, bfd.total_contributors),
            bfd.primary_dev_name.clone(),
            format_percent(bfd.primary_pct),
            if bfd.secondary_dev_name.is_empty() {
                "-".to_string()
            } else {
                bfd.secondary_dev_name.clone()
            },
            if bfd.secondary_pct == 0.0 {
                "-".to_string()
            } else {
                format_percent(bfd.secondary_pct)
            },
        ]);
    }
    parts.push(Box::new(
        Card::new(
            "Bus Factor Analysis",
            "Risk assessment by language ownership concentration (CHAOSS methodology)",
        )
        .with_content(Box::new(table)),
    ));
    parts.push(Box::new(Raw("</div>")));

    Box::new(Composite(parts))
}

// ---------------------------------------------------------------------------
// Churn tab.
// ---------------------------------------------------------------------------

fn create_churn_tab(metrics: &ComputedMetrics) -> Box<dyn Renderable> {
    if metrics.churn.is_empty() {
        return Box::new(Text::new("No churn data available"));
    }

    let labels: Vec<String> = metrics.churn.iter().map(|c| c.tick.to_string()).collect();
    let series = vec![
        BarSeries {
            name: "Added".to_string(),
            data: metrics.churn.iter().map(|c| SeriesValue::Int(c.added)).collect(),
            color: "#22c55e".to_string(),
            ..BarSeries::default()
        },
        BarSeries {
            name: "Removed".to_string(),
            data: metrics.churn.iter().map(|c| SeriesValue::Int(-c.removed)).collect(),
            color: "#ef4444".to_string(),
            ..BarSeries::default()
        },
    ];

    let co = ChartOpts::default_dark();
    let mut bar = build_bar_chart(Some(&co), &labels, &series, "Lines");
    bar.title = co.title("Code Churn", "Lines added vs removed over time");

    Box::new(bar)
}
