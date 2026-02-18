// Package quality tracks code quality metrics (complexity, Halstead, comments,
// cohesion) across commit history by running static analyzers on per-commit
// UAST-parsed changed files.
package quality

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"maps"
	"sort"

	"gopkg.in/yaml.v3"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/cohesion"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/comments"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/reportutil"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/complexity"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/halstead"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// HistoryAnalyzer tracks code quality metrics across commit history by running
// static analyzers on UAST-parsed changed files per commit.
type HistoryAnalyzer struct {
	UAST  *plumbing.UASTChangesAnalyzer
	Ticks *plumbing.TicksSinceStart

	commitQuality map[string]*TickQuality // per-commit quality keyed by hash hex.
	commitsByTick map[int][]gitlib.Hash

	// Static analyzers (stateless, created in Initialize).
	complexityAnalyzer *complexity.Analyzer
	halsteadAnalyzer   *halstead.Analyzer
	commentsAnalyzer   *comments.Analyzer
	cohesionAnalyzer   *cohesion.Analyzer
}

// Name returns the analyzer name.
func (h *HistoryAnalyzer) Name() string { return "CodeQuality" }

// Flag returns the CLI flag for the analyzer.
func (h *HistoryAnalyzer) Flag() string { return "quality" }

// Descriptor returns stable analyzer metadata.
func (h *HistoryAnalyzer) Descriptor() analyze.Descriptor {
	return analyze.Descriptor{
		ID:          "history/quality",
		Description: "Tracks complexity, Halstead, comment quality, and cohesion metrics over commit history.",
		Mode:        analyze.ModeHistory,
	}
}

// NeedsUAST returns true because quality analysis requires UAST parsing.
func (h *HistoryAnalyzer) NeedsUAST() bool { return true }

// ListConfigurationOptions returns an empty list (no configurable options yet).
func (h *HistoryAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return nil
}

// Configure applies configuration from the provided facts map.
func (h *HistoryAnalyzer) Configure(facts map[string]any) error {
	if val, ok := facts[pkgplumbing.FactCommitsByTick].(map[int][]gitlib.Hash); ok {
		h.commitsByTick = val
	}

	return nil
}

// Initialize prepares the analyzer for processing commits.
func (h *HistoryAnalyzer) Initialize(_ *gitlib.Repository) error {
	h.commitQuality = make(map[string]*TickQuality)
	h.complexityAnalyzer = complexity.NewAnalyzer()
	h.halsteadAnalyzer = halstead.NewAnalyzer()
	h.commentsAnalyzer = comments.NewAnalyzer()
	h.cohesionAnalyzer = cohesion.NewAnalyzer()

	return nil
}

// Consume processes a single commit, running static analyzers on each changed file's UAST.
func (h *HistoryAnalyzer) Consume(ctx context.Context, ac *analyze.Context) error {
	changes := h.UAST.Changes(ctx)

	// Per-commit quality data keyed by commit hash.
	var cq *TickQuality

	if ac != nil && ac.Commit != nil {
		hashStr := ac.Commit.Hash().String()
		cq = &TickQuality{}
		h.commitQuality[hashStr] = cq
	}

	for _, change := range changes {
		if change.After == nil {
			continue
		}

		if cq != nil {
			h.analyzeNode(change.After, cq)
		}
	}

	return nil
}

func (h *HistoryAnalyzer) analyzeNode(root *node.Node, tq *TickQuality) {
	h.analyzeComplexity(root, tq)
	h.analyzeHalstead(root, tq)
	h.analyzeComments(root, tq)
	h.analyzeCohesion(root, tq)
}

func (h *HistoryAnalyzer) analyzeComplexity(root *node.Node, tq *TickQuality) {
	report, err := h.complexityAnalyzer.Analyze(root)
	if err != nil {
		return
	}

	tq.Complexities = append(tq.Complexities, float64(extractInt(report, "total_complexity")))
	tq.Cognitives = append(tq.Cognitives, float64(extractInt(report, "cognitive_complexity")))
	tq.MaxComplexities = append(tq.MaxComplexities, extractInt(report, "max_complexity"))
	tq.Functions = append(tq.Functions, extractInt(report, "total_functions"))
}

func (h *HistoryAnalyzer) analyzeHalstead(root *node.Node, tq *TickQuality) {
	report, err := h.halsteadAnalyzer.Analyze(root)
	if err != nil {
		return
	}

	tq.HalsteadVolumes = append(tq.HalsteadVolumes, extractFloat(report, "volume"))
	tq.HalsteadEfforts = append(tq.HalsteadEfforts, extractFloat(report, "effort"))
	tq.DeliveredBugs = append(tq.DeliveredBugs, extractFloat(report, "delivered_bugs"))
}

func (h *HistoryAnalyzer) analyzeComments(root *node.Node, tq *TickQuality) {
	report, err := h.commentsAnalyzer.Analyze(root)
	if err != nil {
		return
	}

	tq.CommentScores = append(tq.CommentScores, extractFloat(report, "overall_score"))
	tq.DocCoverages = append(tq.DocCoverages, extractFloat(report, "documentation_coverage"))
}

func (h *HistoryAnalyzer) analyzeCohesion(root *node.Node, tq *TickQuality) {
	report, err := h.cohesionAnalyzer.Analyze(root)
	if err != nil {
		return
	}

	tq.CohesionScores = append(tq.CohesionScores, extractFloat(report, "cohesion_score"))
}

// Finalize returns the analysis report with per-commit quality data.
func (h *HistoryAnalyzer) Finalize() (analyze.Report, error) {
	return analyze.Report{
		"commit_quality":  h.commitQuality,
		"commits_by_tick": h.commitsByTick,
	}, nil
}

// Fork creates independent copies of the analyzer for parallel processing.
func (h *HistoryAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)

	for i := range n {
		clone := &HistoryAnalyzer{
			UAST:               &plumbing.UASTChangesAnalyzer{},
			Ticks:              &plumbing.TicksSinceStart{},
			commitQuality:      make(map[string]*TickQuality),
			commitsByTick:      h.commitsByTick, // shared read-only.
			complexityAnalyzer: complexity.NewAnalyzer(),
			halsteadAnalyzer:   halstead.NewAnalyzer(),
			commentsAnalyzer:   comments.NewAnalyzer(),
			cohesionAnalyzer:   cohesion.NewAnalyzer(),
		}
		res[i] = clone
	}

	return res
}

// Merge combines results from forked analyzer branches.
func (h *HistoryAnalyzer) Merge(branches []analyze.HistoryAnalyzer) {
	for _, branch := range branches {
		other, ok := branch.(*HistoryAnalyzer)
		if !ok {
			continue
		}

		// Per-commit data: each fork processes distinct commits, so no collisions.
		maps.Copy(h.commitQuality, other.commitQuality)
	}
}

// SequentialOnly returns false because quality analysis can be parallelized.
func (h *HistoryAnalyzer) SequentialOnly() bool { return false }

// CPUHeavy returns true because quality analysis performs UAST processing per commit.
func (h *HistoryAnalyzer) CPUHeavy() bool { return true }

// SnapshotPlumbing captures the current plumbing output state.
func (h *HistoryAnalyzer) SnapshotPlumbing() analyze.PlumbingSnapshot {
	return plumbing.Snapshot{
		UASTChanges: h.UAST.TransferChanges(),
		Tick:        h.Ticks.Tick,
	}
}

// ApplySnapshot restores plumbing state from a previously captured snapshot.
func (h *HistoryAnalyzer) ApplySnapshot(snap analyze.PlumbingSnapshot) {
	ss, ok := snap.(plumbing.Snapshot)
	if !ok {
		return
	}

	h.UAST.SetChanges(ss.UASTChanges)
	h.Ticks.Tick = ss.Tick
}

// ReleaseSnapshot releases UAST trees owned by the snapshot.
func (h *HistoryAnalyzer) ReleaseSnapshot(snap analyze.PlumbingSnapshot) {
	ss, ok := snap.(plumbing.Snapshot)
	if !ok {
		return
	}

	plumbing.ReleaseSnapshotUAST(ss)
}

// Serialize writes the analysis result in the specified format.
func (h *HistoryAnalyzer) Serialize(result analyze.Report, format string, writer io.Writer) error {
	switch format {
	case analyze.FormatJSON:
		return h.serializeJSON(result, writer)
	case analyze.FormatYAML:
		return h.serializeYAML(result, writer)
	case analyze.FormatPlot:
		return h.generatePlot(result, writer)
	case analyze.FormatBinary:
		return h.serializeBinary(result, writer)
	default:
		return fmt.Errorf("%w: %s", analyze.ErrUnsupportedFormat, format)
	}
}

func (h *HistoryAnalyzer) serializeJSON(result analyze.Report, writer io.Writer) error {
	computed, err := ComputeAllMetrics(result)
	if err != nil {
		computed = &ComputedMetrics{}
	}

	err = json.NewEncoder(writer).Encode(computed)
	if err != nil {
		return fmt.Errorf("json encode: %w", err)
	}

	return nil
}

func (h *HistoryAnalyzer) serializeYAML(result analyze.Report, writer io.Writer) error {
	computed, err := ComputeAllMetrics(result)
	if err != nil {
		computed = &ComputedMetrics{}
	}

	data, err := yaml.Marshal(computed)
	if err != nil {
		return fmt.Errorf("yaml marshal: %w", err)
	}

	_, err = writer.Write(data)
	if err != nil {
		return fmt.Errorf("yaml write: %w", err)
	}

	return nil
}

func (h *HistoryAnalyzer) serializeBinary(result analyze.Report, writer io.Writer) error {
	computed, err := ComputeAllMetrics(result)
	if err != nil {
		computed = &ComputedMetrics{}
	}

	err = reportutil.EncodeBinaryEnvelope(computed, writer)
	if err != nil {
		return fmt.Errorf("binary encode: %w", err)
	}

	return nil
}

// FormatReport writes the formatted analysis report in YAML.
func (h *HistoryAnalyzer) FormatReport(report analyze.Report, writer io.Writer) error {
	return h.Serialize(report, analyze.FormatYAML, writer)
}

// --- Helpers ---.

func extractInt(report analyze.Report, key string) int {
	switch v := report[key].(type) {
	case int:
		return v
	case float64:
		return int(v)
	default:
		return 0
	}
}

func extractFloat(report analyze.Report, key string) float64 {
	switch v := report[key].(type) {
	case float64:
		return v
	case int:
		return float64(v)
	default:
		return 0
	}
}

func sortedTickKeys(tq map[int]*TickQuality) []int {
	ticks := make([]int, 0, len(tq))

	for tick := range tq {
		ticks = append(ticks, tick)
	}

	sort.Ints(ticks)

	return ticks
}
