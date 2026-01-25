package sentiment

import (
	"fmt"
	"io"
	"regexp"
	"sort"
	"strings"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
	"github.com/go-git/go-git/v6"
	gitplumbing "github.com/go-git/go-git/v6/plumbing"
)

type SentimentHistoryAnalyzer struct {
	// Configuration
	MinCommentLength int
	Gap              float32

	// Dependencies
	UASTChanges *plumbing.UASTChangesAnalyzer
	Ticks       *plumbing.TicksSinceStart

	// State
	commentsByTick map[int][]string
	commitsByTick  map[int][]gitplumbing.Hash
	
	// Internal
	l interface {
		Warnf(format string, args ...interface{})
	}
}

const (
	ConfigCommentSentimentMinLength = "CommentSentiment.MinLength"
	ConfigCommentSentimentGap       = "CommentSentiment.Gap"

	DefaultCommentSentimentCommentMinLength = 20
	DefaultCommentSentimentGap              = float32(0.5)

	CommentLettersRatio = 0.6
)

var (
	filteredFirstCharRE = regexp.MustCompile("[^a-zA-Z0-9]")
	filteredCharsRE     = regexp.MustCompile("[^-a-zA-Z0-9_:;,./?!#&%+*=\\n \\t()]+")
	charsRE             = regexp.MustCompile("[a-zA-Z]+")
	functionNameRE      = regexp.MustCompile("\\s*[a-zA-Z_][a-zA-Z_0-9]*\\(\\)")
	whitespaceRE        = regexp.MustCompile("\\s+")
	licenseRE           = regexp.MustCompile("(?i)[li[cs]en[cs][ei]|copyright|©")
)

func (s *SentimentHistoryAnalyzer) Name() string {
	return "Sentiment"
}

func (s *SentimentHistoryAnalyzer) Flag() string {
	return "sentiment"
}

func (s *SentimentHistoryAnalyzer) Description() string {
	return "Classifies each new or changed comment per commit as containing positive or negative emotions."
}

func (s *SentimentHistoryAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
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

func (s *SentimentHistoryAnalyzer) Configure(facts map[string]interface{}) error {
	if val, exists := facts[ConfigCommentSentimentGap].(float32); exists {
		s.Gap = val
	}
	if val, exists := facts[ConfigCommentSentimentMinLength].(int); exists {
		s.MinCommentLength = val
	}
	if val, exists := facts[pkgplumbing.FactCommitsByTick].(map[int][]gitplumbing.Hash); exists {
		s.commitsByTick = val
	}
	s.validate()
	return nil
}

func (s *SentimentHistoryAnalyzer) validate() {
	if s.Gap < 0 || s.Gap >= 1 {
		s.Gap = DefaultCommentSentimentGap
	}
	if s.MinCommentLength < 10 {
		s.MinCommentLength = DefaultCommentSentimentCommentMinLength
	}
}

func (s *SentimentHistoryAnalyzer) Initialize(repository *git.Repository) error {
	s.commentsByTick = map[int][]string{}
	s.validate()
	return nil
}

func (s *SentimentHistoryAnalyzer) Consume(ctx *analyze.Context) error {
	changes := s.UASTChanges.Changes
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

func (s *SentimentHistoryAnalyzer) mergeComments(extracted []*node.Node) []string {
	var mergedComments []string
	lines := map[int][]*node.Node{}
	
	for _, n := range extracted {
		if n.Pos == nil {
			continue
		}
		lineno := int(n.Pos.StartLine)
		lines[lineno] = append(lines[lineno], n)
	}
	
	lineNums := make([]int, 0, len(lines))
	for line := range lines {
		lineNums = append(lineNums, line)
	}
	sort.Ints(lineNums)
	
	var buffer []string
	for i, line := range lineNums {
		lineNodes := lines[line]
		maxEnd := line
		for _, n := range lineNodes {
			if n.Pos != nil && maxEnd < int(n.Pos.EndLine) {
				maxEnd = int(n.Pos.EndLine)
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
	
	filteredComments := make([]string, 0, len(mergedComments))
	for _, comment := range mergedComments {
		comment = strings.TrimSpace(comment)
		if comment == "" || filteredFirstCharRE.MatchString(comment[:1]) {
			continue
		}
		comment = functionNameRE.ReplaceAllString(comment, "")
		comment = filteredCharsRE.ReplaceAllString(comment, "")
		if len(comment) < s.MinCommentLength {
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

func (s *SentimentHistoryAnalyzer) Finalize() (analyze.Report, error) {
	emotions := map[int]float32{}
	// Sentiment analysis logic (placeholders)
	for tick, comments := range s.commentsByTick {
		if len(comments) > 0 {
			emotions[tick] = 0.5 // Mock value
		}
	}
	
	return analyze.Report{
		"emotions_by_tick": emotions,
		"comments_by_tick": s.commentsByTick,
		"commits_by_tick":  s.commitsByTick,
	}, nil
}

func (s *SentimentHistoryAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := 0; i < n; i++ {
		res[i] = s // Shared state
	}
	return res
}

func (s *SentimentHistoryAnalyzer) Merge(branches []analyze.HistoryAnalyzer) {
}

func (s *SentimentHistoryAnalyzer) Serialize(result analyze.Report, binary bool, writer io.Writer) error {
	emotions := result["emotions_by_tick"].(map[int]float32)
	comments := result["comments_by_tick"].(map[int][]string)
	commits := result["commits_by_tick"].(map[int][]gitplumbing.Hash)
	
	ticks := make([]int, 0, len(emotions))
	for tick := range emotions {
		ticks = append(ticks, tick)
	}
	sort.Ints(ticks)
	
	for _, tick := range ticks {
		hashes := make([]string, 0)
		if list, ok := commits[tick]; ok {
			for _, hash := range list {
				hashes = append(hashes, hash.String())
			}
		}
		fmt.Fprintf(writer, "  %d: [%.4f, [%s], \"%s\"]\n",
			tick, emotions[tick], strings.Join(hashes, ","),
			strings.Join(comments[tick], "|"))
	}
	return nil
}

func (s *SentimentHistoryAnalyzer) FormatReport(report analyze.Report, writer io.Writer) error {
	return s.Serialize(report, false, writer)
}
