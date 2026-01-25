package comments

import (
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// CommentsVisitor implements NodeVisitor for comment analysis
type CommentsVisitor struct {
	comments  []*node.Node
	functions []*node.Node
	
	// Helpers
	extractor *common.DataExtractor
}

// NewCommentsVisitor creates a new CommentsVisitor
func NewCommentsVisitor() *CommentsVisitor {
	extractionConfig := common.ExtractionConfig{
		DefaultExtractors: true,
		NameExtractors: map[string]common.NameExtractor{
			"function_name": common.ExtractFunctionName,
		},
	}

	return &CommentsVisitor{
		comments:  make([]*node.Node, 0),
		functions: make([]*node.Node, 0),
		extractor: common.NewDataExtractor(extractionConfig),
	}
}

func (v *CommentsVisitor) OnEnter(n *node.Node, depth int) {
	if n.Type == node.UASTComment {
		v.comments = append(v.comments, n)
	}

	if v.isFunction(n) {
		v.functions = append(v.functions, n)
	}
}

func (v *CommentsVisitor) OnExit(n *node.Node, depth int) {
	// Nothing to do on exit
}

func (v *CommentsVisitor) GetReport() analyze.Report {
	analyzer := &CommentsAnalyzer{
		traverser: common.NewUASTTraverser(common.TraversalConfig{}),
		extractor: v.extractor,
	}

	if len(v.comments) == 0 {
		return analyzer.buildEmptyResult()
	}

	config := analyzer.DefaultConfig()
	commentDetails := analyzer.analyzeCommentPlacement(v.comments, v.functions, config)
	metrics := analyzer.calculateMetrics(commentDetails, v.functions)

	return analyzer.buildResult(commentDetails, v.functions, metrics)
}

func (v *CommentsVisitor) isFunction(n *node.Node) bool {
	functionTypes := map[node.Type]bool{
		node.UASTFunction:  true,
		node.UASTMethod:    true,
		node.UASTClass:     true,
		node.UASTInterface: true,
		node.UASTStruct:    true,
	}
	return functionTypes[n.Type]
}
