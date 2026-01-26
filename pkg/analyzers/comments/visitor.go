package comments

import (
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// Visitor implements NodeVisitor for comment analysis.
type Visitor struct {
	extractor *common.DataExtractor
	comments  []*node.Node
	functions []*node.Node
}

// NewVisitor creates a new Visitor.
func NewVisitor() *Visitor {
	extractionConfig := common.ExtractionConfig{
		DefaultExtractors: true,
		NameExtractors: map[string]common.NameExtractor{
			"function_name": common.ExtractFunctionName,
		},
	}

	return &Visitor{
		comments:  make([]*node.Node, 0),
		functions: make([]*node.Node, 0),
		extractor: common.NewDataExtractor(extractionConfig),
	}
}

// OnEnter is called when entering a node during AST traversal.
func (v *Visitor) OnEnter(n *node.Node, _ int) {
	if n.Type == node.UASTComment {
		v.comments = append(v.comments, n)
	}

	if v.isFunction(n) {
		v.functions = append(v.functions, n)
	}
}

// OnExit is called when exiting a node during AST traversal.
func (v *Visitor) OnExit(_ *node.Node, _ int) {
	// Nothing to do on exit.
}

// GetReport returns the collected analysis report.
func (v *Visitor) GetReport() analyze.Report {
	analyzer := &Analyzer{
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

func (v *Visitor) isFunction(target *node.Node) bool {
	functionTypes := map[node.Type]bool{
		node.UASTFunction:  true,
		node.UASTMethod:    true,
		node.UASTClass:     true,
		node.UASTInterface: true,
		node.UASTStruct:    true,
	}

	return functionTypes[target.Type]
}
