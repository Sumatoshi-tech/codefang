package cohesion

import (
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

type cohesionContext struct {
	functionNode *node.Node
	function     Function
	nestingLevel int
}

// CohesionVisitor implements NodeVisitor for cohesion analysis
type CohesionVisitor struct {
	contexts []*cohesionContext

	// Collected functions
	functions []Function

	// Helpers
	extractor *common.DataExtractor
}

// NewCohesionVisitor creates a new CohesionVisitor
func NewCohesionVisitor() *CohesionVisitor {
	extractionConfig := common.ExtractionConfig{
		DefaultExtractors: true,
		NameExtractors: map[string]common.NameExtractor{
			"function_name": common.ExtractFunctionName,
			"variable_name": common.ExtractVariableName,
		},
	}

	return &CohesionVisitor{
		contexts:  make([]*cohesionContext, 0),
		functions: make([]Function, 0),
		extractor: common.NewDataExtractor(extractionConfig),
	}
}

func (v *CohesionVisitor) OnEnter(n *node.Node, depth int) {
	if v.isFunction(n) {
		v.pushContext(n)
	}

	if ctx := v.currentContext(); ctx != nil {
		v.processNode(ctx, n)
	}
}

func (v *CohesionVisitor) OnExit(n *node.Node, depth int) {
	if v.isFunction(n) {
		v.popContext()
	}
}

func (v *CohesionVisitor) GetReport() analyze.Report {
	analyzer := &CohesionAnalyzer{
		traverser: common.NewUASTTraverser(common.TraversalConfig{}),
		extractor: v.extractor,
	}

	if len(v.functions) == 0 {
		return analyzer.buildEmptyResult()
	}

	// Calculate cohesion metrics for collected functions
	for i := range v.functions {
		v.functions[i].Cohesion = analyzer.calculateFunctionLevelCohesion(v.functions[i])
	}

	metrics := analyzer.calculateMetrics(v.functions)
	return analyzer.buildResult(v.functions, metrics)
}

func (v *CohesionVisitor) isFunction(n *node.Node) bool {
	return n.HasAnyType(node.UASTFunction, node.UASTMethod) ||
		n.HasAllRoles(node.RoleFunction, node.RoleDeclaration)
}

func (v *CohesionVisitor) pushContext(n *node.Node) {
	analyzer := &CohesionAnalyzer{
		traverser: common.NewUASTTraverser(common.TraversalConfig{}),
		extractor: v.extractor,
	}

	name := analyzer.extractFunctionName(n)
	lineCount := analyzer.traverser.CountLines(n)

	function := Function{
		Name:      name,
		LineCount: lineCount,
		Variables: make([]string, 0),
		Cohesion:  0.0,
	}

	ctx := &cohesionContext{
		functionNode: n,
		function:     function,
	}
	v.contexts = append(v.contexts, ctx)
}

func (v *CohesionVisitor) popContext() {
	if len(v.contexts) == 0 {
		return
	}
	ctx := v.contexts[len(v.contexts)-1]
	v.contexts = v.contexts[:len(v.contexts)-1]

	// Store collected function
	v.functions = append(v.functions, ctx.function)
}

func (v *CohesionVisitor) currentContext() *cohesionContext {
	if len(v.contexts) == 0 {
		return nil
	}
	return v.contexts[len(v.contexts)-1]
}

func (v *CohesionVisitor) processNode(ctx *cohesionContext, n *node.Node) {
	// Create temporary analyzer to reuse helper methods
	// In a real implementation, we might want to move these helpers to a shared utility
	analyzer := &CohesionAnalyzer{
		extractor: v.extractor,
	}

	variables := &ctx.function.Variables
	analyzer.processVariableNode(n, variables)
}
