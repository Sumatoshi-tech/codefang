//! `static/clones` plot sections — port of Go
//! `internal/analyzers/clones/plot.go`.
//!
//! Consumes the AGGREGATED RAW clones report (the `analyze.Report` value
//! `clones_raw_report_value` builds): one pie chart over the STORED clone
//! pairs' type distribution. The Go renderer never errors, so `sections`
//! always returns `Some`.

use cf_gojson::GoValue;
use cf_plotpage::echarts::{Chart, ChartKind, Label, PieData};
use cf_plotpage::{Hint, Section};

/// Plot display constants (plot.go:14).
const PLOT_CHART_HEIGHT: &str = "400px";
const PLOT_PIE_RADIUS: &str = "60%";

/// Distribution labels (report_section.go:18).
const DIST_LABEL_TYPE1: &str = "Type-1 (Exact)";
const DIST_LABEL_TYPE2: &str = "Type-2 (Renamed)";
const DIST_LABEL_TYPE3: &str = "Type-3 (Near-miss)";

/// The registered section renderer for `static/clones` — Go
/// `clones.RegisterPlotSections` → `(&Analyzer{}).generatePlotSections`.
pub fn sections(report: &GoValue) -> Option<Vec<Section>> {
    Some(vec![Section::new(
        "Clone Type Distribution",
        "Distribution of detected clones by type.",
        Box::new(clone_type_pie(report)),
        Hint {
            title: "How to interpret:".to_string(),
            items: vec![
                "<strong>Type-1 (Exact)</strong> = identical AST structure and tokens".to_string(),
                "<strong>Type-2 (Renamed)</strong> = identical structure, different variable names"
                    .to_string(),
                "<strong>Type-3 (Near-miss)</strong> = similar but modified structure".to_string(),
            ],
        },
    )])
}

/// Go `extractClonePairs` + `categorizeClonePairs` over the report's stored
/// `clone_pairs` (the ≤1000 detail list, not the exact total distribution).
fn categorize_clone_pairs(report: &GoValue) -> (i64, i64, i64) {
    let (mut t1, mut t2, mut t3) = (0i64, 0i64, 0i64);
    let Some(pairs) = report.as_map().and_then(|m| m.get("clone_pairs")) else {
        return (t1, t2, t3);
    };
    let GoValue::Array(items) = pairs else {
        return (t1, t2, t3);
    };
    for item in items {
        let Some(m) = item.as_map() else { continue };
        match m.get("clone_type") {
            Some(GoValue::Str(s)) if s == "Type-1" => t1 += 1,
            Some(GoValue::Str(s)) if s == "Type-2" => t2 += 1,
            Some(GoValue::Str(s)) if s == "Type-3" => t3 += 1,
            _ => {}
        }
    }
    (t1, t2, t3)
}

/// Go `generateCloneTypePieChart` (plot.go:50): a default-theme (`white`) pie —
/// the only init option is the 400px height — with the plain `{b}: {c}` label.
fn clone_type_pie(report: &GoValue) -> Chart {
    let (t1, t2, t3) = categorize_clone_pairs(report);

    let mut pie = Chart::new(ChartKind::Pie);
    pie.set_init("", PLOT_CHART_HEIGHT, "", "");
    pie.title.text = "Clone Types".to_string();

    let pie_data = GoValue::Array(
        [
            (DIST_LABEL_TYPE1, t1),
            (DIST_LABEL_TYPE2, t2),
            (DIST_LABEL_TYPE3, t3),
        ]
        .iter()
        .map(|(name, value)| {
            PieData {
                name: (*name).to_string(),
                value: Some(GoValue::Int(*value)),
                ..PieData::default()
            }
            .value()
        })
        .collect(),
    );
    let series = pie.add_series("Clone Types", pie_data);
    series.radius = Some(GoValue::Str(PLOT_PIE_RADIUS.to_string()));
    series.label = Some(Label {
        show: Some(true),
        formatter: "{b}: {c}".to_string(),
        ..Label::default()
    });
    pie
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_gojson::{GoMap, MapOrigin};

    fn raw_report() -> GoValue {
        let pair = |t: &str| {
            let mut m = GoMap::new(MapOrigin::Map);
            m.push("func_a", GoValue::Str("a.go::f".to_string()));
            m.push("func_b", GoValue::Str("b.go::g".to_string()));
            m.push("similarity", GoValue::Float(1.0));
            m.push("clone_type", GoValue::Str(t.to_string()));
            GoValue::Map(m)
        };
        let mut m = GoMap::new(MapOrigin::Map);
        m.push(
            "clone_pairs",
            GoValue::Array(vec![pair("Type-1"), pair("Type-3"), pair("Type-3")]),
        );
        GoValue::Map(m)
    }

    #[test]
    fn pie_option_json_matches_go_shape() {
        let secs = sections(&raw_report()).expect("sections");
        assert_eq!(secs.len(), 1);
        assert_eq!(secs[0].title, "Clone Type Distribution");
        let json = clone_type_pie(&raw_report()).option_json();
        // Default white theme: color array present, no backgroundColor.
        assert!(json.starts_with("{\"color\":[\"#5470c6\""));
        assert!(!json.contains("backgroundColor"));
        assert!(json.contains(
            "\"series\":[{\"name\":\"Clone Types\",\"type\":\"pie\",\"radius\":\"60%\",\"data\":[{\"name\":\"Type-1 (Exact)\",\"value\":1},{\"name\":\"Type-2 (Renamed)\",\"value\":0},{\"name\":\"Type-3 (Near-miss)\",\"value\":2}],\"label\":{\"show\":true,\"formatter\":\"{b}: {c}\"}}]"
        ));
        assert!(json.contains("\"title\":{\"text\":\"Clone Types\"}"));
    }

    #[test]
    fn pie_snippet_uses_default_canvas() {
        let pie = clone_type_pie(&raw_report());
        let snippet = pie.render_snippet("AAAAAAAAAAAA");
        assert!(snippet.contains("width:900px;height:400px;"));
        assert!(snippet.contains("\"white\""));
    }
}
