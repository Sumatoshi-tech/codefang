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
    /// Legend text style.
    pub text_style: Option<TextStyle>,
}

impl Legend {
    /// Serializes in declaration order: type, show, left, top, …, textStyle.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_str(&mut m, "type", &self.type_);
        push_bool(&mut m, "show", self.show);
        push_str(&mut m, "left", &self.left);
        push_str(&mut m, "top", &self.top);
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

/// `opts.ItemStyle` (series.go).
#[derive(Debug, Clone, Default)]
pub struct ItemStyle {
    /// Item color.
    pub color: String,
    /// Opacity (`types.Float` pointer — emitted whenever set).
    pub opacity: Option<f64>,
}

impl ItemStyle {
    /// Serializes in declaration order: color, …, opacity, ….
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

/// `opts.Label` (series.go).
#[derive(Debug, Clone, Default)]
pub struct Label {
    /// Whether to show the label (`types.Bool`).
    pub show: Option<bool>,
    /// Label text color.
    pub color: String,
    /// Label position.
    pub position: String,
    /// Label formatter template.
    pub formatter: String,
}

impl Label {
    /// Serializes in declaration order: show, color, fontStyle, …, position,
    /// formatter.
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_bool(&mut m, "show", self.show);
        push_str(&mut m, "color", &self.color);
        push_str(&mut m, "position", &self.position);
        push_str(&mut m, "formatter", &self.formatter);
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
    /// Line: whether to show symbols (`types.Bool`).
    pub show_symbol: Option<bool>,
    /// Series color.
    pub color: String,
    /// Pie radius (interface{}).
    pub radius: Option<GoValue>,
    /// Line | Scatter | Radar symbol size (interface{}).
    pub symbol_size: Option<GoValue>,
    /// Smooth line (`types.Bool`).
    pub smooth: Option<bool>,
    /// Series data (interface{} — the typed data array).
    pub data: Option<GoValue>,
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
    /// name, type, …, stack, …, smooth, …, showSymbol, symbol, color, …,
    /// radius, symbolSize, …, data, …, then the embedded option pointers in
    /// order: encode, itemStyle, label, labelLayout, labelLine, emphasis,
    /// markLine, markArea, markPoint, rippleEffect, lineStyle, areaStyle, ….
    #[must_use]
    pub fn value(&self) -> GoValue {
        let mut m = struct_map();
        push_str(&mut m, "name", &self.name);
        push_str(&mut m, "type", &self.type_);
        push_str(&mut m, "stack", &self.stack);
        push_bool(&mut m, "smooth", self.smooth);
        push_bool(&mut m, "showSymbol", self.show_symbol);
        push_str(&mut m, "color", &self.color);
        push_iface(&mut m, "radius", &self.radius);
        push_iface(&mut m, "symbolSize", &self.symbol_size);
        push_iface(&mut m, "data", &self.data);
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
    /// `charts.NewPie()` (no XY axes).
    Pie,
}

impl ChartKind {
    /// The go-echarts series type string.
    #[must_use]
    pub fn series_type(self) -> &'static str {
        match self {
            ChartKind::Bar => "bar",
            ChartKind::Line => "line",
            ChartKind::Scatter => "scatter",
            ChartKind::Pie => "pie",
        }
    }

    /// Whether this chart family carries XY axes (go-echarts `hasXYAxis`).
    #[must_use]
    pub fn has_xy_axis(self) -> bool {
        !matches!(self, ChartKind::Pie)
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
    /// Category labels installed by `SetXAxis` (bar/line), copied into
    /// `x_axis.data` at serialization time (go-echarts `Validate`).
    pub x_axis_data: Option<GoValue>,
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
            x_axis_data: None,
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
        obj.push("legend", self.legend.value());
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
        obj.push("toolbox", GoValue::Map(struct_map()));
        if !self.data_zoom.is_empty() {
            obj.push(
                "dataZoom",
                GoValue::Array(self.data_zoom.iter().map(DataZoom::value).collect()),
            );
        }
        if self.kind.has_xy_axis() {
            // chart.Validate(): XAxisList[0].Data = xAxisData.
            let mut x = self.x_axis.clone();
            x.data.clone_from(&self.x_axis_data);
            obj.push("xAxis", GoValue::Array(vec![x.value()]));
            obj.push("yAxis", GoValue::Array(vec![self.y_axis.value()]));
        }
        if self.theme == "white" {
            obj.push(
                "color",
                GoValue::Array(DEFAULT_COLORS.iter().map(|c| GoValue::Str((*c).to_string())).collect()),
            );
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
