// Package devs provides devs functionality.
package devs

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"sort"
	"strings"
	"time"

	"gopkg.in/yaml.v3"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/identity"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

// HistoryAnalyzer calculates per-developer line statistics across commit history.
type HistoryAnalyzer struct {
	l interface { //nolint:unused // used via dependency injection.
		Warnf(format string, args ...any)
	}
	Identity             *plumbing.IdentityDetector
	TreeDiff             *plumbing.TreeDiffAnalyzer
	Ticks                *plumbing.TicksSinceStart
	Languages            *plumbing.LanguagesDetectionAnalyzer
	LineStats            *plumbing.LinesStatsCalculator
	ticks                map[int]map[int]*DevTick //nolint:revive // intentional naming matches exported Ticks field.
	merges               map[gitlib.Hash]bool
	reversedPeopleDict   []string
	tickSize             time.Duration
	ConsiderEmptyCommits bool
	Anonymize            bool
}

// DevTick is the statistics for a development tick and a particular developer.
type DevTick struct {
	pkgplumbing.LineStats

	Languages map[string]pkgplumbing.LineStats
	Commits   int
}

// Configuration option keys for the devs analyzer.
const (
	ConfigDevsConsiderEmptyCommits = "Devs.ConsiderEmptyCommits"
	ConfigDevsAnonymize            = "Devs.Anonymize"

	defaultHoursPerDay = 24
)

// Name returns the name of the analyzer.
func (d *HistoryAnalyzer) Name() string {
	return "Devs"
}

// Flag returns the CLI flag for the analyzer.
func (d *HistoryAnalyzer) Flag() string {
	return "devs"
}

// Description returns a human-readable description of the analyzer.
func (d *HistoryAnalyzer) Description() string {
	return "Calculates the number of commits, added, removed and changed lines per developer through time."
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (d *HistoryAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{
		{
			Name:        ConfigDevsConsiderEmptyCommits,
			Description: "Take into account empty commits such as trivial merges.",
			Flag:        "empty-commits",
			Type:        pipeline.BoolConfigurationOption,
			Default:     false,
		},
		{
			Name:        ConfigDevsAnonymize,
			Description: "Anonymize developer names in output (e.g., Developer-A, Developer-B).",
			Flag:        "anonymize",
			Type:        pipeline.BoolConfigurationOption,
			Default:     false,
		},
	}
}

// Configure configures the analyzer with the given facts.
func (d *HistoryAnalyzer) Configure(facts map[string]any) error {
	if val, exists := facts[ConfigDevsConsiderEmptyCommits].(bool); exists {
		d.ConsiderEmptyCommits = val
	}

	if val, exists := facts[ConfigDevsAnonymize].(bool); exists {
		d.Anonymize = val
	}

	if val, exists := facts[identity.FactIdentityDetectorReversedPeopleDict].([]string); exists {
		d.reversedPeopleDict = val
	}

	if val, exists := facts[pkgplumbing.FactTickSize].(time.Duration); exists {
		d.tickSize = val
	}

	return nil
}

// Initialize prepares the analyzer for processing commits.
func (d *HistoryAnalyzer) Initialize(_ *gitlib.Repository) error {
	if d.tickSize == 0 {
		d.tickSize = defaultHoursPerDay * time.Hour // Default fallback.
	}

	d.ticks = map[int]map[int]*DevTick{}
	d.merges = map[gitlib.Hash]bool{}

	return nil
}

// Consume processes a single commit with the provided dependency results.
func (d *HistoryAnalyzer) Consume(ctx *analyze.Context) error {
	// OneShotMergeProcessor logic.
	commit := ctx.Commit
	shouldConsume := true
	commitHash := commit.Hash()

	if commit.NumParents() > 1 {
		if d.merges[commitHash] {
			shouldConsume = false
		} else {
			d.merges[commitHash] = true
		}
	}

	if !shouldConsume {
		return nil
	}

	author := d.Identity.AuthorID

	treeDiff := d.TreeDiff.Changes
	if len(treeDiff) == 0 && !d.ConsiderEmptyCommits {
		return nil
	}

	tick := d.Ticks.Tick

	devstick, exists := d.ticks[tick]
	if !exists {
		devstick = map[int]*DevTick{}
		d.ticks[tick] = devstick
	}

	dd, exists := devstick[author]
	if !exists {
		dd = &DevTick{Languages: map[string]pkgplumbing.LineStats{}}
		devstick[author] = dd
	}

	dd.Commits++

	isMerge := ctx.IsMerge
	if isMerge {
		return nil
	}

	langs := d.Languages.Languages()
	lineStats := d.LineStats.LineStats

	for changeEntry, stats := range lineStats {
		dd.Added += stats.Added
		dd.Removed += stats.Removed
		dd.Changed += stats.Changed
		lang := langs[changeEntry.Hash]
		langStats := dd.Languages[lang]
		dd.Languages[lang] = pkgplumbing.LineStats{
			Added:   langStats.Added + stats.Added,
			Removed: langStats.Removed + stats.Removed,
			Changed: langStats.Changed + stats.Changed,
		}
	}

	return nil
}

// Finalize completes the analysis and returns the result.
func (d *HistoryAnalyzer) Finalize() (analyze.Report, error) {
	names := d.reversedPeopleDict

	// If reversedPeopleDict wasn't set via facts, get it from the Identity detector
	if len(names) == 0 && d.Identity != nil {
		names = d.Identity.ReversedPeopleDict
	}

	if d.Anonymize {
		names = anonymizeNames(names)
	}

	return analyze.Report{
		"Ticks":              d.ticks,
		"ReversedPeopleDict": names,
		"TickSize":           d.tickSize,
	}, nil
}

// Fork creates a copy of the analyzer for parallel processing.
func (d *HistoryAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		clone := *d
		res[i] = &clone
	}

	return res
}

// Merge combines results from forked analyzer branches.
func (d *HistoryAnalyzer) Merge(branches []analyze.HistoryAnalyzer) {
	for _, branch := range branches {
		other, ok := branch.(*HistoryAnalyzer)
		if !ok {
			continue
		}

		d.mergeBranch(other)
	}
}

// mergeBranch merges a single branch's ticks into this analyzer.
func (d *HistoryAnalyzer) mergeBranch(other *HistoryAnalyzer) {
	for tick, otherDevTick := range other.ticks {
		d.ensureTickExists(tick)

		for devID, otherStats := range otherDevTick {
			d.mergeDevStats(tick, devID, otherStats)
		}
	}
}

// ensureTickExists ensures the tick map exists.
func (d *HistoryAnalyzer) ensureTickExists(tick int) {
	if d.ticks[tick] == nil {
		d.ticks[tick] = make(map[int]*DevTick)
	}
}

// mergeDevStats merges stats for a single developer in a tick.
func (d *HistoryAnalyzer) mergeDevStats(tick, devID int, otherStats *DevTick) {
	if d.ticks[tick][devID] == nil {
		d.ticks[tick][devID] = &DevTick{Languages: make(map[string]pkgplumbing.LineStats)}
	}

	currentStats := d.ticks[tick][devID]
	currentStats.Commits += otherStats.Commits
	currentStats.Added += otherStats.Added
	currentStats.Removed += otherStats.Removed
	currentStats.Changed += otherStats.Changed

	mergeDevLanguageStats(currentStats.Languages, otherStats.Languages)
}

// mergeDevLanguageStats merges language-specific stats.
func mergeDevLanguageStats(target, source map[string]pkgplumbing.LineStats) {
	for lang, langStats := range source {
		currentLangStats := target[lang]
		target[lang] = pkgplumbing.LineStats{
			Added:   currentLangStats.Added + langStats.Added,
			Removed: currentLangStats.Removed + langStats.Removed,
			Changed: currentLangStats.Changed + langStats.Changed,
		}
	}
}

// Serialize writes the analysis result to the given writer.
func (d *HistoryAnalyzer) Serialize(result analyze.Report, format string, writer io.Writer) error {
	switch format {
	case analyze.FormatJSON:
		return d.serializeJSON(result, writer)
	case analyze.FormatYAML:
		return d.serializeYAML(result, writer)
	case analyze.FormatPlot:
		return d.generatePlot(result, writer)
	default:
		return d.serializeLegacy(result, writer)
	}
}

func (d *HistoryAnalyzer) serializeJSON(result analyze.Report, writer io.Writer) error {
	metrics, err := ComputeAllMetrics(result)
	if err != nil {
		// For empty or invalid reports, serialize empty metrics structure
		metrics = &ComputedMetrics{}
	}

	err = json.NewEncoder(writer).Encode(metrics)
	if err != nil {
		return fmt.Errorf("json encode: %w", err)
	}

	return nil
}

func (d *HistoryAnalyzer) serializeYAML(result analyze.Report, writer io.Writer) error {
	metrics, err := ComputeAllMetrics(result)
	if err != nil {
		// For empty or invalid reports, serialize empty metrics structure
		metrics = &ComputedMetrics{}
	}

	data, err := yaml.Marshal(metrics)
	if err != nil {
		return fmt.Errorf("yaml marshal: %w", err)
	}

	_, err = writer.Write(data)
	if err != nil {
		return fmt.Errorf("yaml write: %w", err)
	}

	return nil
}

func (d *HistoryAnalyzer) serializeLegacy(result analyze.Report, writer io.Writer) error {
	ticks, ok := result["Ticks"].(map[int]map[int]*DevTick)
	if !ok {
		return errors.New("expected map[int]map[int]*DevTick for ticks") //nolint:err113 // descriptive error for type assertion failure.
	}

	reversedPeopleDict, ok := result["ReversedPeopleDict"].([]string)
	if !ok {
		return errors.New("expected []string for reversedPeopleDict") //nolint:err113 // descriptive error for type assertion failure.
	}

	tickSize, ok := result["TickSize"].(time.Duration)
	if !ok {
		return errors.New("expected time.Duration for tickSize") //nolint:err113 // descriptive error for type assertion failure.
	}

	fmt.Fprintln(writer, "  ticks:")
	serializeDevTicks(writer, ticks)

	fmt.Fprintln(writer, "  people:")

	for _, person := range reversedPeopleDict {
		fmt.Fprintf(writer, "  - %s\n", person)
	}

	fmt.Fprintln(writer, "  tick_size:", int(tickSize.Seconds()))

	return nil
}

// serializeDevTicks writes sorted tick data to the writer.
func serializeDevTicks(writer io.Writer, ticks map[int]map[int]*DevTick) {
	tickKeys := make([]int, 0, len(ticks))
	for tick := range ticks {
		tickKeys = append(tickKeys, tick)
	}

	sort.Ints(tickKeys)

	for _, tick := range tickKeys {
		fmt.Fprintf(writer, "    %d:\n", tick)
		serializeDevTickEntries(writer, ticks[tick])
	}
}

// serializeDevTickEntries writes sorted developer entries for a single tick.
func serializeDevTickEntries(writer io.Writer, rtick map[int]*DevTick) {
	devseq := make([]int, 0, len(rtick))
	for dev := range rtick {
		devseq = append(devseq, dev)
	}

	sort.Ints(devseq)

	for _, dev := range devseq {
		stats := rtick[dev]

		devID := dev
		if dev == identity.AuthorMissing {
			devID = -1
		}

		langs := make([]string, 0, len(stats.Languages))

		for lang, ls := range stats.Languages {
			if lang == "" {
				lang = "none"
			}

			langs = append(langs,
				fmt.Sprintf("%s: [%d, %d, %d]", lang, ls.Added, ls.Removed, ls.Changed))
		}

		sort.Strings(langs)
		fmt.Fprintf(writer, "      %d: [%d, %d, %d, %d, {%s}]\n",
			devID, stats.Commits, stats.Added, stats.Removed, stats.Changed,
			strings.Join(langs, ", "))
	}
}

// FormatReport writes the formatted analysis report to the given writer.
func (d *HistoryAnalyzer) FormatReport(report analyze.Report, writer io.Writer) error {
	return d.Serialize(report, analyze.FormatYAML, writer)
}
