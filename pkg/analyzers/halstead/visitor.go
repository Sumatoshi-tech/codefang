package halstead

import (
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

type halsteadContext struct {
	functionNode *node.Node
	metrics      *FunctionHalsteadMetrics
}

// Visitor implements NodeVisitor for Halstead analysis.
type Visitor struct {
	metrics         *MetricsCalculator
	detector        *OperatorOperandDetector
	functionMetrics map[string]*FunctionHalsteadMetrics
	contexts        []*halsteadContext
	nodeStack       []*node.Node
}

// NewVisitor creates a new Visitor.
func NewVisitor() *Visitor {
	return &Visitor{
		contexts:        make([]*halsteadContext, 0),
		metrics:         NewMetricsCalculator(),
		detector:        NewOperatorOperandDetector(),
		functionMetrics: make(map[string]*FunctionHalsteadMetrics),
		nodeStack:       make([]*node.Node, 0),
	}
}

// OnEnter is called when entering a node during AST traversal.
func (v *Visitor) OnEnter(n *node.Node, _ int) {
	parent := v.currentNode()
	v.nodeStack = append(v.nodeStack, n)

	if v.isFunction(n) {
		v.pushContext(n)
	}

	if ctx := v.currentContext(); ctx != nil {
		v.processNode(ctx, n, parent)
	}
}

// OnExit is called when exiting a node during AST traversal.
func (v *Visitor) OnExit(n *node.Node, _ int) {
	if v.isFunction(n) {
		v.popContext()
	}

	if len(v.nodeStack) > 0 {
		v.nodeStack = v.nodeStack[:len(v.nodeStack)-1]
	}
}

// GetReport returns the collected analysis report.
func (v *Visitor) GetReport() analyze.Report {
	// Aggregate results similar to Analyzer.buildResult.
	analyzer := &Analyzer{
		metrics:   v.metrics,
		formatter: NewReportFormatter(),
	}

	fileMetrics := analyzer.calculateFileLevelMetrics(v.functionMetrics)
	detailedFunctionsTable := analyzer.buildDetailedFunctionsTable(v.functionMetrics)
	functionDetails := analyzer.buildFunctionDetails(v.functionMetrics)
	message := analyzer.formatter.GetHalsteadMessage(fileMetrics.Volume, fileMetrics.Difficulty, fileMetrics.Effort)

	return analyzer.buildResult(fileMetrics, detailedFunctionsTable, functionDetails, message)
}

func (v *Visitor) isFunction(n *node.Node) bool {
	return n.HasAnyType(node.UASTFunction, node.UASTMethod) ||
		n.HasAllRoles(node.RoleFunction, node.RoleDeclaration)
}

func (v *Visitor) pushContext(funcNode *node.Node) {
	name, _ := common.ExtractFunctionName(funcNode)
	if name == "" {
		name = "anonymous"
	}

	metrics := &FunctionHalsteadMetrics{
		Name:      name,
		Operators: make(map[string]int),
		Operands:  make(map[string]int),
	}

	ctx := &halsteadContext{
		functionNode: funcNode,
		metrics:      metrics,
	}
	v.contexts = append(v.contexts, ctx)
}

func (v *Visitor) popContext() {
	if len(v.contexts) == 0 {
		return
	}

	ctx := v.contexts[len(v.contexts)-1]
	v.contexts = v.contexts[:len(v.contexts)-1]

	// Populate counts from maps.
	ctx.metrics.DistinctOperators = len(ctx.metrics.Operators)
	ctx.metrics.DistinctOperands = len(ctx.metrics.Operands)
	ctx.metrics.TotalOperators = v.sumMap(ctx.metrics.Operators)
	ctx.metrics.TotalOperands = v.sumMap(ctx.metrics.Operands)

	// Finalize metrics calculation.
	v.metrics.CalculateHalsteadMetrics(ctx.metrics)

	// Store result.
	v.functionMetrics[ctx.metrics.Name] = ctx.metrics
}

func (v *Visitor) currentContext() *halsteadContext {
	if len(v.contexts) == 0 {
		return nil
	}

	return v.contexts[len(v.contexts)-1]
}

func (v *Visitor) currentNode() *node.Node {
	if len(v.nodeStack) == 0 {
		return nil
	}

	return v.nodeStack[len(v.nodeStack)-1]
}

func (v *Visitor) sumMap(m map[string]int) int {
	sum := 0
	for _, count := range m {
		sum += count
	}

	return sum
}

func (v *Visitor) processNode(ctx *halsteadContext, n, parent *node.Node) {
	if v.recordOperator(ctx, n) {
		return
	}

	v.recordOperand(ctx, n, parent)
}

func (v *Visitor) recordOperator(ctx *halsteadContext, target *node.Node) bool {
	if !v.detector.IsOperator(target) {
		return false
	}

	operator := v.detector.GetOperatorName(target)
	if operator == "" {
		return true
	}

	ctx.metrics.Operators[string(operator)]++

	return true
}

func (v *Visitor) recordOperand(ctx *halsteadContext, target, parent *node.Node) {
	if !v.detector.IsOperand(target) || !v.detector.shouldCountOperand(target, parent) {
		return
	}

	operand := v.detector.GetOperandName(target)
	if operand == "" {
		return
	}

	ctx.metrics.Operands[string(operand)]++
}
