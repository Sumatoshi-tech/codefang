//! Multi-page renderer: per-analyzer HTML pages plus an index page with
//! navigation cards.

use std::fs;
use std::io;
use std::path::Path;

use crate::components::RawHtml;
use crate::page::{Page, Section};
use crate::templates::{render_index_content, NAV_HTML};
use crate::theme::Theme;

/// Index page constants.
const INDEX_FILE_NAME: &str = "index.html";
const INDEX_TITLE: &str = "Analysis Report";
const INDEX_DESCRIPTION: &str = "Select an analyzer to view its report.";

/// Metadata about a rendered analyzer page for the index.
#[derive(Debug, Clone, Default)]
pub struct PageMeta {
    /// Filename stem, e.g. `static-complexity`.
    pub id: String,
    /// Display title, e.g. `static/complexity`.
    pub title: String,
    /// Short description for the index card.
    pub description: String,
}

/// Produces per-analyzer HTML pages plus an index page.
pub struct MultiPageRenderer {
    /// Directory to write HTML files into.
    pub output_dir: String,
    /// Project/report title shown on every page.
    pub title: String,
    /// Color theme.
    pub theme: Theme,
}

impl MultiPageRenderer {
    /// Renders a single analyzer page to `<output_dir>/<id>.html`: standalone
    /// HTML with a navigation section prepended.
    ///
    /// # Errors
    /// Returns the I/O error when the output file cannot be written.
    pub fn render_analyzer_page(
        &self,
        id: &str,
        title: &str,
        sections: Vec<Section>,
    ) -> io::Result<()> {
        let mut page = Page::new(title, "");
        page.theme = self.theme;
        page.project_name.clone_from(&self.title);

        // Prepend navigation as a section with no title (just the nav HTML).
        let nav_section = Section {
            title: String::new(),
            subtitle: String::new(),
            hint: crate::page::Hint::default(),
            chart: Some(Box::new(RawHtml(NAV_HTML.to_string()))),
        };
        page.sections.push(nav_section);
        page.sections.extend(sections);

        let out_path = Path::new(&self.output_dir).join(format!("{id}.html"));
        fs::write(out_path, page.render())
    }

    /// Renders the index page with navigation cards to
    /// `<output_dir>/index.html`.
    ///
    /// # Errors
    /// Returns the I/O error when the output file cannot be written.
    pub fn render_index(&self, pages: &[PageMeta]) -> io::Result<()> {
        let mut page = Page::new(INDEX_TITLE, INDEX_DESCRIPTION);
        page.theme = self.theme;
        page.project_name.clone_from(&self.title);

        page.sections.push(Section {
            title: String::new(),
            subtitle: String::new(),
            hint: crate::page::Hint::default(),
            chart: Some(Box::new(RawHtml(render_index_content(pages)))),
        });

        let out_path = Path::new(&self.output_dir).join(INDEX_FILE_NAME);
        fs::write(out_path, page.render())
    }

    /// Scans the output dir for `*.html` files (excluding index.html), derives
    /// page metadata from filenames, and regenerates index.html.
    ///
    /// # Errors
    /// Returns the I/O error when the directory cannot be read or the index
    /// cannot be written.
    pub fn rebuild_index(&self) -> io::Result<()> {
        let mut pages: Vec<PageMeta> = Vec::new();
        for entry in fs::read_dir(&self.output_dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type()?.is_dir() || !name.ends_with(".html") || name == INDEX_FILE_NAME {
                continue;
            }
            let id = name.trim_end_matches(".html").to_string();
            let title = id.replace('-', "/");
            pages.push(PageMeta {
                id,
                title,
                description: String::new(),
            });
        }
        pages.sort_by(|a, b| a.title.cmp(&b.title));
        self.render_index(&pages)
    }
}
