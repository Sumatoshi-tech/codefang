package complexity

import (
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

const anonymousFunctionName = "anonymous"

type complexityContext struct {
	functionNode *node.Node
	metrics      FunctionMetrics
	nestingLevel int
}

// Visitor implements NodeVisitor for complexity analysis.
type Visitor struct {
	totals            map[string]int
	contexts          []*complexityContext
	detailedFunctions []map[string]any
	maxComplexity     int
}

// NewVisitor creates a new Visitor.
func NewVisitor() *Visitor {
	return &Visitor{
		contexts: make([]*complexityContext, 0),
		totals: map[string]int{
			"total_functions":  0,
			"total_complexity": 0,
			"nesting_depth":    0,
			"decision_points":  0,
		},
		detailedFunctions: make([]map[string]any, 0),
	}
}

// OnEnter is called when entering a node during AST traversal.
func (v *Visitor) OnEnter(n *node.Node, _ int) {
	if v.isFunction(n) {
		v.pushContext(n)
	}

	if ctx := v.currentContext(); ctx != nil {
		v.updateMetricsEnter(ctx, n)
	}
}

// OnExit is called when exiting a node during AST traversal.
func (v *Visitor) OnExit(n *node.Node, _ int) {
	if ctx := v.currentContext(); ctx != nil {
		v.updateMetricsExit(ctx, n)
	}

	if v.isFunction(n) {
		v.popContext()
	}
}

// GetReport returns the collected analysis report.
func (v *Visitor) GetReport() analyze.Report {
	totalFunctions := v.totals["total_functions"]
	totalComplexity := v.totals["total_complexity"]

	var avgComplexity float64
	if totalFunctions > 0 {
		avgComplexity = float64(totalComplexity) / float64(totalFunctions)
	}

	report := analyze.Report{
		"total_functions":    totalFunctions,
		"total_complexity":   totalComplexity,
		"max_complexity":     v.maxComplexity,
		"average_complexity": avgComplexity,
		"nesting_depth":      v.totals["nesting_depth"],
		"decision_points":    v.totals["decision_points"],
		"message":            buildComplexityMessage(avgComplexity),
	}

	if len(v.detailedFunctions) > 0 {
		report["functions"] = v.detailedFunctions
	}

	return report
}

func (v *Visitor) isFunction(n *node.Node) bool {
	return n.HasAnyType(node.UASTFunction, node.UASTMethod) ||
		n.HasAllRoles(node.RoleFunction, node.RoleDeclaration)
}

func (v *Visitor) pushContext(funcNode *node.Node) {
	name, _ := common.ExtractFunctionName(funcNode)
	if name == "" {
		name = anonymousFunctionName
	}

	ctx := &complexityContext{
		functionNode: funcNode,
		metrics: FunctionMetrics{
			Name:                 name,
			CyclomaticComplexity: 1, // Base complexity.
		},
		nestingLevel: 0,
	}
	v.contexts = append(v.contexts, ctx)
}

func (v *Visitor) popContext() {
	if len(v.contexts) == 0 {
		return
	}

	ctx := v.contexts[len(v.contexts)-1]
	v.contexts = v.contexts[:len(v.contexts)-1]

	// Aggregate results.
	v.totals["total_functions"]++
	v.totals["total_complexity"] += ctx.metrics.CyclomaticComplexity
	v.totals["nesting_depth"] += ctx.metrics.NestingDepth
	v.totals["decision_points"] += ctx.metrics.DecisionPoints

	if ctx.metrics.CyclomaticComplexity > v.maxComplexity {
		v.maxComplexity = ctx.metrics.CyclomaticComplexity
	}

	// Collect detailed function info.
	v.detailedFunctions = append(v.detailedFunctions, map[string]any{
		"name":                  ctx.metrics.Name,
		"cyclomatic_complexity": ctx.metrics.CyclomaticComplexity,
		"nesting_depth":         ctx.metrics.NestingDepth,
	})
}

func (v *Visitor) currentContext() *complexityContext {
	if len(v.contexts) == 0 {
		return nil
	}

	return v.contexts[len(v.contexts)-1]
}

func (v *Visitor) updateMetricsEnter(ctx *complexityContext, n *node.Node) {
	if v.isDecisionPoint(n) {
		ctx.metrics.CyclomaticComplexity++
		ctx.metrics.DecisionPoints++
	}

	if v.isNestingStart(n) {
		ctx.nestingLevel++
		if ctx.nestingLevel > ctx.metrics.NestingDepth {
			ctx.metrics.NestingDepth = ctx.nestingLevel
		}
	}
}

func (v *Visitor) updateMetricsExit(ctx *complexityContext, n *node.Node) {
	if v.isNestingStart(n) {
		ctx.nestingLevel--
	}
}

func (v *Visitor) isDecisionPoint(n *node.Node) bool {
	if n.Type == node.UASTIf || n.Type == node.UASTLoop || n.Type == node.UASTSwitch {
		return true
	}

	if n.HasAnyRole(node.RoleCondition) {
		return true
	}

	return false
}

func (v *Visitor) isNestingStart(n *node.Node) bool {
	return n.Type == node.UASTIf || n.Type == node.UASTLoop ||
		n.Type == node.UASTSwitch || n.Type == node.UASTTry ||
		n.Type == node.UASTBlock || n.Type == node.UASTFunction
}
