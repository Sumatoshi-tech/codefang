package plotpage

// Theme represents a color theme for visualizations.
type Theme string

const (
	// ThemeLight is the light color theme.
	ThemeLight Theme = "light"
	// ThemeDark is the dark color theme.
	ThemeDark Theme = "dark"
)

// ThemeConfig holds all theme-specific styling values.
type ThemeConfig struct {
	// Base colors.
	Background   string
	Surface      string
	SurfaceHover string
	Border       string
	BorderSubtle string

	// Text colors.
	TextPrimary   string
	TextSecondary string
	TextMuted     string

	// Accent colors (brown palette matching Radix UI).
	Accent       string
	AccentHover  string
	AccentSubtle string
	AccentText   string

	// Semantic colors.
	Success       string
	SuccessSubtle string
	Warning       string
	WarningSubtle string
	Error         string
	ErrorSubtle   string
	Info          string
	InfoSubtle    string

	// Chart-specific.
	ChartBackground string
	ChartGrid       string
	ChartAxis       string
	ChartText       string
	ChartTextMuted  string

	// ECharts theme name.
	EChartsTheme string
}

// ChartPalette returns a consistent color palette for charts.
type ChartPalette struct {
	Primary   []string // Main series colors.
	Secondary []string // Secondary/accent colors.
	Semantic  struct {
		Good    string
		Warning string
		Bad     string
	}
}

// GetThemeConfig returns the configuration for a given theme.
func GetThemeConfig(theme Theme) ThemeConfig {
	switch theme {
	case ThemeDark:
		return darkTheme
	case ThemeLight:
		return lightTheme
	default:
		return lightTheme
	}
}

// GetChartPalette returns the chart color palette for a given theme.
func GetChartPalette(theme Theme) ChartPalette {
	switch theme {
	case ThemeDark:
		return darkChartPalette
	case ThemeLight:
		return lightChartPalette
	default:
		return lightChartPalette
	}
}

var lightTheme = ThemeConfig{
	// Base - warm neutrals.
	Background:   "#fafaf9", // stone-50.
	Surface:      "#ffffff",
	SurfaceHover: "#f5f5f4", // stone-100.
	Border:       "#e7e5e4", // stone-200.
	BorderSubtle: "#d6d3d1", // stone-300.

	// Text.
	TextPrimary:   "#1c1917", // stone-900.
	TextSecondary: "#44403c", // stone-700.
	TextMuted:     "#78716c", // stone-500.

	// Accent (brown - matching Radix UI brown).
	Accent:       "#a16207", // amber-700 / brown-ish.
	AccentHover:  "#854d0e", // amber-800.
	AccentSubtle: "#fef3c7", // amber-100.
	AccentText:   "#ffffff",

	// Semantic.
	Success:       "#16a34a", // green-600.
	SuccessSubtle: "#dcfce7", // green-100.
	Warning:       "#ca8a04", // yellow-600.
	WarningSubtle: "#fef9c3", // yellow-100.
	Error:         "#dc2626", // red-600.
	ErrorSubtle:   "#fee2e2", // red-100.
	Info:          "#2563eb", // blue-600.
	InfoSubtle:    "#dbeafe", // blue-100.

	// Chart.
	ChartBackground: "transparent",
	ChartGrid:       "#e7e5e4", // stone-200.
	ChartAxis:       "#a8a29e", // stone-400.
	ChartText:       "#44403c", // stone-700.
	ChartTextMuted:  "#78716c", // stone-500.

	EChartsTheme: "",
}

var darkTheme = ThemeConfig{
	// Base - dark warm neutrals.
	Background:   "#0c0a09", // stone-950.
	Surface:      "#1c1917", // stone-900.
	SurfaceHover: "#292524", // stone-800.
	Border:       "#44403c", // stone-700.
	BorderSubtle: "#57534e", // stone-600.

	// Text.
	TextPrimary:   "#fafaf9", // stone-50.
	TextSecondary: "#d6d3d1", // stone-300.
	TextMuted:     "#a8a29e", // stone-400.

	// Accent (brown - matching Radix UI brown in dark mode).
	Accent:       "#d97706", // amber-600.
	AccentHover:  "#f59e0b", // amber-500.
	AccentSubtle: "#451a03", // amber-950.
	AccentText:   "#ffffff",

	// Semantic.
	Success:       "#22c55e", // green-500.
	SuccessSubtle: "#14532d", // green-900.
	Warning:       "#eab308", // yellow-500.
	WarningSubtle: "#422006", // yellow-950.
	Error:         "#ef4444", // red-500.
	ErrorSubtle:   "#450a0a", // red-950.
	Info:          "#3b82f6", // blue-500.
	InfoSubtle:    "#1e3a8a", // blue-900.

	// Chart.
	ChartBackground: "transparent",
	ChartGrid:       "#44403c", // stone-700.
	ChartAxis:       "#57534e", // stone-600.
	ChartText:       "#d6d3d1", // stone-300.
	ChartTextMuted:  "#a8a29e", // stone-400.

	EChartsTheme: "",
}

var lightChartPalette = ChartPalette{
	Primary: []string{
		"#a16207", // amber-700 (accent).
		"#0369a1", // sky-700.
		"#4d7c0f", // lime-700.
		"#7c3aed", // violet-600.
		"#be185d", // pink-700.
		"#0891b2", // cyan-600.
		"#c2410c", // orange-700.
		"#4338ca", // indigo-700.
		"#15803d", // green-700.
		"#b91c1c", // red-700.
	},
	Secondary: []string{
		"#d97706", // amber-600.
		"#0284c7", // sky-600.
		"#65a30d", // lime-600.
		"#8b5cf6", // violet-500.
		"#db2777", // pink-600.
		"#06b6d4", // cyan-500.
		"#ea580c", // orange-600.
		"#6366f1", // indigo-500.
		"#16a34a", // green-600.
		"#dc2626", // red-600.
	},
	Semantic: struct {
		Good    string
		Warning string
		Bad     string
	}{
		Good:    "#16a34a", // green-600.
		Warning: "#ca8a04", // yellow-600.
		Bad:     "#dc2626", // red-600.
	},
}

var darkChartPalette = ChartPalette{
	Primary: []string{
		"#fbbf24", // amber-400 (accent).
		"#38bdf8", // sky-400.
		"#a3e635", // lime-400.
		"#a78bfa", // violet-400.
		"#f472b6", // pink-400.
		"#22d3ee", // cyan-400.
		"#fb923c", // orange-400.
		"#818cf8", // indigo-400.
		"#4ade80", // green-400.
		"#f87171", // red-400.
	},
	Secondary: []string{
		"#f59e0b", // amber-500.
		"#0ea5e9", // sky-500.
		"#84cc16", // lime-500.
		"#8b5cf6", // violet-500.
		"#ec4899", // pink-500.
		"#06b6d4", // cyan-500.
		"#f97316", // orange-500.
		"#6366f1", // indigo-500.
		"#22c55e", // green-500.
		"#ef4444", // red-500.
	},
	Semantic: struct {
		Good    string
		Warning string
		Bad     string
	}{
		Good:    "#22c55e", // green-500.
		Warning: "#eab308", // yellow-500.
		Bad:     "#ef4444", // red-500.
	},
}
