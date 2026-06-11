//! Minimal go-echarts v2.6.7 model — enough to reproduce, byte for byte, the
//! chart `option_*` JSON and `<div>`/`<script>` snippet that Go's
//! `--format plot` pages embed.
//!
//! # How Go produces these bytes
//!
//! go-echarts renders a chart as a full HTML page (`render/chart.go`
//! `RenderContent`); `plotpage.extractChartContent` (plotpage.go:244) then
//! slices out everything from `<div class="container">` to `</body>`, renames
//! the class to `echart-box`, and strips `<style>` blocks. The embedded option
//! is `BaseConfiguration.JSONNotEscaped` (charts/base.go:115): the option map
//! is built by `BaseConfiguration.json()` (a Go `map[string]interface{}`, so
//! the **top-level keys byte-sort**) whose values are the go-echarts `opts`
//! structs (so **nested keys keep struct declaration order** with `omitempty`),
//! encoded by `json.Encoder` with `SetEscapeHTML(false)` — which appends a
//! trailing newline.
//!
//! This module rebuilds exactly that: each `opts` struct used by the plot
//! sections has a Rust mirror whose `value()` emits a struct-origin
//! [`GoMap`] in the Go declaration order, honoring Go's `omitempty` semantics
//! (zero numerics/strings skipped; `interface{}` fields skipped only when
//! unset; `types.Bool`/`types.Float` pointers skipped only when `None`). The
//! mirrors carry only the fields the codefang plot builders set; when a later
//! analyzer port needs another field, add it **at its Go declaration position**
//! (cite the go-echarts source line) so the emission order stays exact.
//!
//! # Chart IDs
//!
//! Go assigns each chart a random 12-char `[A-Za-z]` ID
//! (`util.GenerateUniqueID`), which is the ONLY run-to-run nondeterminism in a
//! plot page. The Rust side must be deterministic, so [`ChartIdGen`] yields a
//! per-page sequential ID of the same shape.

use cf_gojson::{Encoder, GoMap, GoValue, MapOrigin};

/// Letters used for chart IDs (the Go generator draws from `[A-Za-z]`).
const ID_ALPHABET: &[u8; 52] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Chart-ID length (go-echarts `util.chartIDSize`).
const ID_LEN: usize = 12;

/// Deterministic replacement for go-echarts' random chart-ID generator: yields
/// `[A-Za-z]{12}` IDs in a fixed per-page sequence (base-52 counter, most
/// significant letter first), so two Rust runs render byte-identical pages.
#[derive(Debug, Default)]
pub struct ChartIdGen {
    next: u64,
}

impl ChartIdGen {
    /// New generator starting at the first ID (`AAAAAAAAAAAA`).
    #[must_use]
    pub fn new() -> Self {
        ChartIdGen::default()
    }

    /// Returns the next deterministic 12-letter chart ID.
    pub fn next_id(&mut self) -> String {
        let mut n = self.next;
        self.next += 1;
        let mut buf = [b'A'; ID_LEN];
        for slot in buf.iter_mut().rev() {
            *slot = ID_ALPHABET[(n % 52) as usize];
            n /= 52;
        }
        String::from_utf8(buf.to_vec()).expect("ASCII letters")
    }
}

// ---------------------------------------------------------------------------
// omitempty helpers (Go encoding/json semantics over the modeled field kinds).
// ---------------------------------------------------------------------------

/// Pushes a string field, skipping the Go `omitempty` empty string.
fn push_str(m: &mut GoMap, key: &str, v: &str) {
    if !v.is_empty() {
        m.push(key, GoValue::Str(v.to_string()));
    }
}

/// Pushes a `types.Bool` (`*bool`) field — emitted whenever set, even `false`.
fn push_bool(m: &mut GoMap, key: &str, v: Option<bool>) {
    if let Some(b) = v {
        m.push(key, GoValue::Bool(b));
    }
}

/// Pushes a numeric field, skipping the Go `omitempty` zero (int/float alike;
/// Go floats marshal via the shared go-float formatter, so `45.0` → `45`).
fn push_num(m: &mut GoMap, key: &str, v: f64) {
    if v != 0.0 {
        m.push(key, GoValue::Float(v));
    }
}

/// Pushes an integer field, skipping the Go `omitempty` zero.
fn push_int(m: &mut GoMap, key: &str, v: i64) {
    if v != 0 {
        m.push(key, GoValue::Int(v));
    }
}

/// Pushes an `interface{}` field — `omitempty` on an interface skips only
/// `nil`, so any present value (including `0` / `""` / `[]`) is emitted.
fn push_iface(m: &mut GoMap, key: &str, v: &Option<GoValue>) {
    if let Some(val) = v {
        m.push(key, val.clone());
    }
}

/// New struct-origin object map (Go struct field ordering).
fn struct_map() -> GoMap {
    GoMap::new(MapOrigin::Struct)
}

// ---------------------------------------------------------------------------
// opts struct mirrors. Field ORDER inside each `value()` is the go-echarts
// struct declaration order — it decides the JSON byte order, do not reorder.
// ---------------------------------------------------------------------------

/// `opts.TextStyle` (text_style.go) — only the fields the plot builders set.
#[derive(Debug, Clone, Default)]
pub struct TextStyle {
    /// Text color.
    pub color: String,
    /// Font size (declared after the font family fields).
    pub font_size: i64,
}

impl TextStyle {
    /// Serializes in declaration order: color, …, fontSize.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_str(&mut m, "color", &self.color);
        push_int(&mut m, "fontSize", self.font_size);
        GoValue::Map(m)
    }
}

/// `opts.Legend` (legend.go).
#[derive(Debug, Clone, Default)]
pub struct Legend {
    /// Legend type (`"scroll"` / `"plain"`).
    pub type_: String,
    /// Whether to show the legend (`types.Bool`).
    pub show: Option<bool>,
    /// Distance from the left.
    pub left: String,
    /// Distance from the top.
    pub top: String,
    /// Legend entries (`interface{}` — `charts.Radar.Validate` sets this to the
    /// series-name list; legend.go:111).
    pub data: Option<GoValue>,
    /// Legend text style.
    pub text_style: Option<TextStyle>,
}

impl Legend {
    /// Serializes in declaration order: type, show, left, top, …, data, …,
    /// textStyle.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_str(&mut m, "type", &self.type_);
        push_bool(&mut m, "show", self.show);
        push_str(&mut m, "left", &self.left);
        push_str(&mut m, "top", &self.top);
        push_iface(&mut m, "data", &self.data);
        if let Some(ts) = &self.text_style {
            m.push("textStyle", ts.value());
        }
        GoValue::Map(m)
    }
}

/// `opts.Tooltip` (tooltip.go).
#[derive(Debug, Clone, Default)]
pub struct Tooltip {
    /// Whether to show the tooltip (`types.Bool`).
    pub show: Option<bool>,
    /// Trigger mode (`"item"` / `"axis"` / `"none"`).
    pub trigger: String,
}

impl Tooltip {
    /// Serializes in declaration order: show, trigger.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_bool(&mut m, "show", self.show);
        push_str(&mut m, "trigger", &self.trigger);
        GoValue::Map(m)
    }
}

/// `opts.Title` (title.go).
#[derive(Debug, Clone, Default)]
pub struct Title {
    /// Main title text (`json:"text"`).
    pub text: String,
    /// Main title style (`json:"textStyle"`).
    pub title_style: Option<TextStyle>,
    /// Subtitle text (`json:"subtext"`).
    pub subtext: String,
    /// Subtitle style (`json:"subtextStyle"`).
    pub subtitle_style: Option<TextStyle>,
    /// Distance from the left.
    pub left: String,
    /// Distance from the top.
    pub top: String,
}

impl Title {
    /// Serializes in declaration order: show, text, link, target, textStyle,
    /// subtext, sublink, subtarget, subtextStyle, …, left, top, ….
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_str(&mut m, "text", &self.text);
        if let Some(ts) = &self.title_style {
            m.push("textStyle", ts.value());
        }
        push_str(&mut m, "subtext", &self.subtext);
        if let Some(ts) = &self.subtitle_style {
            m.push("subtextStyle", ts.value());
        }
        push_str(&mut m, "left", &self.left);
        push_str(&mut m, "top", &self.top);
        GoValue::Map(m)
    }
}

/// `opts.DataZoom` (data_zoom.go). `Type` carries NO `omitempty` in Go, so it
/// is always emitted (empty string included).
#[derive(Debug, Clone, Default)]
pub struct DataZoom {
    /// Zoom type (`"slider"` / `"inside"`); always emitted.
    pub type_: String,
    /// Start percentage (float32, omitempty).
    pub start: f64,
    /// End percentage (float32, omitempty).
    pub end: f64,
}

impl DataZoom {
    /// Serializes in declaration order: type (always), start, end, ….
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        m.push("type", GoValue::Str(self.type_.clone()));
        push_num(&mut m, "start", self.start);
        push_num(&mut m, "end", self.end);
        GoValue::Map(m)
    }
}

/// `opts.Grid` (grid.go).
#[derive(Debug, Clone, Default)]
pub struct Grid {
    /// Distance from the left.
    pub left: String,
    /// Distance from the top.
    pub top: String,
    /// Distance from the right.
    pub right: String,
    /// Distance from the bottom.
    pub bottom: String,
    /// Whether the grid region contains the axis labels (`types.Bool`).
    pub contain_label: Option<bool>,
}

impl Grid {
    /// Serializes in declaration order: show, left, top, right, bottom, width,
    /// height, containLabel, tooltip.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_str(&mut m, "left", &self.left);
        push_str(&mut m, "top", &self.top);
        push_str(&mut m, "right", &self.right);
        push_str(&mut m, "bottom", &self.bottom);
        push_bool(&mut m, "containLabel", self.contain_label);
        GoValue::Map(m)
    }
}

/// `opts.LineStyle` (series.go:511).
#[derive(Debug, Clone, Default)]
pub struct LineStyle {
    /// Line color.
    pub color: String,
    /// Line width (float32, omitempty).
    pub width: f64,
    /// Line type (`"solid"` / `"dashed"` / `"dotted"`).
    pub type_: String,
    /// Opacity (`types.Float` pointer — emitted whenever set).
    pub opacity: Option<f64>,
}

impl LineStyle {
    /// Serializes in declaration order: color, width, type, opacity, curveness.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_str(&mut m, "color", &self.color);
        push_num(&mut m, "width", self.width);
        push_str(&mut m, "type", &self.type_);
        if let Some(o) = self.opacity {
            m.push("opacity", GoValue::Float(o));
        }
        GoValue::Map(m)
    }
}

/// `opts.AreaStyle` (series.go:530).
#[derive(Debug, Clone, Default)]
pub struct AreaStyle {
    /// Fill color.
    pub color: String,
    /// Opacity (`types.Float` pointer — emitted whenever set).
    pub opacity: Option<f64>,
}

impl AreaStyle {
    /// Serializes in declaration order: color, origin, opacity.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_str(&mut m, "color", &self.color);
        if let Some(o) = self.opacity {
            m.push("opacity", GoValue::Float(o));
        }
        GoValue::Map(m)
    }
}

/// `opts.AxisLine` (x_axis.go).
#[derive(Debug, Clone, Default)]
pub struct AxisLine {
    /// Whether to show the axis line (`types.Bool`).
    pub show: Option<bool>,
    /// Line style (declared LAST in the Go struct).
    pub line_style: Option<LineStyle>,
}

impl AxisLine {
    /// Serializes in declaration order: show, onZero, …, lineStyle.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_bool(&mut m, "show", self.show);
        if let Some(ls) = &self.line_style {
            m.push("lineStyle", ls.value());
        }
        GoValue::Map(m)
    }
}

/// `opts.SplitLine` (global.go:144).
#[derive(Debug, Clone, Default)]
pub struct SplitLine {
    /// Whether to show split lines (`types.Bool`).
    pub show: Option<bool>,
    /// Split line style.
    pub line_style: Option<LineStyle>,
}

impl SplitLine {
    /// Serializes in declaration order: show, lineStyle, alignWithLabel.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_bool(&mut m, "show", self.show);
        if let Some(ls) = &self.line_style {
            m.push("lineStyle", ls.value());
        }
        GoValue::Map(m)
    }
}

/// `opts.SplitArea` (global.go:135).
#[derive(Debug, Clone, Default)]
pub struct SplitArea {
    /// Whether to show split areas (`types.Bool`).
    pub show: Option<bool>,
}

impl SplitArea {
    /// Serializes in declaration order: show, areaStyle.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_bool(&mut m, "show", self.show);
        GoValue::Map(m)
    }
}

/// `opts.AxisLabel` (x_axis.go). `ShowMinLabel` / `ShowMaxLabel` carry **no**
/// `omitempty` in Go, so an unset (`nil`) value marshals as `null` — they are
/// always present in the JSON.
#[derive(Debug, Clone, Default)]
pub struct AxisLabel {
    /// Whether to show labels (`types.Bool`).
    pub show: Option<bool>,
    /// Label interval (category axes; `"0"` shows all labels).
    pub interval: String,
    /// Rotation degree (float64, omitempty).
    pub rotate: f64,
    /// Label formatter template.
    pub formatter: String,
    /// Min-tick label visibility — ALWAYS emitted (`null` when unset).
    pub show_min_label: Option<bool>,
    /// Max-tick label visibility — ALWAYS emitted (`null` when unset).
    pub show_max_label: Option<bool>,
    /// Label color.
    pub color: String,
    /// Label font size (int, omitempty).
    pub font_size: i64,
}

impl AxisLabel {
    /// Serializes in declaration order: show, interval, inside, rotate, margin,
    /// formatter, showMinLabel (always), showMaxLabel (always), …, color, …,
    /// fontSize, ….
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_bool(&mut m, "show", self.show);
        push_str(&mut m, "interval", &self.interval);
        push_num(&mut m, "rotate", self.rotate);
        push_str(&mut m, "formatter", &self.formatter);
        m.push(
            "showMinLabel",
            self.show_min_label.map_or(GoValue::Null, GoValue::Bool),
        );
        m.push(
            "showMaxLabel",
            self.show_max_label.map_or(GoValue::Null, GoValue::Bool),
        );
        push_str(&mut m, "color", &self.color);
        push_int(&mut m, "fontSize", self.font_size);
        GoValue::Map(m)
    }
}

/// `opts.XAxis` (x_axis.go). Note the declaration order DIFFERS from
/// [`YAxis`]: `type` comes before `name` here, and `axisLine` before
/// `axisLabel`.
#[derive(Debug, Clone, Default)]
pub struct XAxis {
    /// Axis type (`"value"` / `"category"` / …).
    pub type_: String,
    /// Axis name.
    pub name: String,
    /// Category data (interface{} — emitted only when set).
    pub data: Option<GoValue>,
    /// Split areas (declared before splitLine; x_axis.go:102).
    pub split_area: Option<SplitArea>,
    /// Split lines.
    pub split_line: Option<SplitLine>,
    /// Axis line settings.
    pub axis_line: Option<AxisLine>,
    /// Axis label settings.
    pub axis_label: Option<AxisLabel>,
}

impl XAxis {
    /// Serializes in declaration order: show, alignTicks, position, type, name,
    /// nameLocation, nameGap, inverse, data, splitNumber, scale, min, max,
    /// minInterval, maxInterval, triggerEvent, gridIndex, splitArea, splitLine,
    /// axisLine, axisLabel, axisTick, axisPointer.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_str(&mut m, "type", &self.type_);
        push_str(&mut m, "name", &self.name);
        push_iface(&mut m, "data", &self.data);
        if let Some(sa) = &self.split_area {
            m.push("splitArea", sa.value());
        }
        if let Some(sl) = &self.split_line {
            m.push("splitLine", sl.value());
        }
        if let Some(al) = &self.axis_line {
            m.push("axisLine", al.value());
        }
        if let Some(al) = &self.axis_label {
            m.push("axisLabel", al.value());
        }
        GoValue::Map(m)
    }
}

/// `opts.YAxis` (y_axis.go). Declaration order differs from [`XAxis`]: `name`
/// comes first, `type` after `nameGap`, and `axisLabel` before `axisLine`.
#[derive(Debug, Clone, Default)]
pub struct YAxis {
    /// Axis name.
    pub name: String,
    /// Axis type (`"value"` / `"category"` / …).
    pub type_: String,
    /// Category data (interface{} — emitted only when set).
    pub data: Option<GoValue>,
    /// Axis minimum (interface{} — emitted whenever set, including `0`).
    pub min: Option<GoValue>,
    /// Axis maximum (interface{} — emitted whenever set).
    pub max: Option<GoValue>,
    /// Split areas (declared before splitLine; y_axis.go).
    pub split_area: Option<SplitArea>,
    /// Split lines.
    pub split_line: Option<SplitLine>,
    /// Axis label settings.
    pub axis_label: Option<AxisLabel>,
    /// Axis line settings.
    pub axis_line: Option<AxisLine>,
}

impl YAxis {
    /// Serializes in declaration order: name, alignTicks, position,
    /// nameLocation, nameGap, type, show, inverse, data, splitNumber, scale,
    /// min, max, minInterval, maxInterval, gridIndex, splitArea, splitLine,
    /// axisLabel, axisLine, axisPointer.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_str(&mut m, "name", &self.name);
        push_str(&mut m, "type", &self.type_);
        push_iface(&mut m, "data", &self.data);
        push_iface(&mut m, "min", &self.min);
        push_iface(&mut m, "max", &self.max);
        if let Some(sa) = &self.split_area {
            m.push("splitArea", sa.value());
        }
        if let Some(sl) = &self.split_line {
            m.push("splitLine", sl.value());
        }
        if let Some(al) = &self.axis_label {
            m.push("axisLabel", al.value());
        }
        if let Some(al) = &self.axis_line {
            m.push("axisLine", al.value());
        }
        GoValue::Map(m)
    }
}

/// `opts.ItemStyle` (series.go:178).
#[derive(Debug, Clone, Default)]
pub struct ItemStyle {
    /// Item color.
    pub color: String,
    /// Border color (series.go:193).
    pub border_color: String,
    /// Border width (float32, omitempty; series.go:202).
    pub border_width: f64,
    /// Gap width between treemap nodes (float32, omitempty; series.go:205).
    pub gap_width: f64,
    /// Opacity (`types.Float` pointer — emitted whenever set).
    pub opacity: Option<f64>,
}

impl ItemStyle {
    /// Serializes in declaration order: color, color0, areaColor, borderRadius,
    /// borderColor, borderColor0, borderColorSaturation, borderWidth, gapWidth,
    /// opacity, ….
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_str(&mut m, "color", &self.color);
        push_str(&mut m, "borderColor", &self.border_color);
        push_num(&mut m, "borderWidth", self.border_width);
        push_num(&mut m, "gapWidth", self.gap_width);
        if let Some(o) = self.opacity {
            m.push("opacity", GoValue::Float(o));
        }
        GoValue::Map(m)
    }
}

/// `opts.Label` (series.go).
#[derive(Debug, Clone, Default)]
pub struct Label {
    /// Whether to show the label (`types.Bool`).
    pub show: Option<bool>,
    /// Label text color.
    pub color: String,
    /// Label font size (float32, omitempty; series.go:32).
    pub font_size: f64,
    /// Label position.
    pub position: String,
    /// Label formatter template.
    pub formatter: String,
}

impl Label {
    /// Serializes in declaration order: show, color, fontStyle, fontWeight,
    /// fontFamily, fontSize, …, position, formatter.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_bool(&mut m, "show", self.show);
        push_str(&mut m, "color", &self.color);
        push_num(&mut m, "fontSize", self.font_size);
        push_str(&mut m, "position", &self.position);
        push_str(&mut m, "formatter", &self.formatter);
        GoValue::Map(m)
    }
}

/// `opts.UpperLabel` (series.go:621) — treemap parent-node labels.
#[derive(Debug, Clone, Default)]
pub struct UpperLabel {
    /// Whether to show the upper label (`types.Bool`).
    pub show: Option<bool>,
    /// Label text color (series.go:652).
    pub color: String,
}

impl UpperLabel {
    /// Serializes in declaration order: show, position, distance, rotate,
    /// offset, color, ….
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_bool(&mut m, "show", self.show);
        push_str(&mut m, "color", &self.color);
        GoValue::Map(m)
    }
}

/// `opts.TreeMapLevel` (series.go:596) — per-level treemap styling.
#[derive(Debug, Clone, Default)]
pub struct TreeMapLevel {
    /// Color saturation range (`[]float32`, omitempty).
    pub color_saturation: Vec<f64>,
    /// Upper label for this level.
    pub upper_label: Option<UpperLabel>,
    /// Item style for this level.
    pub item_style: Option<ItemStyle>,
}

impl TreeMapLevel {
    /// Serializes in declaration order: color, colorAlpha, colorSaturation,
    /// colorMappingBy, upperLabel, itemStyle, emphasis.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        if !self.color_saturation.is_empty() {
            m.push(
                "colorSaturation",
                GoValue::Array(self.color_saturation.iter().map(|v| GoValue::Float(*v)).collect()),
            );
        }
        if let Some(ul) = &self.upper_label {
            m.push("upperLabel", ul.value());
        }
        if let Some(is) = &self.item_style {
            m.push("itemStyle", is.value());
        }
        GoValue::Map(m)
    }
}

/// `opts.TreeMapNode` (charts.go:506) — one treemap tree node. `Value` is an
/// `int` with `omitempty` (zero skipped); `Children` is `omitempty` too.
#[derive(Debug, Clone, Default)]
pub struct TreeMapNode {
    /// Node name (`json:"name"`, no omitempty — always emitted).
    pub name: String,
    /// Node value (omitempty int).
    pub value: i64,
    /// Child nodes (omitempty slice).
    pub children: Vec<TreeMapNode>,
}

impl TreeMapNode {
    /// Serializes in declaration order: name (always), value, children.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        m.push("name", GoValue::Str(self.name.clone()));
        push_int(&mut m, "value", self.value);
        if !self.children.is_empty() {
            m.push(
                "children",
                GoValue::Array(self.children.iter().map(TreeMapNode::value).collect()),
            );
        }
        GoValue::Map(m)
    }
}

/// `opts.VisualMapInRange` (visual_map.go:79).
#[derive(Debug, Clone, Default)]
pub struct VisualMapInRange {
    /// In-range color ramp.
    pub color: Vec<String>,
}

impl VisualMapInRange {
    /// Serializes in declaration order: color, symbol, symbolSize.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        if !self.color.is_empty() {
            m.push(
                "color",
                GoValue::Array(self.color.iter().map(|c| GoValue::Str(c.clone())).collect()),
            );
        }
        GoValue::Map(m)
    }
}

/// `opts.VisualMap` (visual_map.go). `Calculable` carries **no** `omitempty`,
/// so an unset (`nil`) value marshals as `null` — always present.
#[derive(Debug, Clone, Default)]
pub struct VisualMap {
    /// Whether handles are shown — ALWAYS emitted (`null` when unset).
    pub calculable: Option<bool>,
    /// Domain minimum (float32, omitempty).
    pub min: f64,
    /// Domain maximum (float32, omitempty).
    pub max: f64,
    /// In-range visual channels.
    pub in_range: Option<VisualMapInRange>,
    /// Distance from the left.
    pub left: String,
    /// Distance from the bottom.
    pub bottom: String,
    /// Layout orientation.
    pub orient: String,
    /// Text style.
    pub text_style: Option<TextStyle>,
}

impl VisualMap {
    /// Serializes in declaration order: type, calculable (always), min, max,
    /// range, text, dimension, inRange, pieces, show, left, right, top, bottom,
    /// orient, textStyle.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        m.push(
            "calculable",
            self.calculable.map_or(GoValue::Null, GoValue::Bool),
        );
        push_num(&mut m, "min", self.min);
        push_num(&mut m, "max", self.max);
        if let Some(ir) = &self.in_range {
            m.push("inRange", ir.value());
        }
        push_str(&mut m, "left", &self.left);
        push_str(&mut m, "bottom", &self.bottom);
        push_str(&mut m, "orient", &self.orient);
        if let Some(ts) = &self.text_style {
            m.push("textStyle", ts.value());
        }
        GoValue::Map(m)
    }
}

/// `opts.Indicator` (radar.go:39) — one radar-chart dimension.
#[derive(Debug, Clone, Default)]
pub struct Indicator {
    /// Indicator name.
    pub name: String,
    /// Maximum value (float32, omitempty).
    pub max: f64,
}

impl Indicator {
    /// Serializes in declaration order: name, max, min, color.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_str(&mut m, "name", &self.name);
        push_num(&mut m, "max", self.max);
        GoValue::Map(m)
    }
}

/// `opts.AxisName` (radar.go:54) — radar indicator-name options.
#[derive(Debug, Clone, Default)]
pub struct AxisName {
    /// Font color.
    pub color: String,
}

impl AxisName {
    /// Serializes in declaration order: show, formatter, color, ….
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_str(&mut m, "color", &self.color);
        GoValue::Map(m)
    }
}

/// `opts.RadarComponent` (radar.go:7) — the chart-level radar coordinate.
#[derive(Debug, Clone, Default)]
pub struct RadarComponent {
    /// Radar indicators.
    pub indicator: Vec<Indicator>,
    /// Render shape (`"polygon"` / `"circle"`).
    pub shape: String,
    /// Indicator-axis segment count (int, omitempty).
    pub split_number: i64,
    /// Split areas.
    pub split_area: Option<SplitArea>,
    /// Split lines.
    pub split_line: Option<SplitLine>,
    /// Axis line.
    pub axis_line: Option<AxisLine>,
    /// Indicator-name options.
    pub axis_name: Option<AxisName>,
}

impl RadarComponent {
    /// Serializes in declaration order: indicator, shape, splitNumber, center,
    /// splitArea, splitLine, axisLine, startAngle, axisName.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        if !self.indicator.is_empty() {
            m.push(
                "indicator",
                GoValue::Array(self.indicator.iter().map(Indicator::value).collect()),
            );
        }
        push_str(&mut m, "shape", &self.shape);
        push_int(&mut m, "splitNumber", self.split_number);
        if let Some(sa) = &self.split_area {
            m.push("splitArea", sa.value());
        }
        if let Some(sl) = &self.split_line {
            m.push("splitLine", sl.value());
        }
        if let Some(al) = &self.axis_line {
            m.push("axisLine", al.value());
        }
        if let Some(an) = &self.axis_name {
            m.push("axisName", an.value());
        }
        GoValue::Map(m)
    }
}

/// One `markLine.data` item — `opts.MarkLineNameXAxisItem` /
/// `opts.MarkLineNameYAxisItem` (series.go).
#[derive(Debug, Clone)]
pub enum MarkLineItem {
    /// A vertical mark line at an X-axis value (`{name, xAxis}`).
    XAxis {
        /// Mark line name.
        name: String,
        /// X-axis value (interface{}).
        value: GoValue,
    },
    /// A horizontal mark line at a Y-axis value (`{name, yAxis}`).
    YAxis {
        /// Mark line name.
        name: String,
        /// Y-axis value (interface{}).
        value: GoValue,
    },
}

impl MarkLineItem {
    /// Serializes the item per its Go struct: name, xAxis|yAxis, valueDim.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        match self {
            MarkLineItem::XAxis { name, value } => {
                push_str(&mut m, "name", name);
                m.push("xAxis", value.clone());
            }
            MarkLineItem::YAxis { name, value } => {
                push_str(&mut m, "name", name);
                m.push("yAxis", value.clone());
            }
        }
        GoValue::Map(m)
    }
}

/// `opts.BarData` (series_bar.go).
#[derive(Debug, Clone, Default)]
pub struct BarData {
    /// Data item name.
    pub name: String,
    /// Value (interface{} — emitted whenever set, including `0`).
    pub value: Option<GoValue>,
    /// Per-item label.
    pub label: Option<Label>,
    /// Per-item style.
    pub item_style: Option<ItemStyle>,
}

impl BarData {
    /// Serializes in declaration order: name, value, label, itemStyle, tooltip.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_str(&mut m, "name", &self.name);
        push_iface(&mut m, "value", &self.value);
        if let Some(l) = &self.label {
            m.push("label", l.value());
        }
        if let Some(is) = &self.item_style {
            m.push("itemStyle", is.value());
        }
        GoValue::Map(m)
    }
}

/// `opts.LineData` (series_line.go:61).
#[derive(Debug, Clone, Default)]
pub struct LineData {
    /// Data item name.
    pub name: String,
    /// Value (interface{} — emitted whenever set, including `0`).
    pub value: Option<GoValue>,
    /// Symbol kind.
    pub symbol: String,
    /// Symbol size (int, omitempty).
    pub symbol_size: i64,
}

impl LineData {
    /// Serializes in declaration order: name, value, symbol, symbolSize, ….
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_str(&mut m, "name", &self.name);
        push_iface(&mut m, "value", &self.value);
        push_str(&mut m, "symbol", &self.symbol);
        push_int(&mut m, "symbolSize", self.symbol_size);
        GoValue::Map(m)
    }
}

/// `opts.ScatterData` (series_scatter.go).
#[derive(Debug, Clone, Default)]
pub struct ScatterData {
    /// Data item name.
    pub name: String,
    /// Value (interface{} — emitted whenever set).
    pub value: Option<GoValue>,
    /// Symbol kind.
    pub symbol: String,
    /// Symbol size (int, omitempty).
    pub symbol_size: i64,
}

impl ScatterData {
    /// Serializes in declaration order: name, value, symbol, symbolSize,
    /// symbolRotate, xAxisIndex, yAxisIndex.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_str(&mut m, "name", &self.name);
        push_iface(&mut m, "value", &self.value);
        push_str(&mut m, "symbol", &self.symbol);
        push_int(&mut m, "symbolSize", self.symbol_size);
        GoValue::Map(m)
    }
}

/// `opts.BoxPlotData` (charts.go:51).
#[derive(Debug, Clone, Default)]
pub struct BoxPlotData {
    /// Data item name.
    pub name: String,
    /// Value (interface{} — the `[min, Q1, median, Q3, max]` array).
    pub value: Option<GoValue>,
}

impl BoxPlotData {
    /// Serializes in declaration order: name, value, label, itemStyle,
    /// emphasis, tooltip.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_str(&mut m, "name", &self.name);
        push_iface(&mut m, "value", &self.value);
        GoValue::Map(m)
    }
}

/// `opts.LiquidData` (charts.go:290).
#[derive(Debug, Clone, Default)]
pub struct LiquidData {
    /// Data item name.
    pub name: String,
    /// Value (interface{} — emitted whenever set, including `0`).
    pub value: Option<GoValue>,
}

impl LiquidData {
    /// Serializes in declaration order: name, value.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_str(&mut m, "name", &self.name);
        push_iface(&mut m, "value", &self.value);
        GoValue::Map(m)
    }
}

/// `opts.HeatMapData` (charts.go:242).
#[derive(Debug, Clone, Default)]
pub struct HeatMapData {
    /// Data item name.
    pub name: String,
    /// Value (interface{} — the `[x, y, v]` triple).
    pub value: Option<GoValue>,
}

impl HeatMapData {
    /// Serializes in declaration order: name, value.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_str(&mut m, "name", &self.name);
        push_iface(&mut m, "value", &self.value);
        GoValue::Map(m)
    }
}

/// `opts.RadarData` (series_radar.go:28).
#[derive(Debug, Clone, Default)]
pub struct RadarData {
    /// Data item name.
    pub name: String,
    /// Value (interface{} — the per-indicator value array).
    pub value: Option<GoValue>,
}

impl RadarData {
    /// Serializes in declaration order: name, value.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_str(&mut m, "name", &self.name);
        push_iface(&mut m, "value", &self.value);
        GoValue::Map(m)
    }
}

/// `opts.PieData` (series_pie.go).
#[derive(Debug, Clone, Default)]
pub struct PieData {
    /// Data item name.
    pub name: String,
    /// Value (interface{} — emitted whenever set, including `0`).
    pub value: Option<GoValue>,
    /// Per-item label.
    pub label: Option<Label>,
    /// Per-item style.
    pub item_style: Option<ItemStyle>,
}

impl PieData {
    /// Serializes in declaration order: name, value, selected, label,
    /// itemStyle, tooltip.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_str(&mut m, "name", &self.name);
        push_iface(&mut m, "value", &self.value);
        if let Some(l) = &self.label {
            m.push("label", l.value());
        }
        if let Some(is) = &self.item_style {
            m.push("itemStyle", is.value());
        }
        GoValue::Map(m)
    }
}

/// `charts.SingleSeries` (series.go:9) — the per-series option object. Only
/// the fields the codefang plot builders set are modeled; each is emitted at
/// its Go declaration position.
#[derive(Debug, Clone, Default)]
pub struct SingleSeries {
    /// Series name.
    pub name: String,
    /// Chart type (`"bar"` / `"line"` / `"scatter"` / `"pie"`).
    pub type_: String,
    /// Stack group (Line | Bar).
    pub stack: String,
    /// Y-axis index (Line | Bar | Scatter; int, omitempty).
    pub y_axis_index: i64,
    /// TreeMap | Graph roam (`types.Bool`).
    pub roam: Option<bool>,
    /// Line step mode (interface{} — `""` is non-nil and EMITTED).
    pub step: Option<GoValue>,
    /// Line: whether to show symbols (`types.Bool`).
    pub show_symbol: Option<bool>,
    /// Line | Scatter | Radar symbol kind.
    pub symbol: String,
    /// Series color.
    pub color: String,
    /// Pie radius (interface{}).
    pub radius: Option<GoValue>,
    /// Line | Scatter | Radar symbol size (interface{}).
    pub symbol_size: Option<GoValue>,
    /// Smooth line (`types.Bool`).
    pub smooth: Option<bool>,
    /// TreeMap: distance from the left (Tree section, series.go:96).
    pub left: String,
    /// TreeMap: distance from the right.
    pub right: String,
    /// TreeMap: distance from the top.
    pub top: String,
    /// TreeMap: distance from the bottom.
    pub bottom: String,
    /// TreeMap leaf depth (int, omitempty; series.go:110).
    pub leaf_depth: i64,
    /// TreeMap per-level styling (`interface{}` holding `*[]TreeMapLevel`).
    pub levels: Vec<TreeMapLevel>,
    /// TreeMap upper label (`interface{}` holding `*UpperLabel`).
    pub upper_label: Option<UpperLabel>,
    /// Series data (interface{} — the typed data array).
    pub data: Option<GoValue>,
    /// Series animation (`types.Bool`; series.go:151, after data).
    pub animation: Option<bool>,
    /// `*opts.ItemStyle` (embedded pointer, `json:"itemStyle"`).
    pub item_style: Option<ItemStyle>,
    /// `*opts.Label` (embedded pointer, `json:"label"`).
    pub label: Option<Label>,
    /// `*opts.MarkLines` data items (embedded pointer, `json:"markLine"`).
    pub mark_lines: Vec<MarkLineItem>,
    /// `*opts.LineStyle` (embedded pointer, `json:"lineStyle"`).
    pub line_style: Option<LineStyle>,
    /// `*opts.AreaStyle` (embedded pointer, `json:"areaStyle"`).
    pub area_style: Option<AreaStyle>,
}

impl SingleSeries {
    /// Serializes the modeled fields at their Go declaration positions:
    /// name, type, …, stack, xAxisIndex, yAxisIndex, …, roam, …, step, smooth,
    /// connectNulls, showSymbol, symbol, color, …, radius, symbolSize, …,
    /// left, right, top, bottom, …, leafDepth, levels, upperLabel, …, data, …,
    /// animation, …, then the embedded option pointers in order: encode,
    /// itemStyle, label, labelLayout, labelLine, emphasis, markLine, markArea,
    /// markPoint, rippleEffect, lineStyle, areaStyle, ….
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_str(&mut m, "name", &self.name);
        push_str(&mut m, "type", &self.type_);
        push_str(&mut m, "stack", &self.stack);
        push_int(&mut m, "yAxisIndex", self.y_axis_index);
        push_bool(&mut m, "roam", self.roam);
        push_iface(&mut m, "step", &self.step);
        push_bool(&mut m, "smooth", self.smooth);
        push_bool(&mut m, "showSymbol", self.show_symbol);
        push_str(&mut m, "symbol", &self.symbol);
        push_str(&mut m, "color", &self.color);
        push_iface(&mut m, "radius", &self.radius);
        push_iface(&mut m, "symbolSize", &self.symbol_size);
        push_str(&mut m, "left", &self.left);
        push_str(&mut m, "right", &self.right);
        push_str(&mut m, "top", &self.top);
        push_str(&mut m, "bottom", &self.bottom);
        push_int(&mut m, "leafDepth", self.leaf_depth);
        if !self.levels.is_empty() {
            m.push(
                "levels",
                GoValue::Array(self.levels.iter().map(TreeMapLevel::value).collect()),
            );
        }
        if let Some(ul) = &self.upper_label {
            m.push("upperLabel", ul.value());
        }
        push_iface(&mut m, "data", &self.data);
        push_bool(&mut m, "animation", self.animation);
        if let Some(is) = &self.item_style {
            m.push("itemStyle", is.value());
        }
        if let Some(l) = &self.label {
            m.push("label", l.value());
        }
        if !self.mark_lines.is_empty() {
            let mut ml = struct_map();
            ml.push(
                "data",
                GoValue::Array(self.mark_lines.iter().map(MarkLineItem::value).collect()),
            );
            m.push("markLine", GoValue::Map(ml));
        }
        if let Some(ls) = &self.line_style {
            m.push("lineStyle", ls.value());
        }
        if let Some(als) = &self.area_style {
            m.push("areaStyle", als.value());
        }
        GoValue::Map(m)
    }
}

/// The chart family — selects the series `type` and whether the chart carries
/// rectangular XY axes (`hasXYAxis` in go-echarts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartKind {
    /// `charts.NewBar()`.
    Bar,
    /// `charts.NewLine()`.
    Line,
    /// `charts.NewScatter()`.
    Scatter,
    /// `charts.NewBoxPlot()`.
    BoxPlot,
    /// `charts.NewPie()` (no XY axes).
    Pie,
    /// `charts.NewLiquid()` (no XY axes; series type `liquidFill`).
    Liquid,
    /// `charts.NewHeatMap()` (XY axes; its `Validate` does NOT copy
    /// `xAxisData`, the axes carry their own category data).
    HeatMap,
    /// `charts.NewTreeMap()` (no XY axes).
    TreeMap,
    /// `charts.NewRadar()` (no XY axes; emits the `radar` component and its
    /// `Validate` sets `legend.data` to the series-name list).
    Radar,
}

impl ChartKind {
    /// The go-echarts series type string (`types.Chart*`).
    #[must_use]
    pub fn series_type(self) -> &'static str {
        match self {
            ChartKind::Bar => "bar",
            ChartKind::Line => "line",
            ChartKind::Scatter => "scatter",
            ChartKind::BoxPlot => "boxplot",
            ChartKind::Pie => "pie",
            ChartKind::Liquid => "liquidFill",
            ChartKind::HeatMap => "heatmap",
            ChartKind::TreeMap => "treemap",
            ChartKind::Radar => "radar",
        }
    }

    /// Whether this chart family carries XY axes (go-echarts `hasXYAxis`).
    #[must_use]
    pub fn has_xy_axis(self) -> bool {
        !matches!(
            self,
            ChartKind::Pie | ChartKind::Liquid | ChartKind::TreeMap | ChartKind::Radar
        )
    }
}

/// go-echarts' default palette (`initSeriesColors`, charts/base.go:257) —
/// emitted as the top-level `color` array whenever the theme is `"white"`.
const DEFAULT_COLORS: [&str; 9] = [
    "#5470c6", "#91cc75", "#fac858", "#ee6666", "#73c0de", "#3ba272", "#fc8452", "#9a60b4",
    "#ea7ccc",
];

/// A chart — the Rust analogue of a configured go-echarts chart instance
/// (`BaseConfiguration` + the per-family `charts.*` wrapper).
#[derive(Debug, Clone)]
pub struct Chart {
    /// Chart family.
    pub kind: ChartKind,
    /// Canvas width (`opts.Initialization.Width`, default `900px`).
    pub width: String,
    /// Canvas height (`opts.Initialization.Height`, default `500px`).
    pub height: String,
    /// Canvas background color (empty → key omitted).
    pub background_color: String,
    /// ECharts theme (`Initialization.Validate` default `"white"`).
    pub theme: String,
    /// Title options.
    pub title: Title,
    /// Legend options.
    pub legend: Legend,
    /// Tooltip options.
    pub tooltip: Tooltip,
    /// DataZoom list (`dataZoom` key when non-empty).
    pub data_zoom: Vec<DataZoom>,
    /// Grid list (`grid` key when non-empty).
    pub grid: Vec<Grid>,
    /// X axis (rect charts only).
    pub x_axis: XAxis,
    /// Y axis (rect charts only).
    pub y_axis: YAxis,
    /// Additional Y axes appended by `ExtendYAxis` (rectangle.go:28).
    pub extra_y_axes: Vec<YAxis>,
    /// Category labels installed by `SetXAxis` (bar/line), copied into
    /// `x_axis.data` at serialization time (go-echarts `Validate`).
    pub x_axis_data: Option<GoValue>,
    /// VisualMap list (`visualMap` key when non-empty).
    pub visual_maps: Vec<VisualMap>,
    /// Radar component (`radar` key; emitted for [`ChartKind::Radar`]).
    pub radar: Option<RadarComponent>,
    /// Palette override (`charts.WithColorsOpts` replaces `bc.Colors`); `None`
    /// keeps the go-echarts default palette.
    pub colors: Option<Vec<String>>,
    /// Series list (`series: null` when empty — Go nil `MultiSeries`).
    pub series: Vec<SingleSeries>,
}

impl Chart {
    /// New chart of the given family with go-echarts initialization defaults
    /// (`initBaseConfiguration` + `Initialization.Validate`).
    #[must_use]
    pub fn new(kind: ChartKind) -> Self {
        Chart {
            kind,
            width: "900px".to_string(),
            height: "500px".to_string(),
            background_color: String::new(),
            theme: "white".to_string(),
            title: Title::default(),
            legend: Legend::default(),
            tooltip: Tooltip::default(),
            data_zoom: Vec::new(),
            grid: Vec::new(),
            x_axis: XAxis::default(),
            y_axis: YAxis::default(),
            extra_y_axes: Vec::new(),
            x_axis_data: None,
            visual_maps: Vec::new(),
            radar: None,
            colors: None,
            series: Vec::new(),
        }
    }

    /// `charts.WithInitializationOpts` — canvas size + background + theme
    /// (`Validate` maps an empty theme to `"white"`).
    pub fn set_init(&mut self, width: &str, height: &str, background_color: &str, theme: &str) {
        self.width = if width.is_empty() { "900px".into() } else { width.to_string() };
        self.height = if height.is_empty() { "500px".into() } else { height.to_string() };
        self.background_color = background_color.to_string();
        self.theme = if theme.is_empty() { "white".into() } else { theme.to_string() };
    }

    /// `SetXAxis` — category labels for bar/line charts.
    pub fn set_x_axis_labels(&mut self, labels: &[String]) {
        self.x_axis_data = Some(GoValue::Array(
            labels.iter().map(|l| GoValue::Str(l.clone())).collect(),
        ));
    }

    /// `AddSeries` — appends a series of this chart's type and returns it for
    /// option configuration (the Go `SeriesOpts` are field assignments).
    pub fn add_series(&mut self, name: &str, data: GoValue) -> &mut SingleSeries {
        self.series.push(SingleSeries {
            name: name.to_string(),
            type_: self.kind.series_type().to_string(),
            data: Some(data),
            ..SingleSeries::default()
        });
        self.series.last_mut().expect("just pushed")
    }

    /// Builds the option object exactly as `BaseConfiguration.json()`
    /// (charts/base.go:125) does: a Go `map[string]interface{}` (top-level keys
    /// byte-sorted at encode time) with the struct-valued components.
    #[must_use]
    pub fn option_value(&self) -> GoValue {
        let mut obj = GoMap::new(MapOrigin::Map);
        obj.push("title", self.title.value());
        // charts.Radar.Validate(): Legend.Data = the series-name list.
        if self.kind == ChartKind::Radar {
            let mut legend = self.legend.clone();
            legend.data = Some(GoValue::Array(
                self.series.iter().map(|s| GoValue::Str(s.name.clone())).collect(),
            ));
            obj.push("legend", legend.value());
        } else {
            obj.push("legend", self.legend.value());
        }
        obj.push("tooltip", self.tooltip.value());
        // MultiSeries is a nil slice when no series were added — Go marshals
        // that as `null` (the empty-chart pages show "series":null).
        if self.series.is_empty() {
            obj.push("series", GoValue::NilSlice);
        } else {
            obj.push(
                "series",
                GoValue::Array(self.series.iter().map(SingleSeries::value).collect()),
            );
        }
        if self.kind == ChartKind::Radar {
            if let Some(radar) = &self.radar {
                obj.push("radar", radar.value());
            } else {
                obj.push("radar", RadarComponent::default().value());
            }
        }
        obj.push("toolbox", GoValue::Map(struct_map()));
        if !self.data_zoom.is_empty() {
            obj.push(
                "dataZoom",
                GoValue::Array(self.data_zoom.iter().map(DataZoom::value).collect()),
            );
        }
        if !self.visual_maps.is_empty() {
            obj.push(
                "visualMap",
                GoValue::Array(self.visual_maps.iter().map(VisualMap::value).collect()),
            );
        }
        if self.kind.has_xy_axis() {
            // chart.Validate(): XAxisList[0].Data = xAxisData — EXCEPT HeatMap,
            // whose own Validate skips the copy (the axes keep the category
            // data set through WithXAxisOpts / WithYAxisOpts).
            let mut x = self.x_axis.clone();
            if self.kind != ChartKind::HeatMap {
                x.data.clone_from(&self.x_axis_data);
            }
            obj.push("xAxis", GoValue::Array(vec![x.value()]));
            let mut y_list = vec![self.y_axis.value()];
            y_list.extend(self.extra_y_axes.iter().map(YAxis::value));
            obj.push("yAxis", GoValue::Array(y_list));
        }
        if self.theme == "white" {
            let palette: Vec<GoValue> = match &self.colors {
                Some(colors) => colors.iter().map(|c| GoValue::Str(c.clone())).collect(),
                None => DEFAULT_COLORS.iter().map(|c| GoValue::Str((*c).to_string())).collect(),
            };
            obj.push("color", GoValue::Array(palette));
        }
        if !self.background_color.is_empty() {
            obj.push("backgroundColor", GoValue::Str(self.background_color.clone()));
        }
        if !self.grid.is_empty() {
            obj.push(
                "grid",
                GoValue::Array(self.grid.iter().map(Grid::value).collect()),
            );
        }
        GoValue::Map(obj)
    }

    /// The option JSON exactly as `JSONNotEscaped` emits it: compact
    /// `encoding/json` with `SetEscapeHTML(false)` and the `Encode` trailing
    /// newline.
    #[must_use]
    pub fn option_json(&self) -> String {
        Encoder::compact()
            .with_html_escaping(false)
            .with_trailing_newline(true)
            .encode_to_string(&self.option_value())
    }

    /// Renders the extracted chart snippet — the bytes
    /// `plotpage.extractChartContent` produces from the go-echarts chart page:
    /// the `echart-box` element, the init/option/setOption script, and the
    /// trailing blank line left where the `<style>` block was stripped.
    #[must_use]
    pub fn render_snippet(&self, chart_id: &str) -> String {
        let json = self.option_json();
        format!(
            "<div class=\"echart-box\">\n    <div class=\"item\" id=\"{id}\" style=\"width:{w};height:{h};\"></div>\n</div><script type=\"text/javascript\">\n    \"use strict\";\n    let goecharts_{id} = echarts.init(document.getElementById('{id}'), \"{theme}\", {{ renderer: \"canvas\" }});\n    let option_{id} = {json}\n    goecharts_{id}.setOption(option_{id});\n</script>\n\n",
            id = chart_id,
            w = self.width,
            h = self.height,
            theme = self.theme,
            json = json,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_gen_shape_and_determinism() {
        let mut a = ChartIdGen::new();
        let mut b = ChartIdGen::new();
        for _ in 0..3 {
            let ia = a.next_id();
            assert_eq!(ia.len(), 12);
            assert!(ia.bytes().all(|c| c.is_ascii_alphabetic()));
            assert_eq!(ia, b.next_id());
        }
    }

    #[test]
    fn empty_pie_option_matches_go_shape() {
        // createEmptyComplexityPie-style chart: title only, no series.
        let mut c = Chart::new(ChartKind::Pie);
        c.set_init("600px", "400px", "transparent", "");
        c.title = Title {
            text: "T".into(),
            subtext: "No data".into(),
            left: "center".into(),
            title_style: Some(TextStyle { color: "#d6d3d1".into(), ..TextStyle::default() }),
            subtitle_style: Some(TextStyle { color: "#a8a29e".into(), ..TextStyle::default() }),
            ..Title::default()
        };
        let json = c.option_json();
        assert!(json.starts_with("{\"backgroundColor\":\"transparent\",\"color\":["));
        assert!(json.contains("\"series\":null"));
        assert!(json.contains(
            "\"title\":{\"text\":\"T\",\"textStyle\":{\"color\":\"#d6d3d1\"},\"subtext\":\"No data\",\"subtextStyle\":{\"color\":\"#a8a29e\"},\"left\":\"center\"}"
        ));
        assert!(json.ends_with("\"tooltip\":{}}\n"));
    }

    #[test]
    fn html_not_escaped_in_option_json() {
        let mut c = Chart::new(ChartKind::Pie);
        let data = GoValue::Array(vec![PieData {
            name: "Complex (>10)".into(),
            value: Some(GoValue::Int(3)),
            ..PieData::default()
        }
        .value()]);
        c.add_series("s", data);
        assert!(c.option_json().contains("\"Complex (>10)\""));
    }
}
