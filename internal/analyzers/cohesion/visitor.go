package cohesion

import (
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

type cohesionContext struct {
	functionNode *node.Node
	function     Function
}

// Visitor implements NodeVisitor for cohesion analysis.
type Visitor struct {
	extractor *common.DataExtractor
	contexts  *common.ContextStack[*cohesionContext]
	functions []Function
}

// NewVisitor creates a new Visitor.
func NewVisitor() *Visitor {
	extractionConfig := common.ExtractionConfig{
		DefaultExtractors: true,
		NameExtractors: map[string]common.NameExtractor{
			"function_name": common.ExtractEntityName,
			"variable_name": common.ExtractEntityName,
		},
	}

	return &Visitor{
		contexts:  common.NewContextStack[*cohesionContext](),
		functions: make([]Function, 0),
		extractor: common.NewDataExtractor(extractionConfig),
	}
}

// OnEnter is called when entering a node during AST traversal.
func (v *Visitor) OnEnter(n *node.Node, _ int) {
	if v.isFunction(n) {
		v.pushContext(n)
	}

	if ctx := v.currentContext(); ctx != nil {
		v.processNode(ctx, n)
	}
}

// OnExit is called when exiting a node during AST traversal.
func (v *Visitor) OnExit(n *node.Node, _ int) {
	if v.isFunction(n) {
		v.popContext()
	}
}

// GetReport returns the collected analysis report.
func (v *Visitor) GetReport() analyze.Report {
	analyzer := &Analyzer{
		traverser: common.NewUASTTraverser(common.TraversalConfig{}),
		extractor: v.extractor,
	}

	if len(v.functions) == 0 {
		return analyzer.buildEmptyResult()
	}

	// Compute per-function cohesion using variable sharing ratio.
	// This requires the full list of functions to determine shared variables.
	analyzer.computePerFunctionCohesion(v.functions)

	metrics := analyzer.calculateMetrics(v.functions)

	return analyzer.buildResult(v.functions, metrics)
}

func (v *Visitor) isFunction(n *node.Node) bool {
	return n.HasAnyType(node.UASTFunction, node.UASTMethod) ||
		n.HasAllRoles(node.RoleFunction, node.RoleDeclaration)
}

func (v *Visitor) pushContext(funcNode *node.Node) {
	analyzer := &Analyzer{
		traverser: common.NewUASTTraverser(common.TraversalConfig{}),
		extractor: v.extractor,
	}

	name := analyzer.extractFunctionName(funcNode)
	lineCount := analyzer.traverser.CountLines(funcNode)

	function := Function{
		Name:      name,
		LineCount: lineCount,
		Variables: make([]string, 0),
		Cohesion:  0.0,
	}

	ctx := &cohesionContext{
		functionNode: funcNode,
		function:     function,
	}
	v.contexts.Push(ctx)
}

func (v *Visitor) popContext() {
	ctx, ok := v.contexts.Pop()
	if !ok {
		return
	}

	// Store collected function.
	v.functions = append(v.functions, ctx.function)
}

func (v *Visitor) currentContext() *cohesionContext {
	ctx, ok := v.contexts.Current()
	if !ok {
		return nil
	}

	return ctx
}

func (v *Visitor) processNode(ctx *cohesionContext, current *node.Node) {
	// Create temporary analyzer to reuse helper methods
	// In a real implementation, we might want to move these helpers to a shared utility.
	analyzer := &Analyzer{
		extractor: v.extractor,
	}

	variables := &ctx.function.Variables
	analyzer.processVariableNode(current, variables)
}
