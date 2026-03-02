package comments

import (
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// MaxDepthValue is the default maximum UAST traversal depth for comment analysis.
const (
	MaxDepthValue = 10
	magic500      = 500
)

const msgGoodCommentQuality = "Good comment quality with room for improvement"

// Analyzer provides comment placement analysis.
type Analyzer struct {
	traverser *common.UASTTraverser
	extractor *common.DataExtractor
}

// CommentMetrics holds comment analysis results.
type CommentMetrics struct {
	FunctionSummary     map[string]FunctionInfo `json:"function_summary"`
	CommentDetails      []CommentDetail         `json:"comment_details"`
	TotalComments       int                     `json:"total_comments"`
	GoodComments        int                     `json:"good_comments"`
	BadComments         int                     `json:"bad_comments"`
	OverallScore        float64                 `json:"overall_score"`
	TotalFunctions      int                     `json:"total_functions"`
	DocumentedFunctions int                     `json:"documented_functions"`
}

// CommentDetail holds information about a specific comment.
type CommentDetail struct {
	Quality        string  `json:"quality"`
	Token          string  `json:"token"`
	Position       string  `json:"position"`
	Type           string  `json:"type"`
	Recommendation string  `json:"recommendation"`
	TargetType     string  `json:"target_type"`
	TargetName     string  `json:"target_name"`
	Score          float64 `json:"score"`
	StartLine      int     `json:"start_line"`
	EndLine        int     `json:"end_line"`
	Length         int     `json:"length"`
	LineNumber     int     `json:"line_number"`
	IsGood         bool    `json:"is_good"`
}

// FunctionInfo holds information about a function.
type FunctionInfo struct {
	Name          string  `json:"name"`
	Type          string  `json:"type"`
	CommentType   string  `json:"comment_type"`
	Documentation string  `json:"documentation"`
	StartLine     int     `json:"start_line"`
	EndLine       int     `json:"end_line"`
	CommentScore  float64 `json:"comment_score"`
	HasComment    bool    `json:"has_comment"`
	NeedsComment  bool    `json:"needs_comment"`
}

// CommentConfig holds configuration for comment analysis.
type CommentConfig struct {
	PenaltyScores    map[string]float64
	RewardScore      float64
	MaxCommentLength int
}

// CommentBlock represents a group of consecutive comment lines.
type CommentBlock struct {
	FullText  string
	Comments  []*node.Node
	StartLine int
	EndLine   int
}

// NewAnalyzer creates a new Analyzer with generic components.
func NewAnalyzer() *Analyzer {
	traversalConfig := createTraversalConfig()
	extractionConfig := createExtractionConfig()

	return &Analyzer{
		traverser: common.NewUASTTraverser(traversalConfig),
		extractor: common.NewDataExtractor(extractionConfig),
	}
}

// createTraversalConfig creates the traversal configuration for UAST analysis.
func createTraversalConfig() common.TraversalConfig {
	return common.TraversalConfig{
		Filters: []common.NodeFilter{
			{
				Types: []string{node.UASTComment},
				Roles: []string{node.RoleComment},
			},
			{
				Types: []string{node.UASTFunction, node.UASTMethod, node.UASTClass, node.UASTInterface, node.UASTStruct},
				Roles: []string{node.RoleFunction, node.RoleDeclaration},
			},
		},
		MaxDepth: MaxDepthValue,
	}
}

// createExtractionConfig creates the extraction configuration for data analysis.
func createExtractionConfig() common.ExtractionConfig {
	return common.ExtractionConfig{
		DefaultExtractors: true,
		NameExtractors: map[string]common.NameExtractor{
			"function_name": createFunctionNameExtractor(),
			"comment_text":  createCommentTextExtractor(),
		},
	}
}

// createFunctionNameExtractor creates a function name extractor.
func createFunctionNameExtractor() common.NameExtractor {
	return common.ExtractEntityName
}

// createCommentTextExtractor creates a comment text extractor.
func createCommentTextExtractor() common.NameExtractor {
	return func(n *node.Node) (string, bool) {
		if n == nil || n.Token == "" {
			return "", false
		}

		return n.Token, true
	}
}

// CreateAggregator creates a new aggregator for comment analysis.
func (c *Analyzer) CreateAggregator() analyze.ResultAggregator {
	return NewAggregator()
}

// CreateVisitor creates a new visitor for comments analysis.
func (c *Analyzer) CreateVisitor() analyze.AnalysisVisitor {
	return NewVisitor()
}

// DefaultConfig returns the default configuration for comment analysis.
func (c *Analyzer) DefaultConfig() CommentConfig {
	return CommentConfig{
		RewardScore:      getDefaultRewardScore(),
		PenaltyScores:    getDefaultPenaltyScores(),
		MaxCommentLength: getDefaultMaxCommentLength(),
	}
}

// getDefaultRewardScore returns the default reward score for good comments.
func getDefaultRewardScore() float64 {
	return 1.0
}

// getDefaultPenaltyScores returns the default penalty scores for different node types.
func getDefaultPenaltyScores() map[string]float64 {
	return map[string]float64{
		node.UASTFunction:   -0.5,
		node.UASTMethod:     -0.5,
		node.UASTClass:      -0.3,
		node.UASTInterface:  -0.3,
		node.UASTStruct:     -0.3,
		node.UASTVariable:   -0.1,
		node.UASTAssignment: -0.1,
		node.UASTCall:       -0.1,
	}
}

// getDefaultMaxCommentLength returns the default maximum comment length.
func getDefaultMaxCommentLength() int {
	return magic500
}
