//! HTML visualization components.
//!
//! Each component reproduces, byte for byte, the output of its reference
//! `html/template` file (`plotpage/templates/*.html`). The templates keep
//! every literal byte outside `{{…}}` actions, so the render functions
//! interleave the same literal fragments with the html/template-escaped data.

use crate::echarts::ChartIdGen;

/// The render contract for chart/page components.
///
/// `ids` is the per-page deterministic chart-ID generator (the reference
/// renderer assigns a random ID per chart at construction; this crate assigns
/// sequential IDs at render so output is reproducible).
///
/// `Send` is part of the contract: sections are built concurrently by the
/// multi-analyzer plot orchestrator and handed to the rendering thread; every
/// component is plain data, so the bound costs implementors nothing.
pub trait Renderable: Send {
    /// Appends this component's HTML to `out`.
    fn render(&self, out: &mut String, ids: &mut ChartIdGen);
}

/// Pre-rendered raw HTML.
pub struct RawHtml(pub String);

impl Renderable for RawHtml {
    fn render(&self, out: &mut String, _ids: &mut ChartIdGen) {
        out.push_str(&self.0);
    }
}

impl Renderable for crate::echarts::Chart {
    fn render(&self, out: &mut String, ids: &mut ChartIdGen) {
        let id = ids.next_id();
        out.push_str(&self.render_snippet(&id));
    }
}

/// Escapes `s` exactly like `html/template`'s contextual auto-escaper for
/// text nodes and quoted attribute values (its `htmlReplacementTable`):
/// `"` `&` `'` `+` `<` `>` become numeric/named entities and NUL becomes
/// U+FFFD. Note `+` → `&#43;` — this is why the base64 logo data URI carries
/// `&#43;` in the reference output.
///
/// ```
/// use cf_plotpage::html_escape;
///
/// assert_eq!(html_escape("a+b"), "a&#43;b");
/// assert_eq!(html_escape("<x>&'\""), "&lt;x&gt;&amp;&#39;&#34;");
/// assert_eq!(html_escape("\0"), "\u{FFFD}");
/// ```
#[must_use]
pub fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\u{0}' => out.push('\u{FFFD}'),
            '"' => out.push_str("&#34;"),
            '&' => out.push_str("&amp;"),
            '\'' => out.push_str("&#39;"),
            '+' => out.push_str("&#43;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
    out
}

/// Escapes `s` exactly like `template.HTMLEscapeString` (text/template's
/// table: `&` `'` `<` `>` `"` only — no `+`). Used by [`Text`], which the
/// reference implementation escapes with that function directly
/// (components.go:261).
///
/// Unlike [`html_escape`], `+` is left untouched:
///
/// ```
/// use cf_plotpage::html_escape_string;
///
/// assert_eq!(html_escape_string("a+b"), "a+b");
/// assert_eq!(html_escape_string("<x>&'\""), "&lt;x&gt;&amp;&#39;&#34;");
/// ```
#[must_use]
pub fn html_escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '\'' => out.push_str("&#39;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&#34;"),
            c => out.push(c),
        }
    }
    out
}

/// Maximum grid columns (components.go `maxGridColumns`).
const MAX_GRID_COLUMNS: usize = 4;

/// Badge styling variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BadgeVariant {
    /// Solid background.
    Solid,
    /// Soft background (default).
    #[default]
    Soft,
    /// Outline only.
    Outline,
}

/// Badge colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BadgeColor {
    /// Neutral stone palette (default).
    #[default]
    Default,
    /// Accent (amber).
    Accent,
    /// Success (green).
    Success,
    /// Warning (yellow).
    Warning,
    /// Error (red).
    Error,
    /// Info (blue).
    Info,
}

/// Plain text content, HTML-escaped on render.
pub struct Text {
    /// The text content.
    pub content: String,
}

impl Text {
    /// New text block.
    #[must_use]
    pub fn new(content: &str) -> Self {
        Self {
            content: content.to_string(),
        }
    }
}

impl Renderable for Text {
    fn render(&self, out: &mut String, _ids: &mut ChartIdGen) {
        out.push_str(&html_escape_string(&self.content));
    }
}

/// An inline badge/tag (templates/badge.html).
pub struct Badge {
    /// Badge text.
    pub text: String,
    /// Styling variant.
    pub variant: BadgeVariant,
    /// Badge color.
    pub color: BadgeColor,
}

impl Badge {
    /// New soft default-colored badge.
    #[must_use]
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            variant: BadgeVariant::Soft,
            color: BadgeColor::Default,
        }
    }

    /// Sets the badge color.
    #[must_use]
    pub const fn with_color(mut self, color: BadgeColor) -> Self {
        self.color = color;
        self
    }

    /// The Tailwind classes for the variant+color.
    #[must_use]
    pub const fn classes(&self) -> &'static str {
        match self.variant {
            BadgeVariant::Solid => match self.color {
                BadgeColor::Accent => "bg-amber-600 text-white",
                BadgeColor::Success => "bg-green-600 text-white",
                BadgeColor::Warning => "bg-yellow-500 text-white",
                BadgeColor::Error => "bg-red-600 text-white",
                BadgeColor::Info => "bg-blue-600 text-white",
                BadgeColor::Default => {
                    "bg-stone-600 text-white dark:bg-stone-400 dark:text-stone-900"
                }
            },
            BadgeVariant::Outline => match self.color {
                BadgeColor::Accent => "border border-amber-600 text-amber-700 dark:text-amber-400",
                BadgeColor::Success => "border border-green-600 text-green-700 dark:text-green-400",
                BadgeColor::Warning => {
                    "border border-yellow-500 text-yellow-700 dark:text-yellow-400"
                }
                BadgeColor::Error => "border border-red-600 text-red-700 dark:text-red-400",
                BadgeColor::Info => "border border-blue-600 text-blue-700 dark:text-blue-400",
                BadgeColor::Default => "border border-stone-400 text-stone-600 dark:text-stone-400",
            },
            BadgeVariant::Soft => match self.color {
                BadgeColor::Accent => {
                    "bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-200"
                }
                BadgeColor::Success => {
                    "bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200"
                }
                BadgeColor::Warning => {
                    "bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-200"
                }
                BadgeColor::Error => "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200",
                BadgeColor::Info => "bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200",
                BadgeColor::Default => {
                    "bg-stone-100 text-stone-800 dark:bg-stone-800 dark:text-stone-200"
                }
            },
        }
    }
}

impl Renderable for Badge {
    fn render(&self, out: &mut String, _ids: &mut ChartIdGen) {
        out.push_str("<span class=\"inline-flex items-center px-2 py-0.5 text-xs font-medium rounded-sm ");
        out.push_str(self.classes());
        out.push_str("\">");
        out.push_str(&html_escape(&self.text));
        out.push_str("</span>\n");
    }
}

/// A card container (templates/card.html).
pub struct Card {
    /// Card title.
    pub title: String,
    /// Card subtitle.
    pub subtitle: String,
    /// Card body content.
    pub content: Option<Box<dyn Renderable>>,
}

impl Card {
    /// New card.
    #[must_use]
    pub fn new(title: &str, subtitle: &str) -> Self {
        Self {
            title: title.to_string(),
            subtitle: subtitle.to_string(),
            content: None,
        }
    }

    /// Sets the card content.
    #[must_use]
    pub fn with_content(mut self, content: Box<dyn Renderable>) -> Self {
        self.content = Some(content);
        self
    }
}

impl Renderable for Card {
    fn render(&self, out: &mut String, ids: &mut ChartIdGen) {
        out.push_str("<div\n    class=\"bg-white dark:bg-stone-900 rounded-sm border border-stone-200 dark:border-stone-700 shadow-sm overflow-hidden\"\n>\n    ");
        if !self.title.is_empty() || !self.subtitle.is_empty() {
            out.push_str("\n    <div class=\"px-5 py-4 border-b border-stone-100 dark:border-stone-800\">\n        ");
            if !self.title.is_empty() {
                out.push_str("\n        <h3 class=\"text-lg font-medium text-stone-900 dark:text-stone-50\">\n            ");
                out.push_str(&html_escape(&self.title));
                out.push_str("\n        </h3>\n        ");
            }
            out.push(' ');
            if !self.subtitle.is_empty() {
                out.push_str("\n        <p class=\"mt-0.5 text-sm text-stone-500 dark:text-stone-400\">\n            ");
                out.push_str(&html_escape(&self.subtitle));
                out.push_str("\n        </p>\n        ");
            }
            out.push_str("\n    </div>\n    ");
        }
        out.push(' ');
        if let Some(content) = &self.content {
            out.push_str("\n    <div class=\"p-5\">");
            content.render(out, ids);
            out.push_str("</div>\n    ");
        }
        out.push_str("\n</div>\n");
    }
}

/// A single tab in a tab group.
pub struct TabItem {
    /// Tab identifier.
    pub id: String,
    /// Tab label.
    pub label: String,
    /// Tab panel content.
    pub content: Option<Box<dyn Renderable>>,
}

/// A tabbed interface (templates/tabs.html).
pub struct Tabs {
    /// Tab group identifier.
    pub id: String,
    /// Tab items.
    pub items: Vec<TabItem>,
}

impl Tabs {
    /// New tab group.
    #[must_use]
    pub fn new(id: &str, items: Vec<TabItem>) -> Self {
        Self {
            id: id.to_string(),
            items,
        }
    }
}

impl Renderable for Tabs {
    fn render(&self, out: &mut String, ids: &mut ChartIdGen) {
        if self.items.is_empty() {
            return;
        }
        out.push_str("<div class=\"tabs\" data-tabs=\"");
        out.push_str(&html_escape(&self.id));
        out.push_str("\">\n    <div class=\"flex border-b border-stone-200 dark:border-stone-700\" role=\"tablist\">\n");
        for (i, item) in self.items.iter().enumerate() {
            let (selected, classes) = if i == 0 {
                ("true", "border-accent text-accent")
            } else {
                (
                    "false",
                    "border-transparent text-stone-500 dark:text-stone-400 hover:text-stone-700 dark:hover:text-stone-300",
                )
            };
            out.push_str("\n        <button\n            type=\"button\"\n            role=\"tab\"\n            aria-selected=\"");
            out.push_str(selected);
            out.push_str("\"\n            aria-controls=\"");
            out.push_str(&html_escape(&self.id));
            out.push('-');
            out.push_str(&html_escape(&item.id));
            out.push_str("\"\n            data-tab=\"");
            out.push_str(&html_escape(&item.id));
            out.push_str("\"\n            class=\"px-4 py-2 text-sm font-medium border-b-2 ");
            out.push_str(classes);
            // The surrounding single quotes are literal template text (not
            // data), so html/template leaves them as-is; only the interpolated
            // ids are escaped (alphanumeric ids pass through unchanged).
            out.push_str(" transition-colors focus:outline-none focus:ring-2 focus:ring-accent focus:ring-offset-2 dark:focus:ring-offset-stone-900\"\n            onclick=\"switchTab('");
            out.push_str(&html_escape(&self.id));
            out.push_str("', '");
            out.push_str(&html_escape(&item.id));
            out.push_str("')\"\n        >");
            out.push_str(&html_escape(&item.label));
            out.push_str("</button>\n");
        }
        out.push_str("\n    </div>\n");
        for (i, item) in self.items.iter().enumerate() {
            out.push_str("\n    <div\n        id=\"");
            out.push_str(&html_escape(&self.id));
            out.push('-');
            out.push_str(&html_escape(&item.id));
            out.push_str("\"\n        role=\"tabpanel\"\n        class=\"tab-panel p-4 ");
            if i != 0 {
                out.push_str("hidden");
            }
            out.push_str("\"\n        data-panel=\"");
            out.push_str(&html_escape(&item.id));
            out.push_str("\"\n    >");
            if let Some(content) = &item.content {
                content.render(out, ids);
            }
            out.push_str("</div>\n");
        }
        out.push_str("\n</div>\n");
    }
}

/// A responsive grid layout (templates/grid.html).
pub struct GridLayout {
    /// Number of columns (clamped to 1..=4).
    pub columns: usize,
    /// Tailwind gap class.
    pub gap: String,
    /// Grid items.
    pub items: Vec<Box<dyn Renderable>>,
}

impl GridLayout {
    /// New grid layout.
    #[must_use]
    pub fn new(columns: usize, items: Vec<Box<dyn Renderable>>) -> Self {
        Self {
            columns: columns.clamp(1, MAX_GRID_COLUMNS),
            gap: "gap-4".to_string(),
            items,
        }
    }

    const fn col_class(&self) -> &'static str {
        match self.columns {
            1 => "grid-cols-1",
            2 => "grid-cols-1 md:grid-cols-2",
            3 => "grid-cols-1 md:grid-cols-2 lg:grid-cols-3",
            4 => "grid-cols-1 md:grid-cols-2 lg:grid-cols-4",
            _ => "",
        }
    }
}

impl Renderable for GridLayout {
    fn render(&self, out: &mut String, ids: &mut ChartIdGen) {
        out.push_str("<div class=\"grid ");
        out.push_str(self.col_class());
        out.push(' ');
        out.push_str(&self.gap);
        out.push_str("\">\n");
        for item in &self.items {
            out.push_str("\n    <div>");
            item.render(out, ids);
            out.push_str("</div>\n");
        }
        out.push_str("\n</div>\n");
    }
}

/// A statistic/metric display (templates/stat.html).
pub struct Stat {
    /// Metric label.
    pub label: String,
    /// Metric value.
    pub value: String,
    /// Optional trend text.
    pub trend: String,
    /// Trend color.
    pub color: BadgeColor,
}

impl Stat {
    /// New stat display.
    #[must_use]
    pub fn new(label: &str, value: &str) -> Self {
        Self {
            label: label.to_string(),
            value: value.to_string(),
            trend: String::new(),
            color: BadgeColor::Default,
        }
    }

    /// Sets the trend indicator.
    #[must_use]
    pub fn with_trend(mut self, trend: &str, color: BadgeColor) -> Self {
        self.trend = trend.to_string();
        self.color = color;
        self
    }
}

impl Renderable for Stat {
    fn render(&self, out: &mut String, _ids: &mut ChartIdGen) {
        let trend_class = match self.color {
            BadgeColor::Success => "text-green-600 dark:text-green-400",
            BadgeColor::Error => "text-red-600 dark:text-red-400",
            BadgeColor::Warning => "text-yellow-600 dark:text-yellow-400",
            BadgeColor::Default | BadgeColor::Accent | BadgeColor::Info => "text-stone-500",
        };
        out.push_str("<div class=\"text-center\">\n    <p class=\"text-sm text-stone-500 dark:text-stone-400\">");
        out.push_str(&html_escape(&self.label));
        out.push_str("</p>\n    <p class=\"text-2xl font-semibold text-stone-900 dark:text-stone-50\">");
        out.push_str(&html_escape(&self.value));
        out.push_str("</p>\n");
        if !self.trend.is_empty() {
            out.push_str("\n    <p class=\"text-sm ");
            out.push_str(trend_class);
            out.push_str("\">");
            out.push_str(&html_escape(&self.trend));
            out.push_str("</p>\n");
        }
        out.push_str("\n</div>\n");
    }
}

/// An alert/notification box (templates/alert.html).
pub struct Alert {
    /// Alert title.
    pub title: String,
    /// Alert message.
    pub message: String,
    /// Alert color.
    pub color: BadgeColor,
}

impl Alert {
    /// New alert.
    #[must_use]
    pub fn new(title: &str, message: &str, color: BadgeColor) -> Self {
        Self {
            title: title.to_string(),
            message: message.to_string(),
            color,
        }
    }
}

impl Renderable for Alert {
    fn render(&self, out: &mut String, _ids: &mut ChartIdGen) {
        let (bg, border, text, title_cls) = match self.color {
            BadgeColor::Success => (
                "bg-green-50 dark:bg-green-950",
                "border-green-500",
                "text-green-700 dark:text-green-300",
                "text-green-800 dark:text-green-200",
            ),
            BadgeColor::Warning => (
                "bg-yellow-50 dark:bg-yellow-950",
                "border-yellow-500",
                "text-yellow-700 dark:text-yellow-300",
                "text-yellow-800 dark:text-yellow-200",
            ),
            BadgeColor::Error => (
                "bg-red-50 dark:bg-red-950",
                "border-red-500",
                "text-red-700 dark:text-red-300",
                "text-red-800 dark:text-red-200",
            ),
            BadgeColor::Info => (
                "bg-blue-50 dark:bg-blue-950",
                "border-blue-500",
                "text-blue-700 dark:text-blue-300",
                "text-blue-800 dark:text-blue-200",
            ),
            BadgeColor::Default | BadgeColor::Accent => (
                "bg-stone-50 dark:bg-stone-900",
                "border-stone-500",
                "text-stone-700 dark:text-stone-300",
                "text-stone-800 dark:text-stone-200",
            ),
        };
        out.push_str("<div class=\"");
        out.push_str(bg);
        out.push_str(" border-l-4 ");
        out.push_str(border);
        out.push_str(" p-4 rounded-sm\">\n");
        if !self.title.is_empty() {
            out.push_str("\n    <p class=\"font-medium ");
            out.push_str(title_cls);
            out.push_str("\">");
            out.push_str(&html_escape(&self.title));
            out.push_str("</p>\n");
        }
        out.push_str("\n    <p class=\"text-sm ");
        out.push_str(text);
        out.push_str("\">");
        out.push_str(&html_escape(&self.message));
        out.push_str("</p>\n</div>\n");
    }
}

/// An HTML table (templates/table.html). Cells may carry raw HTML (the
/// reference template treats cells as pre-trusted `template.HTML`); headers
/// are escaped.
pub struct Table {
    /// Column headers (escaped).
    pub headers: Vec<String>,
    /// Rows of raw-HTML cells.
    pub rows: Vec<Vec<String>>,
    /// Whether odd rows are striped.
    pub striped: bool,
}

impl Table {
    /// New striped table.
    #[must_use]
    pub fn new(headers: Vec<String>) -> Self {
        Self {
            headers,
            rows: Vec::new(),
            striped: true,
        }
    }

    /// Appends a row.
    pub fn add_row(&mut self, cells: Vec<String>) -> &mut Self {
        self.rows.push(cells);
        self
    }

    /// Enables/disables striping.
    #[must_use]
    pub const fn with_striped(mut self, striped: bool) -> Self {
        self.striped = striped;
        self
    }
}

impl Renderable for Table {
    fn render(&self, out: &mut String, _ids: &mut ChartIdGen) {
        out.push_str("<div class=\"overflow-x-auto\">\n    <table class=\"w-full text-sm\">\n        <thead>\n            <tr class=\"border-b border-stone-200 dark:border-stone-700\">\n");
        for h in &self.headers {
            out.push_str("\n                <th class=\"px-4 py-3 text-left font-medium text-stone-500 dark:text-stone-400\">");
            out.push_str(&html_escape(h));
            out.push_str("</th>\n");
        }
        out.push_str("\n            </tr>\n        </thead>\n        <tbody>\n");
        for (i, row) in self.rows.iter().enumerate() {
            out.push_str("\n            <tr class=\"border-b border-stone-100 dark:border-stone-800 ");
            // Template condition: {{if and $.Striped (odd $i)}} (funcMap "odd": i%2==1).
            if self.striped && i % 2 == 1 {
                out.push_str("bg-stone-50 dark:bg-stone-800/50");
            }
            out.push_str("\">\n");
            for cell in row {
                out.push_str("\n                <td class=\"px-4 py-3 text-stone-700 dark:text-stone-300\">");
                out.push_str(cell);
                out.push_str("</td>\n");
            }
            out.push_str("\n            </tr>\n");
        }
        out.push_str("\n        </tbody>\n    </table>\n</div>\n");
    }
}
