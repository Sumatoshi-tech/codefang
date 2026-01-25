package devs

import (
	"fmt"
	"io"
	"sort"
	"strings"
	"time"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/identity"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
	"github.com/go-git/go-git/v6"
	gitplumbing "github.com/go-git/go-git/v6/plumbing"
)

type DevsHistoryAnalyzer struct {
	// Configuration
	ConsiderEmptyCommits bool

	// Dependencies
	Identity  *plumbing.IdentityDetector
	TreeDiff  *plumbing.TreeDiffAnalyzer
	Ticks     *plumbing.TicksSinceStart
	Languages *plumbing.LanguagesDetectionAnalyzer
	LineStats *plumbing.LinesStatsCalculator

	// State
	ticks              map[int]map[int]*DevTick
	reversedPeopleDict []string
	tickSize           time.Duration
	merges             map[gitplumbing.Hash]bool // OneShotMergeProcessor

	// Internal
	l interface {
		Warnf(format string, args ...interface{})
	}
}

// DevTick is the statistics for a development tick and a particular developer.
type DevTick struct {
	// Commits is the number of commits made by a particular developer in a particular tick.
	Commits int
	pkgplumbing.LineStats
	// Languages carries fine-grained line stats per programming language.
	Languages map[string]pkgplumbing.LineStats
}

const (
	ConfigDevsConsiderEmptyCommits = "Devs.ConsiderEmptyCommits"
)

func (d *DevsHistoryAnalyzer) Name() string {
	return "Devs"
}

func (d *DevsHistoryAnalyzer) Flag() string {
	return "devs"
}

func (d *DevsHistoryAnalyzer) Description() string {
	return "Calculates the number of commits, added, removed and changed lines per developer through time."
}

func (d *DevsHistoryAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{{
		Name:        ConfigDevsConsiderEmptyCommits,
		Description: "Take into account empty commits such as trivial merges.",
		Flag:        "empty-commits",
		Type:        pipeline.BoolConfigurationOption,
		Default:     false,
	}}
}

func (d *DevsHistoryAnalyzer) Configure(facts map[string]interface{}) error {
	if val, exists := facts[ConfigDevsConsiderEmptyCommits].(bool); exists {
		d.ConsiderEmptyCommits = val
	}
	if val, exists := facts[identity.FactIdentityDetectorReversedPeopleDict].([]string); exists {
		d.reversedPeopleDict = val
	}
	if val, exists := facts[pkgplumbing.FactTickSize].(time.Duration); exists {
		d.tickSize = val
	}
	return nil
}

func (d *DevsHistoryAnalyzer) Initialize(repository *git.Repository) error {
	if d.tickSize == 0 {
		d.tickSize = 24 * time.Hour // Default fallback
	}
	d.ticks = map[int]map[int]*DevTick{}
	d.merges = map[gitplumbing.Hash]bool{}
	return nil
}

func (d *DevsHistoryAnalyzer) Consume(ctx *analyze.Context) error {
	// OneShotMergeProcessor logic
	commit := ctx.Commit
	shouldConsume := true
	if commit.NumParents() > 1 {
		if d.merges[commit.Hash] {
			shouldConsume = false
		} else {
			d.merges[commit.Hash] = true
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

	langs := d.Languages.Languages
	lineStats := d.LineStats.LineStats

	for changeEntry, stats := range lineStats {
		dd.Added += stats.Added
		dd.Removed += stats.Removed
		dd.Changed += stats.Changed
		lang := langs[changeEntry.TreeEntry.Hash]
		langStats := dd.Languages[lang]
		dd.Languages[lang] = pkgplumbing.LineStats{
			Added:   langStats.Added + stats.Added,
			Removed: langStats.Removed + stats.Removed,
			Changed: langStats.Changed + stats.Changed,
		}
	}
	return nil
}

func (d *DevsHistoryAnalyzer) Finalize() (analyze.Report, error) {
	return analyze.Report{
		"Ticks":              d.ticks,
		"ReversedPeopleDict": d.reversedPeopleDict,
		"TickSize":           d.tickSize,
	}, nil
}

func (d *DevsHistoryAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := 0; i < n; i++ {
		clone := *d
		res[i] = &clone
	}
	return res
}

func (d *DevsHistoryAnalyzer) Merge(branches []analyze.HistoryAnalyzer) {
}

func (d *DevsHistoryAnalyzer) Serialize(result analyze.Report, binary bool, writer io.Writer) error {
	ticks := result["Ticks"].(map[int]map[int]*DevTick)
	reversedPeopleDict := result["ReversedPeopleDict"].([]string)
	tickSize := result["TickSize"].(time.Duration)

	fmt.Fprintln(writer, "  ticks:")
	tickKeys := make([]int, 0, len(ticks))
	for tick := range ticks {
		tickKeys = append(tickKeys, tick)
	}
	sort.Ints(tickKeys)
	for _, tick := range tickKeys {
		fmt.Fprintf(writer, "    %d:\n", tick)
		rtick := ticks[tick]
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
			var langs []string
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
	fmt.Fprintln(writer, "  people:")
	for _, person := range reversedPeopleDict {
		fmt.Fprintf(writer, "  - %s\n", person)
	}
	fmt.Fprintln(writer, "  tick_size:", int(tickSize.Seconds()))
	return nil
}

func (d *DevsHistoryAnalyzer) FormatReport(report analyze.Report, writer io.Writer) error {
	return d.Serialize(report, false, writer)
}
