//! Color themes for plot pages — port of Go `plotpage/theme.go`.

/// A color theme for visualizations (Go `plotpage.Theme`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    /// The light color theme (Go `ThemeLight`).
    Light,
    /// The dark color theme (Go `ThemeDark`) — the default for plot output.
    #[default]
    Dark,
}

/// All theme-specific styling values (Go `plotpage.ThemeConfig`, theme.go:14).
#[derive(Debug, Clone)]
pub struct ThemeConfig {
    /// Page background color.
    pub background: &'static str,
    /// Card/surface background color.
    pub surface: &'static str,
    /// Hovered surface background color.
    pub surface_hover: &'static str,
    /// Border color.
    pub border: &'static str,
    /// Subtle border color.
    pub border_subtle: &'static str,
    /// Primary text color.
    pub text_primary: &'static str,
    /// Secondary text color.
    pub text_secondary: &'static str,
    /// Muted text color.
    pub text_muted: &'static str,
    /// Accent color (brown palette matching Radix UI).
    pub accent: &'static str,
    /// Hovered accent color.
    pub accent_hover: &'static str,
    /// Subtle accent color.
    pub accent_subtle: &'static str,
    /// Text color on accent backgrounds.
    pub accent_text: &'static str,
    /// Semantic success color.
    pub success: &'static str,
    /// Subtle success background.
    pub success_subtle: &'static str,
    /// Semantic warning color.
    pub warning: &'static str,
    /// Subtle warning background.
    pub warning_subtle: &'static str,
    /// Semantic error color.
    pub error: &'static str,
    /// Subtle error background.
    pub error_subtle: &'static str,
    /// Semantic info color.
    pub info: &'static str,
    /// Subtle info background.
    pub info_subtle: &'static str,
    /// Chart canvas background.
    pub chart_background: &'static str,
    /// Chart grid-line color.
    pub chart_grid: &'static str,
    /// Chart axis-line color.
    pub chart_axis: &'static str,
    /// Chart text color.
    pub chart_text: &'static str,
    /// Muted chart text color.
    pub chart_text_muted: &'static str,
    /// ECharts theme name (empty → go-echarts default `"white"`).
    pub echarts_theme: &'static str,
}

/// Semantic chart colors (Go `ChartPalette.Semantic`).
#[derive(Debug, Clone)]
pub struct SemanticColors {
    /// "Good" semantic color.
    pub good: &'static str,
    /// "Warning" semantic color.
    pub warning: &'static str,
    /// "Bad" semantic color.
    pub bad: &'static str,
}

/// A consistent chart color palette (Go `plotpage.ChartPalette`).
#[derive(Debug, Clone)]
pub struct ChartPalette {
    /// Main series colors.
    pub primary: [&'static str; 10],
    /// Secondary/accent colors.
    pub secondary: [&'static str; 10],
    /// Semantic good/warning/bad colors.
    pub semantic: SemanticColors,
}

/// Returns the configuration for a theme (Go `GetThemeConfig`, theme.go:66).
#[must_use]
pub fn get_theme_config(theme: Theme) -> ThemeConfig {
    match theme {
        Theme::Dark => DARK_THEME,
        Theme::Light => LIGHT_THEME,
    }
}

/// Returns the chart palette for a theme (Go `GetChartPalette`, theme.go:78).
#[must_use]
pub fn get_chart_palette(theme: Theme) -> ChartPalette {
    match theme {
        Theme::Dark => DARK_CHART_PALETTE,
        Theme::Light => LIGHT_CHART_PALETTE,
    }
}

/// Go `lightTheme` (theme.go:89).
const LIGHT_THEME: ThemeConfig = ThemeConfig {
    background: "#fafaf9",
    surface: "#ffffff",
    surface_hover: "#f5f5f4",
    border: "#e7e5e4",
    border_subtle: "#d6d3d1",
    text_primary: "#1c1917",
    text_secondary: "#44403c",
    text_muted: "#78716c",
    accent: "#a16207",
    accent_hover: "#854d0e",
    accent_subtle: "#fef3c7",
    accent_text: "#ffffff",
    success: "#16a34a",
    success_subtle: "#dcfce7",
    warning: "#ca8a04",
    warning_subtle: "#fef9c3",
    error: "#dc2626",
    error_subtle: "#fee2e2",
    info: "#2563eb",
    info_subtle: "#dbeafe",
    chart_background: "transparent",
    chart_grid: "#e7e5e4",
    chart_axis: "#a8a29e",
    chart_text: "#44403c",
    chart_text_muted: "#78716c",
    echarts_theme: "",
};

/// Go `darkTheme` (theme.go:128).
const DARK_THEME: ThemeConfig = ThemeConfig {
    background: "#0c0a09",
    surface: "#1c1917",
    surface_hover: "#292524",
    border: "#44403c",
    border_subtle: "#57534e",
    text_primary: "#fafaf9",
    text_secondary: "#d6d3d1",
    text_muted: "#a8a29e",
    accent: "#d97706",
    accent_hover: "#f59e0b",
    accent_subtle: "#451a03",
    accent_text: "#ffffff",
    success: "#22c55e",
    success_subtle: "#14532d",
    warning: "#eab308",
    warning_subtle: "#422006",
    error: "#ef4444",
    error_subtle: "#450a0a",
    info: "#3b82f6",
    info_subtle: "#1e3a8a",
    chart_background: "transparent",
    chart_grid: "#44403c",
    chart_axis: "#57534e",
    chart_text: "#d6d3d1",
    chart_text_muted: "#a8a29e",
    echarts_theme: "",
};

/// Go `lightChartPalette` (theme.go:167).
const LIGHT_CHART_PALETTE: ChartPalette = ChartPalette {
    primary: [
        "#a16207", "#0369a1", "#4d7c0f", "#7c3aed", "#be185d", "#0891b2", "#c2410c", "#4338ca",
        "#15803d", "#b91c1c",
    ],
    secondary: [
        "#d97706", "#0284c7", "#65a30d", "#8b5cf6", "#db2777", "#06b6d4", "#ea580c", "#6366f1",
        "#16a34a", "#dc2626",
    ],
    semantic: SemanticColors {
        good: "#16a34a",
        warning: "#ca8a04",
        bad: "#dc2626",
    },
};

/// Go `darkChartPalette` (theme.go:203).
const DARK_CHART_PALETTE: ChartPalette = ChartPalette {
    primary: [
        "#fbbf24", "#38bdf8", "#a3e635", "#a78bfa", "#f472b6", "#22d3ee", "#fb923c", "#818cf8",
        "#4ade80", "#f87171",
    ],
    secondary: [
        "#f59e0b", "#0ea5e9", "#84cc16", "#8b5cf6", "#ec4899", "#06b6d4", "#f97316", "#6366f1",
        "#22c55e", "#ef4444",
    ],
    semantic: SemanticColors {
        good: "#22c55e",
        warning: "#eab308",
        bad: "#ef4444",
    },
};
