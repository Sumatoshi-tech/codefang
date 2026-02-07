// Package sentiment provides sentiment functionality.
package sentiment

import (
	"encoding/json"
	"fmt"
	"io"
	"regexp"
	"sort"
	"strings"

	"gopkg.in/yaml.v3"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// MinCommentLengthThresholdHigh is the minimum character length for a comment to be included in sentiment analysis.
const (
	MinCommentLengthThresholdHigh = 10
)

// HistoryAnalyzer tracks comment sentiment across commit history.
type HistoryAnalyzer struct {
	l interface { //nolint:unused // used via dependency injection.
		Warnf(format string, args ...any)
	}
	UAST             *plumbing.UASTChangesAnalyzer
	Ticks            *plumbing.TicksSinceStart
	commentsByTick   map[int][]string
	commitsByTick    map[int][]gitlib.Hash
	MinCommentLength int
	Gap              float32
}

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
	filteredFirstCharRE = regexp.MustCompile("[^a-zA-Z0-9]")
	filteredCharsRE     = regexp.MustCompile(`[^-a-zA-Z0-9_:;,./?!#&%+*=\n \t()]+`)
	charsRE             = regexp.MustCompile("[a-zA-Z]+")
	functionNameRE      = regexp.MustCompile(`\s*[a-zA-Z_][a-zA-Z_0-9]*\(\)`)
	whitespaceRE        = regexp.MustCompile(`\s+`)
	licenseRE           = regexp.MustCompile("(?i)[li[cs]en[cs][ei]|copyright|©")
)

// Name returns the name of the analyzer.
func (s *HistoryAnalyzer) Name() string {
	return "Sentiment"
}

// Flag returns the CLI flag for the analyzer.
func (s *HistoryAnalyzer) Flag() string {
	return "sentiment"
}

// Description returns a human-readable description of the analyzer.
func (s *HistoryAnalyzer) Description() string {
	return "Classifies each new or changed comment per commit as containing positive or negative emotions."
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (s *HistoryAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{
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
	}
}

// Configure sets up the analyzer with the provided facts.
func (s *HistoryAnalyzer) Configure(facts map[string]any) error {
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

func (s *HistoryAnalyzer) validate() {
	if s.Gap < 0 || s.Gap >= 1 {
		s.Gap = DefaultCommentSentimentGap
	}

	if s.MinCommentLength < MinCommentLengthThresholdHigh {
		s.MinCommentLength = DefaultCommentSentimentCommentMinLength
	}
}

// Initialize prepares the analyzer for processing commits.
func (s *HistoryAnalyzer) Initialize(_ *gitlib.Repository) error {
	s.commentsByTick = map[int][]string{}
	s.validate()

	return nil
}

// Consume processes a single commit with the provided dependency results.
func (s *HistoryAnalyzer) Consume(_ *analyze.Context) error {
	changes := s.UAST.Changes()
	tick := s.Ticks.Tick

	var commentNodes []*node.Node

	for _, change := range changes {
		if change.After != nil {
			extractComments(change.After, &commentNodes)
		}
	}

	comments := s.mergeComments(commentNodes)
	s.commentsByTick[tick] = append(s.commentsByTick[tick], comments...)

	return nil
}

func extractComments(root *node.Node, result *[]*node.Node) {
	if root.Type == node.UASTComment {
		*result = append(*result, root)
	}

	for _, child := range root.Children {
		extractComments(child, result)
	}
}

// groupCommentsByLine groups extracted comment nodes by their starting line number.
// It returns a map from line number to comment nodes on that line, and a sorted slice of line numbers.
func groupCommentsByLine(extracted []*node.Node) (grouped map[int][]*node.Node, sortedLines []int) {
	lines := map[int][]*node.Node{}

	for _, n := range extracted {
		if n.Pos == nil {
			continue
		}

		lineno := int(n.Pos.StartLine) //nolint:gosec // security concern is acceptable here.
		lines[lineno] = append(lines[lineno], n)
	}

	lineNums := make([]int, 0, len(lines))
	for line := range lines {
		lineNums = append(lineNums, line)
	}

	sort.Ints(lineNums)

	return lines, lineNums
}

// mergeAdjacentComments merges comment tokens from adjacent lines into combined strings.
// Comments on consecutive lines (or lines within the same multi-line comment span) are joined together.
func mergeAdjacentComments(lines map[int][]*node.Node, lineNums []int) []string {
	var mergedComments []string

	var buffer []string

	for i, line := range lineNums {
		lineNodes := lines[line]

		maxEnd := line
		for _, n := range lineNodes {
			if n.Pos != nil && maxEnd < int(n.Pos.EndLine) { //nolint:gosec // security concern is acceptable here.
				maxEnd = int(n.Pos.EndLine) //nolint:gosec // security concern is acceptable here.
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

// filterComments filters out comments that are too short, contain mostly non-letter characters,
// start with non-alphanumeric characters, or match license patterns.
func filterComments(comments []string, minLength int) []string {
	filteredComments := make([]string, 0, len(comments))

	for _, comment := range comments {
		comment = strings.TrimSpace(comment)
		if comment == "" || filteredFirstCharRE.MatchString(comment[:1]) {
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

func (s *HistoryAnalyzer) mergeComments(extracted []*node.Node) []string {
	lines, lineNums := groupCommentsByLine(extracted)
	mergedComments := mergeAdjacentComments(lines, lineNums)

	return filterComments(mergedComments, s.MinCommentLength)
}

// Finalize completes the analysis and returns the result.
func (s *HistoryAnalyzer) Finalize() (analyze.Report, error) {
	emotions := map[int]float32{}
	// Sentiment analysis logic (placeholders).
	for tick, comments := range s.commentsByTick {
		if len(comments) > 0 {
			emotions[tick] = 0.5 // Mock value.
		}
	}

	return analyze.Report{
		"emotions_by_tick": emotions,
		"comments_by_tick": s.commentsByTick,
		"commits_by_tick":  s.commitsByTick,
	}, nil
}

// Fork creates a copy of the analyzer for parallel processing.
// Each fork gets independent mutable state while sharing read-only config.
func (s *HistoryAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		clone := &HistoryAnalyzer{
			UAST:             s.UAST,
			Ticks:            s.Ticks,
			MinCommentLength: s.MinCommentLength,
			Gap:              s.Gap,
			commitsByTick:    s.commitsByTick, // shared read-only
		}
		// Initialize independent state for each fork
		clone.commentsByTick = make(map[int][]string)

		res[i] = clone
	}

	return res
}

// Merge combines results from forked analyzer branches.
func (s *HistoryAnalyzer) Merge(branches []analyze.HistoryAnalyzer) {
	for _, branch := range branches {
		other, ok := branch.(*HistoryAnalyzer)
		if !ok {
			continue
		}

		s.mergeCommentsByTick(other.commentsByTick)
	}
}

// mergeCommentsByTick combines comments from another analyzer.
func (s *HistoryAnalyzer) mergeCommentsByTick(other map[int][]string) {
	for tick, comments := range other {
		s.commentsByTick[tick] = append(s.commentsByTick[tick], comments...)
	}
}

// Serialize writes the analysis result to the given writer.
func (s *HistoryAnalyzer) Serialize(result analyze.Report, format string, writer io.Writer) error {
	switch format {
	case analyze.FormatJSON:
		return s.serializeJSON(result, writer)
	case analyze.FormatYAML:
		return s.serializeYAML(result, writer)
	case analyze.FormatPlot:
		return s.generatePlot(result, writer)
	default:
		return s.serializeYAML(result, writer)
	}
}

func (s *HistoryAnalyzer) serializeJSON(result analyze.Report, writer io.Writer) error {
	metrics, err := ComputeAllMetrics(result)
	if err != nil {
		metrics = &ComputedMetrics{}
	}

	err = json.NewEncoder(writer).Encode(metrics)
	if err != nil {
		return fmt.Errorf("json encode: %w", err)
	}

	return nil
}

func (s *HistoryAnalyzer) serializeYAML(result analyze.Report, writer io.Writer) error {
	metrics, err := ComputeAllMetrics(result)
	if err != nil {
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

// FormatReport writes the formatted analysis report to the given writer.
func (s *HistoryAnalyzer) FormatReport(report analyze.Report, writer io.Writer) error {
	return s.Serialize(report, analyze.FormatYAML, writer)
}
