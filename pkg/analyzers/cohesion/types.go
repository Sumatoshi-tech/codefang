package cohesion

import (
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// CohesionAnalyzer performs cohesion analysis on UAST
type CohesionAnalyzer struct {
	traverser *common.UASTTraverser
	extractor *common.DataExtractor
}

// Function represents a function with its cohesion metrics
type Function struct {
	Name      string
	LineCount int
	Variables []string
	Cohesion  float64
}

// NewCohesionAnalyzer creates a new CohesionAnalyzer with generic components
func NewCohesionAnalyzer() *CohesionAnalyzer {
	// Configure UAST traverser for functions
	traversalConfig := common.TraversalConfig{
		Filters: []common.NodeFilter{
			{
				Types: []string{node.UASTFunction, node.UASTMethod},
				Roles: []string{node.RoleFunction},
			},
		},
		MaxDepth: 10,
	}

	// Configure data extractor
	extractionConfig := common.ExtractionConfig{
		DefaultExtractors: true,
		NameExtractors: map[string]common.NameExtractor{
			"function_name": func(n *node.Node) (string, bool) {
				return common.ExtractFunctionName(n)
			},
			"variable_name": func(n *node.Node) (string, bool) {
				return common.ExtractVariableName(n)
			},
		},
	}

	return &CohesionAnalyzer{
		traverser: common.NewUASTTraverser(traversalConfig),
		extractor: common.NewDataExtractor(extractionConfig),
	}
}

// CreateAggregator creates a new aggregator for cohesion analysis
func (c *CohesionAnalyzer) CreateAggregator() analyze.ResultAggregator {
	return NewCohesionAggregator()
}
