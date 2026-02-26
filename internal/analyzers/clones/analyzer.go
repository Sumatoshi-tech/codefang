package clones

import (
	"encoding/json"
	"fmt"
	"io"

	"gopkg.in/yaml.v3"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/renderer"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/reportutil"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/terminal"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/lsh"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/minhash"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// Analysis configuration constants.
const (
	// numHashes is the number of MinHash hash functions per signature.
	numHashes = 128

	// numBands is the number of LSH bands.
	numBands = 16

	// numRows is the number of rows per LSH band.
	numRows = 8

	// analyzerName is the registered name of the clone detection analyzer.
	analyzerName = "clones"

	// analyzerFlag is the CLI flag for the clone detection analyzer.
	analyzerFlag = "clone-detection"

	// analyzerDescription is the human-readable description.
	analyzerDescription = "Detects duplicate and near-duplicate code using MinHash and LSH."

	// analyzerID is the full analyzer ID for registration.
	analyzerID = "static/clones"
)

// Threshold constants for the Thresholds() method.
const (
	thresholdCloneRatioYellow = 0.1
	thresholdCloneRatioRed    = 0.3
	thresholdClonePairsYellow = 5
	thresholdClonePairsRed    = 20
)

// Message constants.
const (
	msgNoClones     = "No code clones detected"
	msgLowClones    = "Low duplication - few clone pairs detected"
	msgModClones    = "Moderate duplication - consider refactoring clone pairs"
	msgHighClones   = "High duplication - significant refactoring recommended"
	msgNoFunctions  = "No functions found for clone analysis"
	msgEmptyAST     = "No AST provided"
	msgAnalysisOK   = "Clone analysis completed"
	pairCountLow    = 5
	pairCountMod    = 15
	maxTraversalVal = 10
)

// Analyzer provides clone detection analysis using MinHash and LSH.
type Analyzer struct {
	traverser *common.UASTTraverser
	shingler  *Shingler
}

// NewAnalyzer creates a new clone detection Analyzer.
func NewAnalyzer() *Analyzer {
	return &Analyzer{
		traverser: common.NewUASTTraverser(common.TraversalConfig{
			MaxDepth:    maxTraversalVal,
			IncludeRoot: true,
		}),
		shingler: NewShingler(defaultShingleSize),
	}
}

// Name returns the analyzer name.
func (a *Analyzer) Name() string {
	return analyzerName
}

// Flag returns the CLI flag for the analyzer.
func (a *Analyzer) Flag() string {
	return analyzerFlag
}

// Descriptor returns stable analyzer metadata.
func (a *Analyzer) Descriptor() analyze.Descriptor {
	return analyze.NewDescriptor(
		analyze.ModeStatic,
		a.Name(),
		analyzerDescription,
	)
}

// ListConfigurationOptions returns configuration options.
func (a *Analyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{}
}

// Configure configures the analyzer.
func (a *Analyzer) Configure(_ map[string]any) error {
	return nil
}

// Thresholds returns the color-coded thresholds for clone metrics.
func (a *Analyzer) Thresholds() analyze.Thresholds {
	return analyze.Thresholds{
		"clone_ratio": {
			"green":  0.0,
			"yellow": thresholdCloneRatioYellow,
			"red":    thresholdCloneRatioRed,
		},
		"total_clone_pairs": {
			"green":  0,
			"yellow": thresholdClonePairsYellow,
			"red":    thresholdClonePairsRed,
		},
	}
}

// CreateAggregator returns a new aggregator for clone analysis.
func (a *Analyzer) CreateAggregator() analyze.ResultAggregator {
	return NewAggregator()
}

// CreateVisitor creates a new visitor for single-pass traversal optimization.
func (a *Analyzer) CreateVisitor() analyze.AnalysisVisitor {
	return NewVisitor()
}

// CreateReportSection creates a ReportSection from report data.
func (a *Analyzer) CreateReportSection(report analyze.Report) analyze.ReportSection {
	return NewReportSection(report)
}

// Analyze performs clone detection on the given UAST.
func (a *Analyzer) Analyze(root *node.Node) (analyze.Report, error) {
	if root == nil {
		return buildEmptyReport(msgEmptyAST), nil
	}

	functions := a.findFunctions(root)
	if len(functions) == 0 {
		return buildEmptyReport(msgNoFunctions), nil
	}

	pairs := a.detectClones(functions)

	return a.buildReport(len(functions), pairs), nil
}

// findFunctions finds all function and method nodes in the UAST.
func (a *Analyzer) findFunctions(root *node.Node) []*node.Node {
	typeNodes := a.traverser.FindNodesByType(root, []string{node.UASTFunction, node.UASTMethod})
	roleNodes := a.traverser.FindNodesByRoles(root, []string{node.RoleFunction})

	seen := make(map[*node.Node]bool)

	var functions []*node.Node

	for _, n := range typeNodes {
		if !seen[n] && isFunctionNode(n) {
			seen[n] = true

			functions = append(functions, n)
		}
	}

	for _, n := range roleNodes {
		if !seen[n] && isFunctionNode(n) {
			seen[n] = true

			functions = append(functions, n)
		}
	}

	return functions
}

// isFunctionNode checks if a node represents a function.
func isFunctionNode(n *node.Node) bool {
	if n == nil {
		return false
	}

	return n.HasAnyType(node.UASTFunction, node.UASTMethod) ||
		n.HasAllRoles(node.RoleFunction, node.RoleDeclaration)
}

// funcEntry holds a function's name and MinHash signature for clone detection.
type funcEntry struct {
	name string
	sig  *minhash.Signature
}

// detectClones builds MinHash signatures and uses LSH to find clone pairs.
func (a *Analyzer) detectClones(functions []*node.Node) []ClonePair {
	entries := a.buildSignatures(functions)
	if len(entries) == 0 {
		return nil
	}

	idx, err := lsh.New(numBands, numRows)
	if err != nil {
		return nil
	}

	for _, entry := range entries {
		insertErr := idx.Insert(entry.name, entry.sig)
		if insertErr != nil {
			continue
		}
	}

	return findClonePairs(entries, idx)
}

// buildSignatures computes MinHash signatures for all functions.
func (a *Analyzer) buildSignatures(functions []*node.Node) []funcEntry {
	entries := make([]funcEntry, 0, len(functions))

	for _, fn := range functions {
		shingles := a.shingler.ExtractShingles(fn)
		if len(shingles) == 0 {
			continue
		}

		sig, err := minhash.New(numHashes)
		if err != nil {
			continue
		}

		for _, shingle := range shingles {
			sig.Add(shingle)
		}

		name := extractFuncName(fn)

		entries = append(entries, funcEntry{
			name: name,
			sig:  sig,
		})
	}

	return entries
}

// extractFuncName extracts the function name from a node.
func extractFuncName(fn *node.Node) string {
	if name, ok := common.ExtractFunctionName(fn); ok && name != "" {
		return name
	}

	if fn.Token != "" {
		return fn.Token
	}

	return string(fn.Type)
}

// buildReport constructs the analysis report.
func (a *Analyzer) buildReport(totalFunctions int, pairs []ClonePair) analyze.Report {
	cloneRatio := computeCloneRatio(len(pairs), totalFunctions)
	message := cloneMessage(len(pairs))

	pairsForReport := make([]map[string]any, 0, len(pairs))

	for _, p := range pairs {
		pairsForReport = append(pairsForReport, map[string]any{
			"func_a":     p.FuncA,
			"func_b":     p.FuncB,
			"similarity": p.Similarity,
			"clone_type": p.CloneType,
		})
	}

	return analyze.Report{
		keyAnalyzerName:    analyzerName,
		keyTotalFunctions:  totalFunctions,
		keyTotalClonePairs: len(pairs),
		keyCloneRatio:      cloneRatio,
		keyClonePairs:      pairsForReport,
		keyMessage:         message,
	}
}

// buildEmptyReport creates an empty report with the given message.
func buildEmptyReport(message string) analyze.Report {
	return common.NewResultBuilder().BuildCustomEmptyResult(map[string]any{
		keyAnalyzerName:    analyzerName,
		keyTotalFunctions:  0,
		keyTotalClonePairs: 0,
		keyCloneRatio:      0.0,
		keyMessage:         message,
	})
}

// computeCloneRatio calculates the ratio of clone pairs to total functions.
func computeCloneRatio(pairCount, totalFunctions int) float64 {
	if totalFunctions == 0 {
		return 0.0
	}

	return float64(pairCount) / float64(totalFunctions)
}

// cloneMessage returns a human-readable message based on clone pair count.
func cloneMessage(pairCount int) string {
	if pairCount == 0 {
		return msgNoClones
	}

	if pairCount <= pairCountLow {
		return msgLowClones
	}

	if pairCount <= pairCountMod {
		return msgModClones
	}

	return msgHighClones
}

// FormatReport formats clone analysis results as human-readable text.
func (a *Analyzer) FormatReport(report analyze.Report, w io.Writer) error {
	section := NewReportSection(report)
	config := terminal.NewConfig()
	r := renderer.NewSectionRenderer(config.Width, false, config.NoColor)

	_, err := fmt.Fprint(w, r.Render(section))
	if err != nil {
		return fmt.Errorf("formatreport: %w", err)
	}

	return nil
}

// FormatReportJSON formats clone analysis results as JSON.
func (a *Analyzer) FormatReportJSON(report analyze.Report, w io.Writer) error {
	metrics := computeMetricsFromReport(report)

	jsonData, err := json.MarshalIndent(metrics, "", "  ")
	if err != nil {
		return fmt.Errorf("formatreportjson: %w", err)
	}

	_, err = fmt.Fprint(w, string(jsonData))
	if err != nil {
		return fmt.Errorf("formatreportjson: %w", err)
	}

	return nil
}

// FormatReportYAML formats clone analysis results as YAML.
func (a *Analyzer) FormatReportYAML(report analyze.Report, w io.Writer) error {
	metrics := computeMetricsFromReport(report)

	data, err := yaml.Marshal(metrics)
	if err != nil {
		return fmt.Errorf("formatreportyaml: %w", err)
	}

	_, err = w.Write(data)
	if err != nil {
		return fmt.Errorf("formatreportyaml: %w", err)
	}

	return nil
}

// FormatReportBinary formats clone analysis results as binary envelope.
func (a *Analyzer) FormatReportBinary(report analyze.Report, w io.Writer) error {
	metrics := computeMetricsFromReport(report)

	err := reportutil.EncodeBinaryEnvelope(metrics, w)
	if err != nil {
		return fmt.Errorf("formatreportbinary: %w", err)
	}

	return nil
}

// FormatReportPlot formats clone analysis results as HTML plot.
func (a *Analyzer) FormatReportPlot(report analyze.Report, w io.Writer) error {
	sections, err := a.generatePlotSections(report)
	if err != nil {
		return fmt.Errorf("formatreportplot: %w", err)
	}

	return renderPlotSections(sections, w)
}
