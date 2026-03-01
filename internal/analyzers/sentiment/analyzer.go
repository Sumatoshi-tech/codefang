// Package sentiment provides sentiment functionality.
package sentiment

import (
	"context"
	"fmt"
	"io"
	"maps"
	"regexp"
	"sort"
	"strings"
	"time"
	"unicode/utf8"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/plotpage"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/plumbing"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/safeconv"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// CommitResult is the per-commit TC payload for the sentiment analyzer.
// It holds the filtered comment texts extracted from UAST changes for a single commit.
type CommitResult struct {
	// Comments contains filtered comment texts from this commit's UAST changes.
	Comments []string
}

// TickData is the per-tick aggregated payload for the sentiment analyzer.
// It holds per-commit comments for the canonical report format.
type TickData struct {
	// CommentsByCommit maps commit hash (hex) to comment texts.
	CommentsByCommit map[string][]string
}

// MinCommentLengthThresholdHigh is the minimum character length for a comment to be included in sentiment analysis.
const (
	MinCommentLengthThresholdHigh = 10
)

const (
	// ConfigCommentSentimentMinLength is the configuration key for the minimum comment length.
	ConfigCommentSentimentMinLength = "CommentSentiment.MinLength"
	// ConfigCommentSentimentGap is the configuration key for the sentiment gap threshold.
	ConfigCommentSentimentGap = "CommentSentiment.Gap"

	// DefaultCommentSentimentCommentMinLength is the default minimum comment length for sentiment analysis.
	DefaultCommentSentimentCommentMinLength = 20
	// DefaultCommentSentimentGap is the default gap threshold for sentiment analysis.
	DefaultCommentSentimentGap = float32(0.5)

	// CommentLettersRatio defines the minimum ratio of letters in a comment.
	CommentLettersRatio = 0.6
)

var (
	filteredFirstCharRE = regexp.MustCompile(`[^\p{L}\p{N}]`)
	filteredCharsRE     = regexp.MustCompile(`[^\p{L}\p{N}\-_:;,./?!#&%+*=\n \t()]+`)
	charsRE             = regexp.MustCompile(`\p{L}+`)
	functionNameRE      = regexp.MustCompile(`\s*[a-zA-Z_][a-zA-Z_0-9]*\(\)`)
	whitespaceRE        = regexp.MustCompile(`\s+`)
	licenseRE           = regexp.MustCompile(`(?i)(licen[cs]e|copyright|©)`)
)

// Analyzer tracks comment sentiment across commit history.
type Analyzer struct {
	*analyze.BaseHistoryAnalyzer[*ComputedMetrics]

	UAST             *plumbing.UASTChangesAnalyzer
	Ticks            *plumbing.TicksSinceStart
	commitsByTick    map[int][]gitlib.Hash
	MinCommentLength int
	Gap              float32
}

// NewAnalyzer creates a new sentiment analyzer.
func NewAnalyzer() *Analyzer {
	a := &Analyzer{}
	a.BaseHistoryAnalyzer = &analyze.BaseHistoryAnalyzer[*ComputedMetrics]{
		Desc: analyze.Descriptor{
			ID:          "history/sentiment",
			Description: "Classifies each new or changed comment per commit as containing positive or negative emotions.",
			Mode:        analyze.ModeHistory,
		},
		Sequential: false,
		ConfigOptions: []pipeline.ConfigurationOption{
			{
				Name:        ConfigCommentSentimentMinLength,
				Description: "Minimum length of the comment to be analyzed.",
				Flag:        "min-comment-len",
				Type:        pipeline.IntConfigurationOption,
				Default:     DefaultCommentSentimentCommentMinLength,
			},
			{
				Name:        ConfigCommentSentimentGap,
				Description: "Sentiment value threshold.",
				Flag:        "sentiment-gap",
				Type:        pipeline.FloatConfigurationOption,
				Default:     DefaultCommentSentimentGap,
			},
		},
		ComputeMetricsFn: computeMetricsSafe,
		AggregatorFn:     newAggregator,
	}

	a.TicksToReportFn = func(ctx context.Context, ticks []analyze.TICK) analyze.Report {
		return ticksToReport(ctx, ticks, a.commitsByTick)
	}

	return a
}

// CPUHeavy indicates this analyzer does heavy computation.
func (s *Analyzer) CPUHeavy() bool {
	return true
}

// Serialize writes the analysis result to the given writer.
// Overrides base to add text and plot format support.
func (s *Analyzer) Serialize(result analyze.Report, format string, writer io.Writer) error {
	if format == analyze.FormatText {
		return s.generateText(result, writer)
	}

	if format == analyze.FormatPlot {
		return s.generatePlot(result, writer)
	}

	return s.BaseHistoryAnalyzer.Serialize(result, format, writer)
}

// SerializeTICKs converts aggregated TICKs into the final report and serializes it.
// Overrides base to add text and plot format support.
func (s *Analyzer) SerializeTICKs(ticks []analyze.TICK, format string, writer io.Writer) error {
	if format == analyze.FormatText || format == analyze.FormatPlot {
		report, err := s.ReportFromTICKs(context.Background(), ticks)
		if err != nil {
			return err
		}

		if format == analyze.FormatText {
			return s.generateText(report, writer)
		}

		return s.generatePlot(report, writer)
	}

	return s.BaseHistoryAnalyzer.SerializeTICKs(ticks, format, writer)
}

func (s *Analyzer) generateText(report analyze.Report, writer io.Writer) error {
	metrics, err := ComputeAllMetrics(report)
	if err != nil {
		return fmt.Errorf("compute metrics: %w", err)
	}

	_, err = io.WriteString(writer, RenderTerminal(metrics))
	if err != nil {
		return fmt.Errorf("write text report: %w", err)
	}

	return nil
}

func (s *Analyzer) generatePlot(report analyze.Report, writer io.Writer) error {
	sections, err := s.GenerateSections(report)
	if err != nil {
		return err
	}

	page := plotpage.NewPage(chartSectionTitle, chartSectionSubtitle)
	page.Add(sections...)

	return page.Render(writer)
}

func computeMetricsSafe(report analyze.Report) (*ComputedMetrics, error) {
	if len(report) == 0 {
		return &ComputedMetrics{}, nil
	}

	return ComputeAllMetrics(report)
}

// Configure sets up the analyzer with the provided facts.
func (s *Analyzer) Configure(facts map[string]any) error {
	if val, exists := facts[ConfigCommentSentimentGap].(float32); exists {
		s.Gap = val
	}

	if val, exists := facts[ConfigCommentSentimentMinLength].(int); exists {
		s.MinCommentLength = val
	}

	if val, exists := facts[pkgplumbing.FactCommitsByTick].(map[int][]gitlib.Hash); exists {
		s.commitsByTick = val
	}

	s.validate()

	return nil
}

func (s *Analyzer) validate() {
	if s.Gap < 0 || s.Gap >= 1 {
		s.Gap = DefaultCommentSentimentGap
	}

	if s.MinCommentLength < MinCommentLengthThresholdHigh {
		s.MinCommentLength = DefaultCommentSentimentCommentMinLength
	}
}

// Initialize prepares the analyzer for processing commits.
func (s *Analyzer) Initialize(_ *gitlib.Repository) error {
	s.validate()

	return nil
}

// Consume processes a single commit and returns a TC with extracted comments.
// The analyzer does not retain any per-commit state; all output is in the TC.
func (s *Analyzer) Consume(ctx context.Context, ac *analyze.Context) (analyze.TC, error) {
	changes := s.UAST.Changes(ctx)

	var commentNodes []*node.Node

	for change := range changes {
		if change.After != nil {
			extractComments(change.After, &commentNodes)
		}
	}

	comments := s.mergeComments(commentNodes)

	tc := analyze.TC{
		Data: &CommitResult{Comments: comments},
	}

	if ac != nil && ac.Commit != nil {
		tc.CommitHash = ac.Commit.Hash()
	}

	return tc, nil
}

func extractComments(root *node.Node, result *[]*node.Node) {
	if root.Type == node.UASTComment {
		*result = append(*result, root)
	}

	for _, child := range root.Children {
		extractComments(child, result)
	}
}

func groupCommentsByLine(extracted []*node.Node) (grouped map[int][]*node.Node, sortedLines []int) {
	lines := map[int][]*node.Node{}

	for _, n := range extracted {
		if n.Pos == nil {
			continue
		}

		lineno := safeconv.MustUintToInt(n.Pos.StartLine)
		lines[lineno] = append(lines[lineno], n)
	}

	lineNums := make([]int, 0, len(lines))
	for line := range lines {
		lineNums = append(lineNums, line)
	}

	sort.Ints(lineNums)

	return lines, lineNums
}

func mergeAdjacentComments(lines map[int][]*node.Node, lineNums []int) []string {
	var mergedComments []string

	var buffer []string

	for i, line := range lineNums {
		lineNodes := lines[line]

		maxEnd := line
		for _, n := range lineNodes {
			if n.Pos != nil && maxEnd < safeconv.MustUintToInt(n.Pos.EndLine) {
				maxEnd = safeconv.MustUintToInt(n.Pos.EndLine)
			}

			token := strings.TrimSpace(n.Token)
			if token != "" {
				buffer = append(buffer, token)
			}
		}

		if i < len(lineNums)-1 && lineNums[i+1] <= maxEnd+1 {
			continue
		}

		mergedComments = append(mergedComments, strings.Join(buffer, "\n"))
		buffer = nil
	}

	return mergedComments
}

// commentPrefixes are comment delimiters stripped before analysis.
// Longer prefixes first so "///" is matched before "//".
var commentPrefixes = []string{"///", "//!", "//", "/**", "/*", "#!", "##", "#", "--", ";;", ";"}

// commentSuffixes are closing delimiters stripped from lines.
var commentSuffixes = []string{"*/"}

// stripCommentDelimiters removes common comment syntax from each line.
func stripCommentDelimiters(text string) string {
	lines := strings.Split(text, "\n")

	for i, line := range lines {
		trimmed := strings.TrimSpace(line)
		for _, prefix := range commentPrefixes {
			if strings.HasPrefix(trimmed, prefix) {
				trimmed = strings.TrimSpace(trimmed[len(prefix):])

				break
			}
		}

		for _, suffix := range commentSuffixes {
			if strings.HasSuffix(trimmed, suffix) {
				trimmed = strings.TrimSpace(trimmed[:len(trimmed)-len(suffix)])
			}
		}

		lines[i] = trimmed
	}

	return strings.TrimSpace(strings.Join(lines, " "))
}

func filterComments(comments []string, minLength int) []string {
	filteredComments := make([]string, 0, len(comments))

	for _, comment := range comments {
		comment = stripCommentDelimiters(comment)
		comment = strings.TrimSpace(comment)

		if comment == "" {
			continue
		}

		firstRune, _ := utf8.DecodeRuneInString(comment)
		if firstRune == utf8.RuneError || filteredFirstCharRE.MatchString(string(firstRune)) {
			continue
		}

		comment = functionNameRE.ReplaceAllString(comment, "")

		comment = filteredCharsRE.ReplaceAllString(comment, "")
		if len(comment) < minLength {
			continue
		}

		comment = whitespaceRE.ReplaceAllString(comment, " ")

		charsCount := 0
		for _, match := range charsRE.FindAllStringIndex(comment, -1) {
			charsCount += match[1] - match[0]
		}

		if charsCount < int(float32(len(comment))*CommentLettersRatio) {
			continue
		}

		if licenseRE.MatchString(comment) {
			continue
		}

		filteredComments = append(filteredComments, comment)
	}

	return filteredComments
}

func (s *Analyzer) mergeComments(extracted []*node.Node) []string {
	lines, lineNums := groupCommentsByLine(extracted)
	mergedComments := mergeAdjacentComments(lines, lineNums)

	return filterComments(mergedComments, s.MinCommentLength)
}

// Fork creates a copy of the analyzer for parallel processing.
func (s *Analyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		clone := &Analyzer{
			UAST:             &plumbing.UASTChangesAnalyzer{},
			Ticks:            &plumbing.TicksSinceStart{},
			MinCommentLength: s.MinCommentLength,
			Gap:              s.Gap,
			commitsByTick:    s.commitsByTick, // shared read-only.
		}
		res[i] = clone
	}

	return res
}

// Merge is a no-op. Per-commit results are emitted as TCs.
func (s *Analyzer) Merge(_ []analyze.HistoryAnalyzer) {}

// NeedsUAST returns true to enable the UAST pipeline.
func (s *Analyzer) NeedsUAST() bool { return true }

// SnapshotPlumbing captures the current plumbing output state for parallel execution.
func (s *Analyzer) SnapshotPlumbing() analyze.PlumbingSnapshot {
	return plumbing.Snapshot{
		UASTChanges: s.UAST.TransferChanges(),
		Tick:        s.Ticks.Tick,
	}
}

// ApplySnapshot restores plumbing state from a previously captured snapshot.
func (s *Analyzer) ApplySnapshot(snap analyze.PlumbingSnapshot) {
	ss, ok := snap.(plumbing.Snapshot)
	if !ok {
		return
	}

	s.UAST.SetChanges(ss.UASTChanges)
	s.Ticks.Tick = ss.Tick
}

// ReleaseSnapshot releases UAST trees owned by the snapshot.
func (s *Analyzer) ReleaseSnapshot(snap analyze.PlumbingSnapshot) {
	ss, ok := snap.(plumbing.Snapshot)
	if !ok {
		return
	}

	for _, ch := range ss.UASTChanges {
		node.ReleaseTree(ch.Before)
		node.ReleaseTree(ch.After)
	}
}

// NewAggregator creates an aggregator for this analyzer.
func (s *Analyzer) NewAggregator(opts analyze.AggregatorOptions) analyze.Aggregator {
	return s.AggregatorFn(opts)
}

// ReportFromTICKs converts aggregated TICKs into a Report.
func (s *Analyzer) ReportFromTICKs(ctx context.Context, ticks []analyze.TICK) (analyze.Report, error) {
	return s.TicksToReportFn(ctx, ticks), nil
}

// ExtractCommitTimeSeries extracts per-commit sentiment data from a finalized report.
func (s *Analyzer) ExtractCommitTimeSeries(report analyze.Report) map[string]any {
	commentsByCommit, ok := report["comments_by_commit"].(map[string][]string)
	if !ok || len(commentsByCommit) == 0 {
		return nil
	}

	result := make(map[string]any, len(commentsByCommit))

	for hash, comments := range commentsByCommit {
		entry := map[string]any{
			"comment_count": len(comments),
		}
		if len(comments) > 0 {
			entry["sentiment"] = ComputeSentiment(comments)
		}

		result[hash] = entry
	}

	return result
}

// Extract properties for GenericAggregator.

type tickAccumulator struct {
	commentsByCommit map[string][]string
	startTime        time.Time
	endTime          time.Time
}

const (
	tickAccumulatorOverhead = 64
	bytesPerCommentEstimate = 80
)

func extractTC(tc analyze.TC, byTick map[int]*tickAccumulator) error {
	if tc.CommitHash.IsZero() {
		return nil
	}

	acc, ok := byTick[tc.Tick]
	if !ok {
		acc = &tickAccumulator{
			commentsByCommit: make(map[string][]string),
			startTime:        tc.Timestamp,
			endTime:          tc.Timestamp,
		}
		byTick[tc.Tick] = acc
	}

	if !tc.Timestamp.IsZero() {
		if tc.Timestamp.Before(acc.startTime) || acc.startTime.IsZero() {
			acc.startTime = tc.Timestamp
		}

		if tc.Timestamp.After(acc.endTime) {
			acc.endTime = tc.Timestamp
		}
	}

	cr, isCommitResult := tc.Data.(*CommitResult)
	if isCommitResult {
		acc.commentsByCommit[tc.CommitHash.String()] = cr.Comments
	} else {
		acc.commentsByCommit[tc.CommitHash.String()] = []string{}
	}

	return nil
}

func mergeState(existing, incoming *tickAccumulator) *tickAccumulator {
	if existing == nil {
		return incoming
	}

	if incoming == nil {
		return existing
	}

	if incoming.commentsByCommit != nil {
		if existing.commentsByCommit == nil {
			existing.commentsByCommit = make(map[string][]string)
		}

		maps.Copy(existing.commentsByCommit, incoming.commentsByCommit)
	}

	if !incoming.startTime.IsZero() && (incoming.startTime.Before(existing.startTime) || existing.startTime.IsZero()) {
		existing.startTime = incoming.startTime
	}

	if !incoming.endTime.IsZero() && incoming.endTime.After(existing.endTime) {
		existing.endTime = incoming.endTime
	}

	return existing
}

func sizeState(state *tickAccumulator) int64 {
	if state == nil || state.commentsByCommit == nil {
		return 0
	}

	var size int64 = tickAccumulatorOverhead

	for _, comments := range state.commentsByCommit {
		for _, c := range comments {
			size += int64(len(c)) + bytesPerCommentEstimate
		}
	}

	return size
}

func buildTick(tick int, state *tickAccumulator) (analyze.TICK, error) {
	if state == nil || state.commentsByCommit == nil {
		return analyze.TICK{Tick: tick, Data: &TickData{CommentsByCommit: make(map[string][]string)}}, nil
	}

	return analyze.TICK{
		Tick:      tick,
		StartTime: state.startTime,
		EndTime:   state.endTime,
		Data:      &TickData{CommentsByCommit: state.commentsByCommit},
	}, nil
}

func newAggregator(opts analyze.AggregatorOptions) analyze.Aggregator {
	agg := analyze.NewGenericAggregator[*tickAccumulator, *TickData](
		opts,
		extractTC,
		mergeState,
		sizeState,
		buildTick,
	)
	agg.DrainCommitDataFn = drainSentimentCommitData

	return agg
}

func drainSentimentCommitData(state *tickAccumulator) (stats map[string]any, tickHashes map[int][]gitlib.Hash) {
	if state == nil || len(state.commentsByCommit) == 0 {
		return nil, nil
	}

	result := make(map[string]any, len(state.commentsByCommit))
	for hash, comments := range state.commentsByCommit {
		entry := map[string]any{
			"comment_count": len(comments),
		}
		if len(comments) > 0 {
			entry["sentiment"] = ComputeSentiment(comments)
		}

		result[hash] = entry
	}

	state.commentsByCommit = make(map[string][]string)

	return result, nil
}

func ticksToReport(_ context.Context, ticks []analyze.TICK, commitsByTick map[int][]gitlib.Hash) analyze.Report {
	commentsByCommit := buildCommentsByCommitFromTicks(ticks)
	ct := commitsByTick

	if ct == nil {
		ct = buildCommitsByTickFromTicks(ticks)
	}

	return analyze.Report{
		"comments_by_commit": commentsByCommit,
		"commits_by_tick":    ct,
	}
}

func buildCommentsByCommitFromTicks(ticks []analyze.TICK) map[string][]string {
	commentsByCommit := make(map[string][]string)

	for _, tick := range ticks {
		td, ok := tick.Data.(*TickData)
		if !ok || td == nil || td.CommentsByCommit == nil {
			continue
		}

		maps.Copy(commentsByCommit, td.CommentsByCommit)
	}

	return commentsByCommit
}

func buildCommitsByTickFromTicks(ticks []analyze.TICK) map[int][]gitlib.Hash {
	ct := make(map[int][]gitlib.Hash)

	for _, tick := range ticks {
		td, ok := tick.Data.(*TickData)
		if !ok || td == nil || td.CommentsByCommit == nil {
			continue
		}

		hashes := make([]gitlib.Hash, 0, len(td.CommentsByCommit))

		for h := range td.CommentsByCommit {
			hashes = append(hashes, gitlib.NewHash(h))
		}

		ct[tick.Tick] = append(ct[tick.Tick], hashes...)
	}

	return ct
}
