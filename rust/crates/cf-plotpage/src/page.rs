//! Page model + HTML renderer — port of Go `plotpage/plotpage.go`.

use crate::components::Renderable;
use crate::echarts::ChartIdGen;
use crate::templates::{render_header, render_page, render_section, HeaderData, SCRIPTS_HTML};
use crate::theme::{get_theme_config, Theme};

/// Chart dimensions and grid margins (Go `plotpage.Style`).
#[derive(Debug, Clone)]
pub struct Style {
    /// Chart width.
    pub width: String,
    /// Chart height.
    pub height: String,
    /// Grid left margin.
    pub grid_left: String,
    /// Grid right margin.
    pub grid_right: String,
    /// Grid top margin.
    pub grid_top: String,
    /// Grid bottom margin.
    pub grid_bottom: String,
}

impl Default for Style {
    /// Go `plotpage.DefaultStyle`.
    fn default() -> Self {
        Style {
            width: "100%".to_string(),
            height: "500px".to_string(),
            grid_left: "5%".to_string(),
            grid_right: "5%".to_string(),
            grid_top: "40".to_string(),
            grid_bottom: "15%".to_string(),
        }
    }
}

/// Interpretive guidance for a chart section (Go `plotpage.Hint`).
#[derive(Debug, Clone, Default)]
pub struct Hint {
    /// Hint heading (escaped on render).
    pub title: String,
    /// Hint bullet items — raw HTML, exactly like Go's `template.HTML` items.
    pub items: Vec<String>,
}

/// A chart section within a page (Go `plotpage.Section`).
pub struct Section {
    /// Section title (escaped).
    pub title: String,
    /// Section subtitle (escaped).
    pub subtitle: String,
    /// Interpretation hint (rendered when it has items).
    pub hint: Hint,
    /// The section chart/content.
    pub chart: Option<Box<dyn Renderable>>,
}

impl Section {
    /// Convenience constructor for a chart-only section.
    #[must_use]
    pub fn new(title: &str, subtitle: &str, chart: Box<dyn Renderable>, hint: Hint) -> Self {
        Section {
            title: title.to_string(),
            subtitle: subtitle.to_string(),
            hint,
            chart: Some(chart),
        }
    }
}

/// A complete visualization page (Go `plotpage.Page`).
pub struct Page {
    /// Page title.
    pub title: String,
    /// Page description.
    pub description: String,
    /// Project name shown in the header brand.
    pub project_name: String,
    /// Project subtitle shown in the header brand.
    pub project_subtitle: String,
    /// Whether the theme-toggle button is shown.
    pub show_theme_toggle: bool,
    /// Chart style defaults (kept for Go parity; rendering reads the charts'
    /// own sizes).
    pub style: Style,
    /// Color theme.
    pub theme: Theme,
    /// Page sections.
    pub sections: Vec<Section>,
}

impl Page {
    /// New page with Go `plotpage.NewPage` defaults.
    #[must_use]
    pub fn new(title: &str, description: &str) -> Self {
        Page {
            title: title.to_string(),
            description: description.to_string(),
            project_name: "Codefang".to_string(),
            project_subtitle: "Code Analysis".to_string(),
            show_theme_toggle: true,
            style: Style::default(),
            theme: Theme::Dark,
            sections: Vec::new(),
        }
    }

    /// Appends sections (Go `Page.Add`).
    pub fn add(&mut self, sections: Vec<Section>) {
        self.sections.extend(sections);
    }

    /// Renders the page as HTML (Go `Page.Render` via `HTMLRenderer`).
    #[must_use]
    pub fn render(&self) -> String {
        HtmlRenderer::default().render(self)
    }
}

/// Renders pages as HTML (Go `plotpage.HTMLRenderer`).
#[derive(Default)]
pub struct HtmlRenderer {
    /// Extra CSS appended into the page `<style>` (Go `ExtraCSS`,
    /// `template.CSS` — raw).
    pub extra_css: String,
}

impl HtmlRenderer {
    /// Renders the page (Go `HTMLRenderer.Render`, plotpage.go:107).
    #[must_use]
    pub fn render(&self, page: &Page) -> String {
        let theme_config = get_theme_config(page.theme);

        let header = render_header(&HeaderData {
            project_name: &page.project_name,
            subtitle: &page.project_subtitle,
            title: &page.title,
            description: &page.description,
            show_theme_toggle: page.show_theme_toggle,
        });

        // One deterministic chart-ID sequence per page render (Go draws random
        // IDs at chart construction; sequence order is identical).
        let mut ids = ChartIdGen::new();
        let mut sections_html = String::new();
        for section in &page.sections {
            let mut chart_html = String::new();
            if let Some(chart) = &section.chart {
                chart.render(&mut chart_html, &mut ids);
            }
            sections_html.push_str(&render_section(
                &section.title,
                &section.subtitle,
                &chart_html,
                &section.hint.title,
                &section.hint.items,
            ));
        }

        let dark_class = if page.theme == Theme::Dark { "dark" } else { "" };

        render_page(
            &page.title,
            &page.project_name,
            dark_class,
            &theme_config,
            &self.extra_css,
            &header,
            &sections_html,
            SCRIPTS_HTML,
        )
    }
}

/// Renders a standalone analyzer page with default settings (Go
/// `plotpage.RenderAnalyzerPage`).
#[must_use]
pub fn render_analyzer_page(title: &str, description: &str, sections: Vec<Section>) -> String {
    let mut page = Page::new(title, description);
    page.add(sections);
    page.render()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::echarts::{Chart, ChartKind};

    fn one_chart_page() -> Page {
        let mut page = Page::new("static/complexity", "");
        page.sections.push(Section::new(
            "Top",
            "Sub",
            Box::new(Chart::new(ChartKind::Bar)),
            Hint {
                title: "How to interpret:".to_string(),
                items: vec!["<strong>x</strong> = y".to_string()],
            },
        ));
        page
    }

    #[test]
    fn render_is_deterministic_across_runs() {
        assert_eq!(one_chart_page().render(), one_chart_page().render());
    }

    #[test]
    fn page_shell_matches_go_template_invariants() {
        let html = one_chart_page().render();
        // The html/template-transformed literals (verified against the live
        // Go binary's plot output).
        assert!(html.starts_with("<!doctype html>\n<html class=\"dark\">\n"));
        // CSS comment replaced by one space: 21-space line inside <style>.
        assert!(html.contains("; }\n                     \n                    .tab-panel .container,"));
        // JS comment removed: bare-indentation line inside the scripts block.
        assert!(html.contains("});\n\n        \n        setTimeout(function () {"));
        // Title slot: "<Title> - <ProjectName>".
        assert!(html.contains("<title>static/complexity - Codefang</title>"));
        // Hint items are raw HTML (template.HTML), not escaped.
        assert!(html.contains("<li><strong>x</strong> = y</li>"));
        // Chart snippet is the extracted echart-box form with the script and
        // the blank line left where the <style> block was stripped.
        assert!(html.contains("</div><script type=\"text/javascript\">\n    \"use strict\";\n"));
        assert!(html.contains("</script>\n\n</div>\n    </div>\n"));
        assert!(html.ends_with("</script>\n\n    </body>\n</html>\n"));
    }
}
