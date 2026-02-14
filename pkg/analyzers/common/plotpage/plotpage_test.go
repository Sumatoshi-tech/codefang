package plotpage

import (
	"bytes"
	"strings"
	"testing"
)

func TestPageRenderDarkDefault(t *testing.T) {
	t.Parallel()

	page := NewPage("Test Page", "Test description")
	page.Add(Section{
		Title:    "Test Section",
		Subtitle: "Test subtitle",
		Hint: Hint{
			Title: "Test hint",
			Items: []string{"Item 1", "Item 2"},
		},
	})

	var buf bytes.Buffer

	err := page.Render(&buf)
	if err != nil {
		t.Fatalf("Render failed: %v", err)
	}

	html := buf.String()

	// Verify Tailwind CDN is included.
	if !strings.Contains(html, "cdn.tailwindcss.com") {
		t.Error("Expected Tailwind CDN to be included")
	}

	// Verify dark theme is default.
	if !strings.Contains(html, `class="dark"`) {
		t.Error("Dark theme should be default")
	}

	// Verify title and description.
	if !strings.Contains(html, "Test Page") {
		t.Error("Expected page title")
	}

	if !strings.Contains(html, "Test description") {
		t.Error("Expected page description")
	}

	// Verify section.
	if !strings.Contains(html, "Test Section") {
		t.Error("Expected section title")
	}

	// Verify dark theme Tailwind classes are present.
	if !strings.Contains(html, "dark:bg-stone-950") {
		t.Error("Expected dark mode classes")
	}
}

func TestPageRenderLight(t *testing.T) {
	t.Parallel()

	page := NewPage("Light Page", "Light theme test")
	page.WithTheme(ThemeLight)

	var buf bytes.Buffer

	err := page.Render(&buf)
	if err != nil {
		t.Fatalf("Render failed: %v", err)
	}

	html := buf.String()

	// Verify no dark class on html for light theme.
	if strings.Contains(html, `class="dark"`) {
		t.Error("Light theme should not have dark class")
	}

	// Verify Tailwind classes.
	if !strings.Contains(html, "bg-stone-50") {
		t.Error("Expected stone background class for light theme")
	}
}

func TestThemeConfig(t *testing.T) {
	t.Parallel()

	light := GetThemeConfig(ThemeLight)
	dark := GetThemeConfig(ThemeDark)

	if light.Background == dark.Background {
		t.Error("Light and dark themes should have different backgrounds")
	}

	if light.TextPrimary == dark.TextPrimary {
		t.Error("Light and dark themes should have different text colors")
	}
}

func TestChartPalette(t *testing.T) {
	t.Parallel()

	light := GetChartPalette(ThemeLight)
	dark := GetChartPalette(ThemeDark)

	if len(light.Primary) == 0 {
		t.Error("Light palette should have primary colors")
	}

	if len(dark.Primary) == 0 {
		t.Error("Dark palette should have primary colors")
	}

	if light.Primary[0] == dark.Primary[0] {
		t.Error("Light and dark palettes should have different primary colors")
	}
}

func TestBadgeRender(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		badge    *Badge
		contains string
	}{
		{
			name:     "default badge",
			badge:    NewBadge("Test"),
			contains: "bg-stone-100",
		},
		{
			name:     "success soft badge",
			badge:    NewBadge("Success").WithColor(BadgeSuccess),
			contains: "bg-green-100",
		},
		{
			name:     "error solid badge",
			badge:    NewBadge("Error").WithVariant(BadgeSolid).WithColor(BadgeError),
			contains: "bg-red-600",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			var buf bytes.Buffer

			err := tt.badge.Render(&buf)
			if err != nil {
				t.Fatalf("Render failed: %v", err)
			}

			if !strings.Contains(buf.String(), tt.contains) {
				t.Errorf("Expected %q in output: %s", tt.contains, buf.String())
			}
		})
	}
}

func TestCardRender(t *testing.T) {
	t.Parallel()

	card := NewCard("Card Title", "Card subtitle")
	card.WithContent(NewText("Card content"))

	var buf bytes.Buffer

	err := card.Render(&buf)
	if err != nil {
		t.Fatalf("Render failed: %v", err)
	}

	html := buf.String()
	if !strings.Contains(html, "Card Title") {
		t.Error("Expected card title")
	}

	if !strings.Contains(html, "Card subtitle") {
		t.Error("Expected card subtitle")
	}

	if !strings.Contains(html, "Card content") {
		t.Error("Expected card content")
	}
}

func TestTableRender(t *testing.T) {
	t.Parallel()

	table := NewTable([]string{"Name", "Value"})
	table.AddRow("foo", "123")
	table.AddRow("bar", "456")

	var buf bytes.Buffer

	err := table.Render(&buf)
	if err != nil {
		t.Fatalf("Render failed: %v", err)
	}

	html := buf.String()
	if !strings.Contains(html, "<table") {
		t.Error("Expected table element")
	}

	if !strings.Contains(html, "Name") {
		t.Error("Expected header")
	}

	if !strings.Contains(html, "foo") {
		t.Error("Expected row data")
	}
}

func TestAlertRender(t *testing.T) {
	t.Parallel()

	alert := NewAlert("Warning", "This is a warning", BadgeWarning)

	var buf bytes.Buffer

	err := alert.Render(&buf)
	if err != nil {
		t.Fatalf("Render failed: %v", err)
	}

	html := buf.String()
	if !strings.Contains(html, "Warning") {
		t.Error("Expected alert title")
	}

	if !strings.Contains(html, "border-yellow-500") {
		t.Error("Expected warning border color")
	}
}

func TestGridRender(t *testing.T) {
	t.Parallel()

	grid := NewGrid(2, NewText("Item 1"), NewText("Item 2"))

	var buf bytes.Buffer

	err := grid.Render(&buf)
	if err != nil {
		t.Fatalf("Render failed: %v", err)
	}

	html := buf.String()
	if !strings.Contains(html, "grid-cols-1 md:grid-cols-2") {
		t.Error("Expected 2-column grid classes")
	}
}
