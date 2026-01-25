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

// HalsteadVisitor implements NodeVisitor for Halstead analysis
type HalsteadVisitor struct {
	contexts []*halsteadContext
	metrics  *MetricsCalculator
	detector *OperatorOperandDetector
	
	// Collected function metrics
	functionMetrics map[string]*FunctionHalsteadMetrics
}

// NewHalsteadVisitor creates a new HalsteadVisitor
func NewHalsteadVisitor() *HalsteadVisitor {
	return &HalsteadVisitor{
		contexts:        make([]*halsteadContext, 0),
		metrics:         NewMetricsCalculator(),
		detector:        NewOperatorOperandDetector(),
		functionMetrics: make(map[string]*FunctionHalsteadMetrics),
	}
}

func (v *HalsteadVisitor) OnEnter(n *node.Node, depth int) {
	if v.isFunction(n) {
		v.pushContext(n)
	}

	if ctx := v.currentContext(); ctx != nil {
		v.processNode(ctx, n)
	}
}

func (v *HalsteadVisitor) OnExit(n *node.Node, depth int) {
	if v.isFunction(n) {
		v.popContext()
	}
}

func (v *HalsteadVisitor) GetReport() analyze.Report {
	// Aggregate results similar to HalsteadAnalyzer.buildResult
	analyzer := &HalsteadAnalyzer{
		metrics:   v.metrics,
		formatter: NewReportFormatter(),
	}
	
	fileMetrics := analyzer.calculateFileLevelMetrics(v.functionMetrics)
	detailedFunctionsTable := analyzer.buildDetailedFunctionsTable(v.functionMetrics)
	functionDetails := analyzer.buildFunctionDetails(v.functionMetrics)
	message := analyzer.formatter.GetHalsteadMessage(fileMetrics.Volume, fileMetrics.Difficulty, fileMetrics.Effort)

	return analyzer.buildResult(fileMetrics, detailedFunctionsTable, functionDetails, message)
}

func (v *HalsteadVisitor) isFunction(n *node.Node) bool {
	return n.HasAnyType(node.UASTFunction, node.UASTMethod) ||
		n.HasAllRoles(node.RoleFunction, node.RoleDeclaration)
}

func (v *HalsteadVisitor) pushContext(n *node.Node) {
	name, _ := common.ExtractFunctionName(n)
	if name == "" {
		name = "anonymous"
	}

	metrics := &FunctionHalsteadMetrics{
		Name:      name,
		Operators: make(map[string]int),
		Operands:  make(map[string]int),
	}

	ctx := &halsteadContext{
		functionNode: n,
		metrics:      metrics,
	}
	v.contexts = append(v.contexts, ctx)
}

func (v *HalsteadVisitor) popContext() {
	if len(v.contexts) == 0 {
		return
	}
	ctx := v.contexts[len(v.contexts)-1]
	v.contexts = v.contexts[:len(v.contexts)-1]

    // Populate counts from maps
    ctx.metrics.DistinctOperators = len(ctx.metrics.Operators)
    ctx.metrics.DistinctOperands = len(ctx.metrics.Operands)
    ctx.metrics.TotalOperators = v.sumMap(ctx.metrics.Operators)
    ctx.metrics.TotalOperands = v.sumMap(ctx.metrics.Operands)

	// Finalize metrics calculation
	v.metrics.CalculateHalsteadMetrics(ctx.metrics)
	
	// Store result
	v.functionMetrics[ctx.metrics.Name] = ctx.metrics
}

func (v *HalsteadVisitor) currentContext() *halsteadContext {
	if len(v.contexts) == 0 {
		return nil
	}
	return v.contexts[len(v.contexts)-1]
}

func (v *HalsteadVisitor) sumMap(m map[string]int) int {
	sum := 0
	for _, count := range m {
		sum += count
	}
	return sum
}

func (v *HalsteadVisitor) processNode(ctx *halsteadContext, n *node.Node) {
	if v.detector.IsOperator(n) {
		operator := v.detector.GetOperatorName(n)
		ctx.metrics.Operators[string(operator)]++
	} else if v.detector.IsOperand(n) {
		operand := v.detector.GetOperandName(n)
		ctx.metrics.Operands[string(operand)]++
	}
}
