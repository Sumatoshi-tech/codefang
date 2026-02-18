// Package devs provides devs functionality.
package devs

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"maps"
	"time"

	"gopkg.in/yaml.v3"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/reportutil"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/identity"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

// HistoryAnalyzer calculates per-developer line statistics across commit history.
type HistoryAnalyzer struct {
	Identity             *plumbing.IdentityDetector
	TreeDiff             *plumbing.TreeDiffAnalyzer
	Ticks                *plumbing.TicksSinceStart
	Languages            *plumbing.LanguagesDetectionAnalyzer
	LineStats            *plumbing.LinesStatsCalculator
	commitDevData        map[string]*CommitDevData // per-commit stats keyed by hash hex.
	commitsByTick        map[int][]gitlib.Hash
	merges               map[gitlib.Hash]bool
	reversedPeopleDict   []string
	tickSize             time.Duration
	ConsiderEmptyCommits bool
	Anonymize            bool
}

// CommitDevData holds aggregate dev stats for a single commit.
type CommitDevData struct {
	Commits   int                              `json:"commits"`
	Added     int                              `json:"lines_added"`
	Removed   int                              `json:"lines_removed"`
	Changed   int                              `json:"lines_changed"`
	AuthorID  int                              `json:"author_id"`
	Languages map[string]pkgplumbing.LineStats `json:"languages,omitempty"`
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
	return d.Descriptor().Description
}

// Descriptor returns stable analyzer metadata.
func (d *HistoryAnalyzer) Descriptor() analyze.Descriptor {
	return analyze.Descriptor{
		ID:          "history/devs",
		Description: "Calculates the number of commits, added, removed and changed lines per developer through time.",
		Mode:        analyze.ModeHistory,
	}
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

	if val, exists := facts[pkgplumbing.FactCommitsByTick].(map[int][]gitlib.Hash); exists {
		d.commitsByTick = val
	}

	return nil
}

// Initialize prepares the analyzer for processing commits.
func (d *HistoryAnalyzer) Initialize(_ *gitlib.Repository) error {
	RegisterDevPlotSections()

	if d.tickSize == 0 {
		d.tickSize = defaultHoursPerDay * time.Hour // Default fallback.
	}

	d.commitDevData = map[string]*CommitDevData{}
	d.merges = map[gitlib.Hash]bool{}

	return nil
}

// Consume processes a single commit with the provided dependency results.
func (d *HistoryAnalyzer) Consume(_ context.Context, ac *analyze.Context) error {
	// OneShotMergeProcessor logic.
	commit := ac.Commit
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

	cdd := &CommitDevData{
		Commits:   1,
		AuthorID:  author,
		Languages: make(map[string]pkgplumbing.LineStats),
	}
	d.commitDevData[commit.Hash().String()] = cdd

	isMerge := ac.IsMerge

	if isMerge {
		return nil
	}

	langs := d.Languages.Languages()
	lineStats := d.LineStats.LineStats

	for changeEntry, stats := range lineStats {
		cdd.Added += stats.Added
		cdd.Removed += stats.Removed
		cdd.Changed += stats.Changed

		lang := langs[changeEntry.Hash]
		cddLangStats := cdd.Languages[lang]
		cdd.Languages[lang] = pkgplumbing.LineStats{
			Added:   cddLangStats.Added + stats.Added,
			Removed: cddLangStats.Removed + stats.Removed,
			Changed: cddLangStats.Changed + stats.Changed,
		}
	}

	return nil
}

// Finalize completes the analysis and returns the result.
func (d *HistoryAnalyzer) Finalize() (analyze.Report, error) {
	names := d.reversedPeopleDict

	// If reversedPeopleDict wasn't set via facts, get it from the Identity detector.
	if len(names) == 0 && d.Identity != nil {
		names = d.Identity.ReversedPeopleDict
	}

	if d.Anonymize {
		names = anonymizeNames(names)
	}

	return analyze.Report{
		"CommitDevData":      d.commitDevData,
		"CommitsByTick":      d.commitsByTick,
		"ReversedPeopleDict": names,
		"TickSize":           d.tickSize,
	}, nil
}

// Fork creates a copy of the analyzer for parallel processing.
func (d *HistoryAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		clone := *d

		// Independent plumbing state (not shared with parent).
		clone.Identity = &plumbing.IdentityDetector{}
		clone.TreeDiff = &plumbing.TreeDiffAnalyzer{}
		clone.Ticks = &plumbing.TicksSinceStart{}
		clone.Languages = &plumbing.LanguagesDetectionAnalyzer{}
		clone.LineStats = &plumbing.LinesStatsCalculator{}

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

		maps.Copy(d.commitDevData, other.commitDevData)
	}
}

// SequentialOnly returns true because devs' Fork() does not isolate mutable map state.
func (d *HistoryAnalyzer) SequentialOnly() bool { return true }

// CPUHeavy returns false because developer stats aggregation is lightweight bookkeeping.
func (d *HistoryAnalyzer) CPUHeavy() bool { return false }

// SnapshotPlumbing captures the current plumbing state.
func (d *HistoryAnalyzer) SnapshotPlumbing() analyze.PlumbingSnapshot {
	return plumbing.Snapshot{
		Changes:   d.TreeDiff.Changes,
		Tick:      d.Ticks.Tick,
		AuthorID:  d.Identity.AuthorID,
		Languages: d.Languages.Languages(),
		LineStats: d.LineStats.LineStats,
	}
}

// ApplySnapshot restores plumbing state from a snapshot.
func (d *HistoryAnalyzer) ApplySnapshot(snap analyze.PlumbingSnapshot) {
	snapshot, ok := snap.(plumbing.Snapshot)
	if !ok {
		return
	}

	d.TreeDiff.Changes = snapshot.Changes
	d.Ticks.Tick = snapshot.Tick
	d.Identity.AuthorID = snapshot.AuthorID
	d.Languages.SetLanguages(snapshot.Languages)
	d.LineStats.LineStats = snapshot.LineStats
}

// ReleaseSnapshot is a no-op for devs (no UAST resources).
func (d *HistoryAnalyzer) ReleaseSnapshot(_ analyze.PlumbingSnapshot) {}

// Serialize writes the analysis result to the given writer.
func (d *HistoryAnalyzer) Serialize(result analyze.Report, format string, writer io.Writer) error {
	switch format {
	case analyze.FormatJSON:
		return d.serializeJSON(result, writer)
	case analyze.FormatYAML:
		return d.serializeYAML(result, writer)
	case analyze.FormatPlot:
		return d.generatePlot(result, writer)
	case analyze.FormatBinary:
		return d.serializeBinary(result, writer)
	default:
		return fmt.Errorf("%w: %s", analyze.ErrUnsupportedFormat, format)
	}
}

func (d *HistoryAnalyzer) serializeJSON(result analyze.Report, writer io.Writer) error {
	metrics, err := ComputeAllMetrics(result)
	if err != nil {
		// For empty or invalid reports, serialize empty metrics structure.
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
		// For empty or invalid reports, serialize empty metrics structure.
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

func (d *HistoryAnalyzer) serializeBinary(result analyze.Report, writer io.Writer) error {
	metrics, err := ComputeAllMetrics(result)
	if err != nil {
		metrics = &ComputedMetrics{}
	}

	err = reportutil.EncodeBinaryEnvelope(metrics, writer)
	if err != nil {
		return fmt.Errorf("binary encode: %w", err)
	}

	return nil
}

// FormatReport writes the formatted analysis report to the given writer.
func (d *HistoryAnalyzer) FormatReport(report analyze.Report, writer io.Writer) error {
	return d.Serialize(report, analyze.FormatYAML, writer)
}
