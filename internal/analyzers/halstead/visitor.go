package halstead

import (
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/cms"
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
	contexts        *common.ContextStack[*halsteadContext]
	nodeStack       *common.ContextStack[*node.Node]
}

// NewVisitor creates a new Visitor.
func NewVisitor() *Visitor {
	return &Visitor{
		contexts:        common.NewContextStack[*halsteadContext](),
		metrics:         NewMetricsCalculator(),
		detector:        NewOperatorOperandDetector(),
		functionMetrics: make(map[string]*FunctionHalsteadMetrics),
		nodeStack:       common.NewContextStack[*node.Node](),
	}
}

// OnEnter is called when entering a node during AST traversal.
func (v *Visitor) OnEnter(n *node.Node, _ int) {
	parent := v.currentNode()
	v.nodeStack.Push(n)

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

	v.nodeStack.Pop()
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
	name, _ := common.ExtractEntityName(funcNode)
	if name == "" {
		name = "anonymous"
	}

	metrics := &FunctionHalsteadMetrics{
		Name:      name,
		Operators: make(map[string]int),
		Operands:  make(map[string]int),
	}

	// Initialize CMS sketches for streaming total counting.
	opSketch, err := cms.New(cmsEpsilon, cmsDelta)
	if err == nil {
		metrics.OperatorSketch = opSketch
	}

	opndSketch, err := cms.New(cmsEpsilon, cmsDelta)
	if err == nil {
		metrics.OperandSketch = opndSketch
	}

	ctx := &halsteadContext{
		functionNode: funcNode,
		metrics:      metrics,
	}
	v.contexts.Push(ctx)
}

func (v *Visitor) popContext() {
	ctx, ok := v.contexts.Pop()
	if !ok {
		return
	}

	// Populate distinct counts from maps (always exact).
	ctx.metrics.DistinctOperators = len(ctx.metrics.Operators)
	ctx.metrics.DistinctOperands = len(ctx.metrics.Operands)
	ctx.metrics.TotalOperators = v.sumMap(ctx.metrics.Operators)
	ctx.metrics.TotalOperands = v.sumMap(ctx.metrics.Operands)

	// Use CMS for estimated totals if function exceeds token threshold.
	totalTokens := ctx.metrics.TotalOperators + ctx.metrics.TotalOperands
	if totalTokens >= cmsTokenThreshold && ctx.metrics.OperatorSketch != nil && ctx.metrics.OperandSketch != nil {
		ctx.metrics.EstimatedTotalOperators = ctx.metrics.OperatorSketch.TotalCount()
		ctx.metrics.EstimatedTotalOperands = ctx.metrics.OperandSketch.TotalCount()
	} else {
		// Below threshold: nil out sketches to signal exact-only path.
		ctx.metrics.OperatorSketch = nil
		ctx.metrics.OperandSketch = nil
	}

	// Finalize metrics calculation.
	v.metrics.CalculateHalsteadMetrics(ctx.metrics)

	// Store result.
	v.functionMetrics[ctx.metrics.Name] = ctx.metrics
}

func (v *Visitor) currentContext() *halsteadContext {
	ctx, ok := v.contexts.Current()
	if !ok {
		return nil
	}

	return ctx
}

func (v *Visitor) currentNode() *node.Node {
	n, ok := v.nodeStack.Current()
	if !ok {
		return nil
	}

	return n
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

	opStr := string(operator)
	ctx.metrics.Operators[opStr]++

	if ctx.metrics.OperatorSketch != nil {
		ctx.metrics.OperatorSketch.Add([]byte(opStr), 1)
	}

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

	opndStr := string(operand)
	ctx.metrics.Operands[opndStr]++

	if ctx.metrics.OperandSketch != nil {
		ctx.metrics.OperandSketch.Add([]byte(opndStr), 1)
	}
}
