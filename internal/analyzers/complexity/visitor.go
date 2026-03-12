package complexity

import (
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

const anonymousFunctionName = "anonymous"

// Visitor implements NodeVisitor for complexity analysis.
type Visitor struct {
	analyzer  *Analyzer
	functions []*node.Node
}

// NewVisitor creates a new Visitor.
func NewVisitor() *Visitor {
	return &Visitor{
		analyzer:  NewAnalyzer(),
		functions: make([]*node.Node, 0),
	}
}

// OnEnter is called when entering a node during AST traversal.
func (v *Visitor) OnEnter(n *node.Node, _ int) {
	if v.isFunction(n) {
		v.functions = append(v.functions, n)
	}
}

// OnExit is called when exiting a node during AST traversal.
func (v *Visitor) OnExit(_ *node.Node, _ int) {
	// Intentionally no-op; metrics are computed in GetReport().
}

// GetReport returns the collected analysis report.
func (v *Visitor) GetReport() analyze.Report {
	if len(v.functions) == 0 {
		return v.analyzer.buildEmptyResult("No functions found")
	}

	config := v.analyzer.DefaultConfig()
	functionMetrics, totals := v.analyzer.calculateAllFunctionMetrics(v.functions, config)
	detailedFunctions := v.analyzer.buildDetailedFunctionsTable(functionMetrics, config)
	avgComplexity := v.analyzer.calculateAverageComplexity(totals, len(v.functions))
	message := v.analyzer.getComplexityMessage(avgComplexity)

	return v.analyzer.buildResult(len(v.functions), avgComplexity, totals, detailedFunctions, message)
}

func (v *Visitor) isFunction(n *node.Node) bool {
	return n.HasAnyType(node.UASTFunction, node.UASTMethod) ||
		n.HasAllRoles(node.RoleFunction, node.RoleDeclaration)
}
