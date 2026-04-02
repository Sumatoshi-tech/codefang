package clones

import (
	"encoding/json"
	"fmt"
	"io"
	"strings"

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

	// minFunctionNodes is the minimum number of AST nodes a function must have
	// to be included in clone detection. Functions below this threshold are
	// trivial (getters, setters, return-nil stubs) and produce false positives
	// because their minimal AST structure hashes identically regardless of purpose.
	// Empirical: getters ≈ 13-15 nodes, setters ≈ 19, real logic ≥ 25.
	minFunctionNodes = 20

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

// Configuration option keys for the clones analyzer.
const (
	ConfigClonesMaxClonePairs        = "Clones.MaxClonePairs"
	ConfigClonesNumHashes            = "Clones.NumHashes"
	ConfigClonesNumBands             = "Clones.NumBands"
	ConfigClonesNumRows              = "Clones.NumRows"
	ConfigClonesShingleSize          = "Clones.ShingleSize"
	ConfigClonesSimilarityType2      = "Clones.SimilarityType2"
	ConfigClonesSimilarityType3      = "Clones.SimilarityType3"
	ConfigClonesThresholdRatioYellow = "Clones.ThresholdRatioYellow"
	ConfigClonesThresholdRatioRed    = "Clones.ThresholdRatioRed"
	ConfigClonesThresholdPairsYellow = "Clones.ThresholdPairsYellow"
	ConfigClonesThresholdPairsRed    = "Clones.ThresholdPairsRed"
)

// Analyzer provides clone detection analysis using MinHash and LSH.
type Analyzer struct {
	traverser *common.UASTTraverser
	shingler  *Shingler

	cfgMaxClonePairs        int
	cfgNumHashes            int
	cfgNumBands             int
	cfgNumRows              int
	cfgSimilarityType2      float64
	cfgSimilarityType3      float64
	cfgThresholdRatioYellow float64
	cfgThresholdRatioRed    float64
	cfgThresholdPairsYellow int
	cfgThresholdPairsRed    int
}

// NewAnalyzer creates a new clone detection Analyzer.
func NewAnalyzer() *Analyzer {
	return &Analyzer{
		traverser: common.NewUASTTraverser(common.TraversalConfig{
			MaxDepth:    maxTraversalVal,
			IncludeRoot: true,
		}),
		shingler:                NewShingler(defaultShingleSize),
		cfgNumHashes:            numHashes,
		cfgNumBands:             numBands,
		cfgNumRows:              numRows,
		cfgSimilarityType2:      similarityType2,
		cfgSimilarityType3:      similarityType3,
		cfgThresholdRatioYellow: thresholdCloneRatioYellow,
		cfgThresholdRatioRed:    thresholdCloneRatioRed,
		cfgThresholdPairsYellow: thresholdClonePairsYellow,
		cfgThresholdPairsRed:    thresholdClonePairsRed,
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
func (a *Analyzer) Configure(facts map[string]any) error {
	if val, ok := facts[ConfigClonesMaxClonePairs].(int); ok {
		a.cfgMaxClonePairs = val
	}

	if val, ok := facts[ConfigClonesNumHashes].(int); ok {
		a.cfgNumHashes = val
	}

	if val, ok := facts[ConfigClonesNumBands].(int); ok {
		a.cfgNumBands = val
	}

	if val, ok := facts[ConfigClonesNumRows].(int); ok {
		a.cfgNumRows = val
	}

	if val, ok := facts[ConfigClonesShingleSize].(int); ok {
		a.shingler = NewShingler(val)
	}

	if val, ok := facts[ConfigClonesSimilarityType2].(float64); ok {
		a.cfgSimilarityType2 = val
	}

	if val, ok := facts[ConfigClonesSimilarityType3].(float64); ok {
		a.cfgSimilarityType3 = val
	}

	if val, ok := facts[ConfigClonesThresholdRatioYellow].(float64); ok {
		a.cfgThresholdRatioYellow = val
	}

	if val, ok := facts[ConfigClonesThresholdRatioRed].(float64); ok {
		a.cfgThresholdRatioRed = val
	}

	if val, ok := facts[ConfigClonesThresholdPairsYellow].(int); ok {
		a.cfgThresholdPairsYellow = val
	}

	if val, ok := facts[ConfigClonesThresholdPairsRed].(int); ok {
		a.cfgThresholdPairsRed = val
	}

	return nil
}

// Thresholds returns the color-coded thresholds for clone metrics.
func (a *Analyzer) Thresholds() analyze.Thresholds {
	return analyze.Thresholds{
		"clone_ratio": {
			"green":  0.0,
			"yellow": a.cfgThresholdRatioYellow,
			"red":    a.cfgThresholdRatioRed,
		},
		"total_clone_pairs": {
			"green":  0,
			"yellow": a.cfgThresholdPairsYellow,
			"red":    a.cfgThresholdPairsRed,
		},
	}
}

// CreateAggregator returns a new aggregator for clone analysis.
func (a *Analyzer) CreateAggregator() analyze.ResultAggregator {
	agg := NewAggregator()
	if a.cfgMaxClonePairs > 0 {
		agg.MaxClonePairs = a.cfgMaxClonePairs
	}

	agg.NumBands = a.cfgNumBands
	agg.NumRows = a.cfgNumRows
	agg.SimilarityType3 = a.cfgSimilarityType3

	return agg
}

// CreateVisitor creates a new visitor for single-pass traversal optimization.
func (a *Analyzer) CreateVisitor() analyze.AnalysisVisitor {
	v := NewVisitor()
	v.numHashes = a.cfgNumHashes
	v.shingler = a.shingler
	v.similarityType2 = a.cfgSimilarityType2
	v.similarityType3 = a.cfgSimilarityType3

	return v
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

	idx, err := lsh.New(a.cfgNumBands, a.cfgNumRows)
	if err != nil {
		return nil
	}

	for _, entry := range entries {
		insertErr := idx.Insert(entry.name, entry.sig)
		if insertErr != nil {
			continue
		}
	}

	// Per-file detection: no cap (single-file scope, bounded by function count).
	result := findClonePairs(entries, idx, 0, a.cfgSimilarityType3)

	return result.pairs
}

// buildSignatures computes MinHash signatures for all functions.
func (a *Analyzer) buildSignatures(functions []*node.Node) []funcEntry {
	entries := make([]funcEntry, 0, len(functions))

	for _, fn := range functions {
		if countNodes(fn) < minFunctionNodes {
			continue
		}

		shingles := a.shingler.ExtractShingles(fn)
		if len(shingles) == 0 {
			continue
		}

		sig, err := minhash.New(a.cfgNumHashes)
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

// extractFuncName extracts a unique function name from a node.
// For methods, qualifies with the receiver type (e.g., "Foo.DoWork") to avoid
// collisions in the LSH index when different types share the same method name.
func extractFuncName(fn *node.Node) string {
	name, ok := common.ExtractEntityName(fn)
	if !ok || name == "" {
		if fn.Token != "" {
			name = fn.Token
		} else {
			name = string(fn.Type)
		}
	}

	if fn.Type == node.UASTMethod {
		if recv := extractReceiverType(fn); recv != "" {
			return recv + "." + name
		}
	}

	return name
}

// extractReceiverType extracts the receiver type name from a Method node.
// The UAST represents the receiver as the first Parameter child with a token
// like "(f *Foo)" or "(f Foo)".
func extractReceiverType(fn *node.Node) string {
	for _, child := range fn.Children {
		if !child.HasAnyRole(node.RoleParameter) {
			continue
		}

		// The receiver parameter token contains the full "(name *Type)" text.
		tok := child.Token
		if tok == "" {
			continue
		}

		// Extract the type name: strip parens, pointer star, and variable name.
		// Strip parens, pointer star, and variable name to extract the type.
		tok = strings.TrimPrefix(tok, "(")
		tok = strings.TrimSuffix(tok, ")")
		tok = strings.TrimSpace(tok)

		// Split "f *Foo" into parts, take the last one (the type).
		parts := strings.Fields(tok)
		// Receiver has at least two parts: variable name and type.
		const minReceiverParts = 2
		if len(parts) < minReceiverParts {
			continue
		}

		typeName := parts[len(parts)-1]
		typeName = strings.TrimPrefix(typeName, "*")

		if typeName != "" {
			return typeName
		}
	}

	return ""
}

// countNodes returns the total number of nodes in a subtree.
func countNodes(n *node.Node) int {
	if n == nil {
		return 0
	}

	count := 1

	for _, child := range n.Children {
		count += countNodes(child)
	}

	return count
}

// buildReport constructs the analysis report.
func (a *Analyzer) buildReport(totalFunctions int, pairs []ClonePair) analyze.Report {
	cloneRatio := computeCloneRatio(countDistinctFuncs(pairs), totalFunctions)
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

// countDistinctFuncs returns the number of unique function names across all pairs.
func countDistinctFuncs(pairs []ClonePair) int {
	unique := make(map[string]struct{}, len(pairs))

	for idx := range pairs {
		unique[pairs[idx].FuncA] = struct{}{}
		unique[pairs[idx].FuncB] = struct{}{}
	}

	return len(unique)
}

// computeCloneRatio calculates the fraction of functions involved in at least one clone pair.
// Returns a value in [0, 1]: 0 means no duplication, 1 means every function has a clone.
func computeCloneRatio(clonedFuncs, totalFunctions int) float64 {
	if totalFunctions == 0 || clonedFuncs == 0 {
		return 0.0
	}

	return float64(clonedFuncs) / float64(totalFunctions)
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
