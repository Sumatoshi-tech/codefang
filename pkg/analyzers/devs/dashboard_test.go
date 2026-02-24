package devs

import (
	"bytes"
	"fmt"
	"strings"
	"testing"
	"time"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

// ticksToCanonicalReport converts legacy Ticks format to canonical CommitDevData+CommitsByTick.
func ticksToCanonicalReport(ticks map[int]map[int]*DevTick, names []string) analyze.Report {
	commitDevData := make(map[string]*CommitDevData)
	commitsByTick := make(map[int][]gitlib.Hash)

	n := 0

	for tick, devTicks := range ticks {
		for devID, dt := range devTicks {
			hash := fmt.Sprintf("%038x%02x", n, devID)
			n++

			cdd := &CommitDevData{
				Commits:   dt.Commits,
				Added:     dt.Added,
				Removed:   dt.Removed,
				Changed:   dt.Changed,
				AuthorID:  devID,
				Languages: dt.Languages,
			}

			commitDevData[hash] = cdd
			commitsByTick[tick] = append(commitsByTick[tick], gitlib.NewHash(hash))
		}
	}

	return analyze.Report{
		"CommitDevData":      commitDevData,
		"CommitsByTick":      commitsByTick,
		"ReversedPeopleDict": names,
		"TickSize":           24 * time.Hour,
	}
}

func TestGenerateDashboard(t *testing.T) {
	t.Parallel()

	ticks := map[int]map[int]*DevTick{
		0: {
			0: {
				LineStats: pkgplumbing.LineStats{Added: 100, Removed: 20, Changed: 10},
				Languages: map[string]pkgplumbing.LineStats{
					"Go":     {Added: 80, Removed: 15, Changed: 8},
					"Python": {Added: 20, Removed: 5, Changed: 2},
				},
				Commits: 5,
			},
			1: {
				LineStats: pkgplumbing.LineStats{Added: 50, Removed: 10, Changed: 5},
				Languages: map[string]pkgplumbing.LineStats{
					"Go": {Added: 50, Removed: 10, Changed: 5},
				},
				Commits: 3,
			},
		},
		1: {
			0: {
				LineStats: pkgplumbing.LineStats{Added: 200, Removed: 30, Changed: 15},
				Languages: map[string]pkgplumbing.LineStats{
					"Go": {Added: 200, Removed: 30, Changed: 15},
				},
				Commits: 8,
			},
		},
	}
	report := ticksToCanonicalReport(ticks, []string{"Alice", "Bob"})

	var buf bytes.Buffer

	err := GenerateDashboard(report, &buf)
	if err != nil {
		t.Fatalf("GenerateDashboard failed: %v", err)
	}

	output := buf.String()

	// Check for main dashboard elements.
	checks := []struct {
		name     string
		contains string
	}{
		{"dashboard title", "Developer Analytics Dashboard"},
		{"tabs component", "data-tabs"},
		{"overview tab", "Overview"},
		{"activity tab", "Activity Trends"},
		{"workload tab", "Workload Distribution"},
		{"languages tab", "Language Expertise"},
		{"busfactor tab", "Bus Factor"},
		{"churn tab", "Code Churn"},
		{"echarts", "echarts"},
		{"developer name Alice", "Alice"},
		{"developer name Bob", "Bob"},
	}

	for _, check := range checks {
		if !strings.Contains(output, check.contains) {
			t.Errorf("expected %s (%q) in output", check.name, check.contains)
		}
	}
}

func TestGenerateDashboard_EmptyData(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"Ticks":              map[int]map[int]*DevTick{},
		"ReversedPeopleDict": []string{},
		"TickSize":           24 * time.Hour,
	}

	var buf bytes.Buffer

	err := GenerateDashboard(report, &buf)
	if err != nil {
		t.Fatalf("GenerateDashboard with empty data failed: %v", err)
	}

	output := buf.String()
	if !strings.Contains(output, "Developer Analytics Dashboard") {
		t.Error("expected dashboard title in empty output")
	}
}

func TestGenerateDashboard_InvalidCommitDevDataType(t *testing.T) {
	t.Parallel()

	// Invalid CommitDevData type (string instead of map) is treated as absent; yields empty dashboard.
	report := analyze.Report{
		"CommitDevData":      "invalid",
		"CommitsByTick":      map[int][]gitlib.Hash{},
		"ReversedPeopleDict": []string{},
	}

	var buf bytes.Buffer

	err := GenerateDashboard(report, &buf)
	if err != nil {
		t.Fatalf("expected success with empty data, got: %v", err)
	}

	if !strings.Contains(buf.String(), "Developer Analytics Dashboard") {
		t.Error("expected dashboard title in output")
	}
}

func TestGenerateDashboard_InvalidPeopleDict(t *testing.T) {
	t.Parallel()

	report := ticksToCanonicalReport(map[int]map[int]*DevTick{}, []string{})
	report["ReversedPeopleDict"] = 123

	var buf bytes.Buffer

	err := GenerateDashboard(report, &buf)
	if err == nil {
		t.Error("expected error for invalid people dict")
	}
}

func TestNewDashboardData(t *testing.T) {
	t.Parallel()

	ticks := map[int]map[int]*DevTick{
		0: {
			0: {
				LineStats: pkgplumbing.LineStats{Added: 100, Removed: 20},
				Languages: map[string]pkgplumbing.LineStats{
					"Go": {Added: 100, Removed: 20},
				},
				Commits: 5,
			},
		},
		1: {
			0: {
				LineStats: pkgplumbing.LineStats{Added: 50, Removed: 10},
				Languages: map[string]pkgplumbing.LineStats{
					"Go": {Added: 50, Removed: 10},
				},
				Commits: 3,
			},
			1: {
				LineStats: pkgplumbing.LineStats{Added: 30, Removed: 5},
				Languages: map[string]pkgplumbing.LineStats{
					"Python": {Added: 30, Removed: 5},
				},
				Commits: 2,
			},
		},
	}
	report := ticksToCanonicalReport(ticks, []string{"Alice", "Bob"})

	data, err := newDashboardData(report)
	if err != nil {
		t.Fatalf("newDashboardData failed: %v", err)
	}

	// Check developers via metrics.
	if len(data.Metrics.Developers) != 2 {
		t.Errorf("expected 2 developers, got %d", len(data.Metrics.Developers))
	}

	// Find Alice (should have 8 commits total).
	var alice *DeveloperData

	for i := range data.Metrics.Developers {
		if data.Metrics.Developers[i].Name == "Alice" {
			alice = &data.Metrics.Developers[i]

			break
		}
	}

	if alice == nil {
		t.Fatal("Alice not found in developers")
	}

	if alice.Commits != 8 {
		t.Errorf("expected Alice to have 8 commits, got %d", alice.Commits)
	}

	if alice.Added != 150 {
		t.Errorf("expected Alice to have 150 added lines, got %d", alice.Added)
	}

	// Check languages.
	if len(data.Metrics.Languages) == 0 {
		t.Error("expected language data")
	}

	// Check aggregates.
	if data.Metrics.Aggregate.TotalCommits != 10 {
		t.Errorf("expected 10 total commits, got %d", data.Metrics.Aggregate.TotalCommits)
	}

	if data.Metrics.Aggregate.TotalDevelopers != 2 {
		t.Errorf("expected 2 total developers, got %d", data.Metrics.Aggregate.TotalDevelopers)
	}
}

func TestBusFactorCalculation(t *testing.T) {
	t.Parallel()

	ticks := map[int]map[int]*DevTick{
		0: {
			0: {
				LineStats: pkgplumbing.LineStats{Added: 950},
				Languages: map[string]pkgplumbing.LineStats{
					"Go": {Added: 950},
				},
				Commits: 10,
			},
			1: {
				LineStats: pkgplumbing.LineStats{Added: 50},
				Languages: map[string]pkgplumbing.LineStats{
					"Go": {Added: 50},
				},
				Commits: 1,
			},
		},
	}
	report := ticksToCanonicalReport(ticks, []string{"Hero Developer", "Minor Contributor"})

	data, err := newDashboardData(report)
	if err != nil {
		t.Fatalf("newDashboardData failed: %v", err)
	}

	// Check bus factor entries.
	if len(data.Metrics.BusFactor) == 0 {
		t.Fatal("expected bus factor entries")
	}

	// Find Go entry.
	var goEntry *BusFactorData

	for i := range data.Metrics.BusFactor {
		if data.Metrics.BusFactor[i].Language == "Go" {
			goEntry = &data.Metrics.BusFactor[i]

			break
		}
	}

	if goEntry == nil {
		t.Fatal("Go language not found in bus factor entries")
	}

	// Hero Developer should have ~95% ownership -> CRITICAL risk.
	if goEntry.RiskLevel != "CRITICAL" {
		t.Errorf("expected CRITICAL risk for Go, got %s (primary: %.1f%%)", goEntry.RiskLevel, goEntry.PrimaryPct)
	}

	if goEntry.PrimaryDevName != "Hero Developer" {
		t.Errorf("expected Hero Developer as primary, got %s", goEntry.PrimaryDevName)
	}
}

func TestAnonymizeNames(t *testing.T) {
	t.Parallel()

	names := []string{"John Doe", "Jane Smith", "Bob Wilson"}
	anon := anonymizeNames(names)

	if len(anon) != len(names) {
		t.Fatalf("expected %d names, got %d", len(names), len(anon))
	}

	for _, name := range anon {
		if !strings.HasPrefix(name, "Developer-") {
			t.Errorf("expected anonymized name, got %s", name)
		}

		if strings.Contains(name, "John") || strings.Contains(name, "Jane") || strings.Contains(name, "Bob") {
			t.Errorf("original name leaked in anonymized output: %s", name)
		}
	}
}

func TestGenerateIdentityAudit(t *testing.T) {
	t.Parallel()

	ticks := map[int]map[int]*DevTick{
		0: {
			0: {Commits: 10},
			1: {Commits: 5},
		},
		1: {
			0: {Commits: 5},
		},
	}
	report := ticksToCanonicalReport(ticks, []string{"Alice", "Bob"})

	entries := GenerateIdentityAudit(report)
	if len(entries) != 2 {
		t.Fatalf("expected 2 entries, got %d", len(entries))
	}

	// Should be sorted by commit count (Alice: 15, Bob: 5).
	if entries[0].CanonicalName != "Alice" {
		t.Errorf("expected Alice first, got %s", entries[0].CanonicalName)
	}

	if entries[0].CommitCount != 15 {
		t.Errorf("expected 15 commits for Alice, got %d", entries[0].CommitCount)
	}

	if entries[1].CanonicalName != "Bob" {
		t.Errorf("expected Bob second, got %s", entries[1].CanonicalName)
	}

	if entries[1].CommitCount != 5 {
		t.Errorf("expected 5 commits for Bob, got %d", entries[1].CommitCount)
	}
}

func TestFormatNumber(t *testing.T) {
	t.Parallel()

	tests := []struct {
		input    int
		expected string
	}{
		{0, "0"},
		{999, "999"},
		{1000, "1.0K"},
		{1500, "1.5K"},
		{10000, "10.0K"},
		{1000000, "1.0M"},
		{1500000, "1.5M"},
	}

	for _, tt := range tests {
		result := formatNumber(tt.input)
		if result != tt.expected {
			t.Errorf("formatNumber(%d) = %s, expected %s", tt.input, result, tt.expected)
		}
	}
}

func TestFormatSignedNumber(t *testing.T) {
	t.Parallel()

	tests := []struct {
		input    int
		expected string
	}{
		{0, "0"},
		{100, "+100"},
		{-100, "-100"},
		{1500, "+1.5K"},
		{-1500, "-1.5K"},
	}

	for _, tt := range tests {
		result := formatSignedNumber(tt.input)
		if result != tt.expected {
			t.Errorf("formatSignedNumber(%d) = %s, expected %s", tt.input, result, tt.expected)
		}
	}
}

func TestAnonymousID(t *testing.T) {
	t.Parallel()

	// First 26 should be single letters A-Z.
	for i := range 26 {
		id := anonymousID(i)
		if len(id) != 1 {
			t.Errorf("expected single letter for index %d, got %s", i, id)
		}
	}

	// Beyond 26 should be two letters.
	id := anonymousID(26)
	if len(id) != 2 {
		t.Errorf("expected two letters for index 26, got %s", id)
	}
}
