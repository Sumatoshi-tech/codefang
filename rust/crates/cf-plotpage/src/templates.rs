//! Rendered template text — port of Go `plotpage/templates.go` +
//! `templates/*.html`.
//!
//! Go parses the template files with `html/template` and interpolates data at
//! run time. The literal bytes between `{{…}}` actions are emitted verbatim,
//! with two context-sensitive transformations that `html/template` applies to
//! the template TEXT itself (verified against the live Go binary's output):
//!
//! * the CSS block comment in `page.html` (`/* Override Tailwind's … */`) is
//!   replaced by a single space; and
//! * the JS line comment in `scripts.html` (`// Resize echarts …`) is removed
//!   entirely (its line keeps only the leading indentation).
//!
//! The Rust functions below carry the POST-transformation literal bytes, so
//! their output is byte-identical to Go's `renderTemplate` results. Data slots
//! that Go HTML-escapes (`{{.Title}}` …) route through
//! [`crate::components::html_escape`]; `template.HTML`/`template.URL`/
//! `template.CSS` slots (chart content, hint items, the logo data URI,
//! ExtraCSS) are passed through raw, exactly as in Go.

use crate::components::html_escape;
use crate::multipage::PageMeta;
use crate::theme::ThemeConfig;

/// The embedded logo (Go `//go:embed assets/uast_small.png`).
const LOGO_PNG: &[u8] = include_bytes!("../assets/uast_small.png");

/// Standard-alphabet base64 with `=` padding (Go
/// `base64.StdEncoding.EncodeToString`).
#[must_use]
fn base64_std(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// The logo as a data URI (Go `plotpage.LogoDataURI`, templates.go:99).
#[must_use]
pub fn logo_data_uri() -> String {
    format!("data:image/png;base64,{}", base64_std(LOGO_PNG))
}

/// Header template data (Go `headerData`).
pub struct HeaderData<'a> {
    /// Project name (header brand + img alt).
    pub project_name: &'a str,
    /// Brand subtitle.
    pub subtitle: &'a str,
    /// Page title (the centered `<h2>`).
    pub title: &'a str,
    /// Optional page description.
    pub description: &'a str,
    /// Whether the theme-toggle button is shown.
    pub show_theme_toggle: bool,
}

/// Renders `templates/header.html`.
#[must_use]
pub fn render_header(d: &HeaderData<'_>) -> String {
    let mut out = String::new();
    out.push_str("<header class=\"mb-8\">\n    <div\n        class=\"flex items-center justify-between py-4 border-b border-stone-200 dark:border-stone-800\"\n    >\n        <div class=\"flex items-center gap-3\">\n            <img\n                src=\"");
    // template.URL skips URL sanitization but still passes the quoted-attr
    // HTML escaper, so the base64 `+` bytes render as `&#43;` (as in Go).
    out.push_str(&html_escape(&logo_data_uri()));
    out.push_str("\"\n                alt=\"");
    out.push_str(&html_escape(d.project_name));
    out.push_str("\"\n                class=\"w-8 h-8\"\n            />\n            <div>\n                <h1\n                    class=\"text-lg font-semibold text-stone-900 dark:text-stone-50\"\n                >\n                    ");
    out.push_str(&html_escape(d.project_name));
    out.push_str("\n                </h1>\n                <p class=\"text-xs text-stone-500 dark:text-stone-400\">\n                    ");
    out.push_str(&html_escape(d.subtitle));
    out.push_str("\n                </p>\n            </div>\n        </div>\n        ");
    if d.show_theme_toggle {
        out.push_str("\n        <button\n            onclick=\"toggleTheme()\"\n            class=\"p-2 rounded-sm text-stone-500 dark:text-stone-400 hover:text-stone-700 dark:hover:text-stone-200 hover:bg-stone-100 dark:hover:bg-stone-800 transition-colors\"\n            title=\"Toggle theme\"\n        >\n            <svg\n                class=\"w-5 h-5 hidden dark:block\"\n                viewBox=\"0 0 24 24\"\n                fill=\"none\"\n                stroke=\"currentColor\"\n                stroke-width=\"2\"\n            >\n                <circle cx=\"12\" cy=\"12\" r=\"5\" />\n                <path\n                    d=\"M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42\"\n                />\n            </svg>\n            <svg\n                class=\"w-5 h-5 block dark:hidden\"\n                viewBox=\"0 0 24 24\"\n                fill=\"none\"\n                stroke=\"currentColor\"\n                stroke-width=\"2\"\n            >\n                <path d=\"M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z\" />\n            </svg>\n        </button>\n        ");
    }
    out.push_str("\n    </div>\n    <div class=\"mt-6 text-center\">\n        <h2\n            class=\"text-2xl font-semibold tracking-tight text-stone-900 dark:text-stone-50\"\n        >\n            ");
    out.push_str(&html_escape(d.title));
    out.push_str("\n        </h2>\n        ");
    if !d.description.is_empty() {
        out.push_str("\n        <p class=\"mt-2 text-sm text-stone-500 dark:text-stone-400\">\n            ");
        out.push_str(&html_escape(d.description));
        out.push_str("\n        </p>\n        ");
    }
    out.push_str("\n    </div>\n</header>\n");
    out
}

/// Renders `templates/section.html`. `chart_html` and `hint_items` are raw
/// HTML (Go `template.HTML`); the title/subtitle/hint-title are escaped.
#[must_use]
pub fn render_section(
    title: &str,
    subtitle: &str,
    chart_html: &str,
    hint_title: &str,
    hint_items: &[String],
) -> String {
    let mut out = String::new();
    out.push_str("<section class=\"bg-white dark:bg-stone-900 rounded-sm border border-stone-200 dark:border-stone-700 shadow-sm overflow-hidden\">\n    <div class=\"px-5 py-4 border-b border-stone-100 dark:border-stone-800\">\n        <h2 class=\"text-lg font-medium text-stone-900 dark:text-stone-50\">");
    out.push_str(&html_escape(title));
    out.push_str("</h2>\n        <p class=\"mt-0.5 text-sm text-stone-500 dark:text-stone-400\">");
    out.push_str(&html_escape(subtitle));
    out.push_str("</p>\n    </div>\n    <div class=\"p-5 overflow-x-auto\">\n        <div class=\"chart-container\">");
    out.push_str(chart_html);
    out.push_str("</div>\n    </div>\n");
    // Go renders the hint block when the hint has any items (plotpage.go:173).
    if !hint_items.is_empty() {
        out.push_str("\n    <div class=\"mx-5 mb-5 p-4 bg-stone-50 dark:bg-stone-800 border-l-4 border-accent rounded-sm\">\n");
        if !hint_title.is_empty() {
            out.push_str("\n        <p class=\"font-medium text-stone-900 dark:text-stone-100 text-sm\">");
            out.push_str(&html_escape(hint_title));
            out.push_str("</p>\n");
        }
        out.push_str("\n        <ul class=\"mt-2 space-y-1 text-sm text-stone-600 dark:text-stone-300 list-disc list-inside\">\n");
        for item in hint_items {
            out.push_str("\n            <li>");
            out.push_str(item);
            out.push_str("</li>\n");
        }
        out.push_str("\n        </ul>\n    </div>\n");
    }
    out.push_str("\n</section>\n");
    out
}

/// `templates/nav.html` rendered (no data slots).
pub const NAV_HTML: &str = "<nav class=\"mb-4\">\n    <a\n        href=\"index.html\"\n        class=\"inline-flex items-center text-sm text-stone-500 dark:text-stone-400\n               hover:text-accent transition-colors\"\n    >\n        <svg\n            class=\"w-4 h-4 mr-1\"\n            viewBox=\"0 0 24 24\"\n            fill=\"none\"\n            stroke=\"currentColor\"\n            stroke-width=\"2\"\n        >\n            <path d=\"M15 18l-6-6 6-6\" />\n        </svg>\n        Back to index\n    </a>\n</nav>\n";

/// `templates/scripts.html` rendered (no data slots). The JS line comment in
/// the source template is removed by `html/template`'s script-context
/// sanitizer; only its leading indentation survives.
pub const SCRIPTS_HTML: &str = "<script>\n    function switchTab(groupId, tabId) {\n        const group = document.querySelector('[data-tabs=\"' + groupId + '\"]');\n        if (!group) return;\n\n        group.querySelectorAll('[role=\"tab\"]').forEach((btn) => {\n            const isActive = btn.dataset.tab === tabId;\n            btn.setAttribute(\"aria-selected\", isActive);\n            btn.classList.toggle(\"border-accent\", isActive);\n            btn.classList.toggle(\"text-accent\", isActive);\n            btn.classList.toggle(\"border-transparent\", !isActive);\n            btn.classList.toggle(\"text-stone-500\", !isActive);\n            btn.classList.toggle(\"dark:text-stone-400\", !isActive);\n        });\n\n        group.querySelectorAll('[role=\"tabpanel\"]').forEach((panel) => {\n            panel.classList.toggle(\"hidden\", panel.dataset.panel !== tabId);\n        });\n\n        \n        setTimeout(function () {\n            const activePanel = group.querySelector(\n                '[data-panel=\"' + tabId + '\"]',\n            );\n            if (activePanel) {\n                activePanel\n                    .querySelectorAll(\"[_echarts_instance_]\")\n                    .forEach(function (el) {\n                        const chart = echarts.getInstanceByDom(el);\n                        if (chart) chart.resize();\n                    });\n            }\n        }, 0);\n    }\n\n    function toggleTheme() {\n        const html = document.documentElement;\n        const isDark = html.classList.contains(\"dark\");\n        html.classList.toggle(\"dark\", !isDark);\n        localStorage.setItem(\"theme\", isDark ? \"light\" : \"dark\");\n    }\n\n    (function () {\n        const saved = localStorage.getItem(\"theme\");\n        if (saved === \"dark\") {\n            document.documentElement.classList.add(\"dark\");\n        }\n    })();\n</script>\n";

/// Renders `templates/index.html` — the navigation-card grid for the index
/// page.
#[must_use]
pub fn render_index_content(pages: &[PageMeta]) -> String {
    let mut out = String::new();
    out.push_str("<div class=\"grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4\">\n    ");
    for page in pages {
        out.push_str("\n    <a\n        href=\"");
        out.push_str(&html_escape(&page.id));
        out.push_str(".html\"\n        class=\"block p-5 rounded-sm border border-stone-200 dark:border-stone-800\n               bg-white dark:bg-stone-900\n               hover:border-accent dark:hover:border-accent\n               hover:bg-stone-50 dark:hover:bg-stone-800\n               transition-colors group\"\n    >\n        <h3\n            class=\"text-base font-semibold text-stone-900 dark:text-stone-50\n                   group-hover:text-accent transition-colors\"\n        >\n            ");
        out.push_str(&html_escape(&page.title));
        out.push_str("\n        </h3>\n        ");
        if !page.description.is_empty() {
            out.push_str("\n        <p class=\"mt-1 text-sm text-stone-500 dark:text-stone-400\">\n            ");
            out.push_str(&html_escape(&page.description));
            out.push_str("\n        </p>\n        ");
        }
        out.push_str("\n        <span\n            class=\"mt-3 inline-flex items-center text-xs font-medium text-accent\"\n        >\n            View report →\n        </span>\n    </a>\n    ");
    }
    out.push_str("\n</div>\n");
    out
}

/// Renders `templates/page.html` — the full HTML document shell. The CSS
/// block comment in the source template is replaced by one space by
/// `html/template`'s style-context sanitizer.
#[must_use]
pub fn render_page(
    title: &str,
    project_name: &str,
    dark_class: &str,
    theme: &ThemeConfig,
    extra_css: &str,
    header: &str,
    content: &str,
    scripts: &str,
) -> String {
    let mut out = String::new();
    out.push_str("<!doctype html>\n<html class=\"");
    out.push_str(&html_escape(dark_class));
    out.push_str("\">\n    <head>\n        <meta charset=\"utf-8\" />\n        <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n        <title>");
    out.push_str(&html_escape(title));
    out.push_str(" - ");
    out.push_str(&html_escape(project_name));
    out.push_str("</title>\n        <script src=\"https://cdn.tailwindcss.com\"></script>\n        <script src=\"https://go-echarts.github.io/go-echarts-assets/assets/echarts.min.js\"></script>\n        <script>\n            tailwind.config = {\n                darkMode: \"class\",\n                theme: {\n                    extend: {\n                        colors: {\n                            accent: {\n                                DEFAULT: \"");
    out.push_str(theme.accent);
    out.push_str("\",\n                                hover: \"");
    out.push_str(theme.accent_hover);
    out.push_str("\",\n                                subtle: \"");
    out.push_str(theme.accent_subtle);
    out.push_str("\",\n                            },\n                        },\n                        borderRadius: {\n                            sm: \"0.25rem\",\n                        },\n                    },\n                },\n            };\n        </script>\n        <style>\n                    html { font-size: 95%; }\n                    * { transition: background-color 0.2s, border-color 0.2s, color 0.2s; }\n                    .chart-container { min-width: 600px; }\n                    ::-webkit-scrollbar { width: 8px; height: 8px; }\n                    ::-webkit-scrollbar-track { background: transparent; }\n                    ::-webkit-scrollbar-thumb {\n                        background: ");
    out.push_str(theme.border);
    out.push_str(";\n                        border-radius: 4px;\n                    }\n                    ::-webkit-scrollbar-thumb:hover { background: ");
    out.push_str(theme.border_subtle);
    // The next line carried the CSS comment in the Go template; html/template
    // replaces it with a single space (20 spaces of indent + 1 space survive).
    out.push_str("; }\n                     \n                    .tab-panel .container,\n                    .card .container {\n                        max-width: none;\n                        padding: 0;\n                        margin: 0;\n                        width: 100%;\n                    }\n            ");
    out.push_str(extra_css);
    out.push_str("\n        </style>\n    </head>\n    <body\n        class=\"min-h-screen bg-stone-50 dark:bg-stone-950 text-stone-900 dark:text-stone-50 antialiased\"\n    >\n        <div class=\"max-w-6xl mx-auto px-4 py-6\">\n            ");
    out.push_str(header);
    out.push_str("\n            <main class=\"space-y-6\">");
    out.push_str(content);
    out.push_str("</main>\n        </div>\n        ");
    out.push_str(scripts);
    out.push_str("\n    </body>\n</html>\n");
    out
}
