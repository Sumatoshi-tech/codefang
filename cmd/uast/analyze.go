package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"html/template"
	"io"
	"os"
	"path/filepath"
	"runtime"
	"sort"
	"sync"

	"github.com/spf13/cobra"

	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// Sentinel errors for the analyze command.
var (
	ErrNoFilesSpecified  = errors.New("no files specified for analysis")
	ErrUnsupportedAnaFmt = errors.New("unsupported format")
)

// Initial stack capacity for tree traversal, sized for typical ASTs.
const analyzeStackInitCap = 64

// Initial map capacities for type and role frequency tables.
const (
	typeMapInitCap = 32
	roleMapInitCap = 16
)

// analysisResult holds typed analysis metrics for a single file.
type analysisResult struct {
	File           string         `json:"file"`
	TotalNodes     int            `json:"total_nodes"`
	LeafNodes      int            `json:"leaf_nodes"`
	LeafRatio      float64        `json:"leaf_ratio"`
	MaxDepth       int            `json:"max_depth"`
	AvgDepth       float64        `json:"avg_depth"`
	MaxChildren    int            `json:"max_children"`
	AvgBranching   float64        `json:"avg_branching"`
	TypeDiversity  int            `json:"type_diversity"`
	Types          map[string]int `json:"types"`
	Roles          map[string]int `json:"roles"`
	RoleCoverage   float64        `json:"role_coverage"`
	PosCoverage    float64        `json:"pos_coverage"`
	SyntheticNodes int            `json:"synthetic_nodes"`
}

func analyzeCmd() *cobra.Command {
	var output, format string

	cmd := &cobra.Command{
		Use:   "analyze [files...]",
		Short: "Analyze UAST tree structure and composition",
		Long: `Analyze the structural properties of parsed UAST trees.

Reports tree shape (depth, breadth, node count), node type distribution,
role coverage, and structural metrics. For code-quality metrics
(complexity, Halstead, cohesion), use codefang run -a 'static/*'.

Examples:
  uast analyze main.go                  # Analyze single file
  uast analyze *.go                     # Analyze all Go files
  uast analyze -f json *.go             # JSON output
  uast analyze -o report.html *.go      # HTML report`,
		RunE: func(_ *cobra.Command, args []string) error {
			return runAnalyze(args, output, format)
		},
	}

	cmd.Flags().StringVarP(&output, "output", "o", "", "output file (default: stdout)")
	cmd.Flags().StringVarP(&format, "format", "f", "text", "output format (text, json, html)")

	return cmd
}

// indexedFile pairs a file path with its position in the input list.
type indexedFile struct {
	index int
	path  string
}

func runAnalyze(files []string, output, format string) error {
	if len(files) == 0 {
		return ErrNoFilesSpecified
	}

	for i, f := range files {
		files[i] = filepath.Clean(f)
	}

	// Single parser instance shared across all files (thread-safe via
	// lazy-init parsers with sync.Once).
	parser, err := uast.NewParser()
	if err != nil {
		return fmt.Errorf("failed to initialize parser: %w", err)
	}

	supported := filterSupported(parser, files)
	if len(supported) == 0 {
		return outputAnalysis(nil, output, format)
	}

	// For a single file, skip goroutine overhead.
	if len(supported) == 1 {
		return runAnalyzeSingle(parser, supported[0].path, output, format)
	}

	results, err := runAnalyzeParallel(parser, supported)
	if err != nil {
		return err
	}

	return outputAnalysis(results, output, format)
}

// filterSupported returns only files the parser can handle, preserving order.
func filterSupported(parser *uast.Parser, files []string) []indexedFile {
	supported := make([]indexedFile, 0, len(files))

	for i, file := range files {
		if !parser.IsSupported(file) {
			fmt.Fprintf(os.Stderr, "Warning: Skipping unsupported file %s\n", file)

			continue
		}

		supported = append(supported, indexedFile{index: i, path: file})
	}

	return supported
}

// runAnalyzeParallel fans analysis out across NumCPU workers.
func runAnalyzeParallel(parser *uast.Parser, supported []indexedFile) ([]analysisResult, error) {
	workers := min(runtime.NumCPU(), len(supported))
	allResults := make([]analysisResult, len(supported))
	work := make(chan indexedFile, len(supported))

	var (
		wg       sync.WaitGroup
		errOnce  sync.Once
		firstErr error
	)

	for range workers {
		wg.Go(func() {
			for item := range work {
				pErr := analyzeFile(parser, item, allResults)
				if pErr != nil {
					errOnce.Do(func() { firstErr = pErr })

					return
				}
			}
		})
	}

	// Use contiguous indices into allResults.
	for i, item := range supported {
		work <- indexedFile{index: i, path: item.path}
	}

	close(work)
	wg.Wait()

	if firstErr != nil {
		return nil, firstErr
	}

	return allResults, nil
}

// analyzeFile parses and analyzes a single file, storing the result in results[item.index].
func analyzeFile(parser *uast.Parser, item indexedFile, results []analysisResult) error {
	code, err := os.ReadFile(item.path)
	if err != nil {
		return fmt.Errorf("failed to read file %s: %w", item.path, err)
	}

	parsedNode, err := parser.Parse(context.Background(), item.path, code)
	if err != nil {
		return fmt.Errorf("parse error in %s: %w", item.path, err)
	}

	results[item.index] = analyzeNode(parsedNode, item.path)

	return nil
}

// runAnalyzeSingle is the fast path for a single file (no goroutine overhead).
func runAnalyzeSingle(parser *uast.Parser, file, output, format string) error {
	code, err := os.ReadFile(file)
	if err != nil {
		return fmt.Errorf("failed to read file %s: %w", file, err)
	}

	parsedNode, err := parser.Parse(context.Background(), file, code)
	if err != nil {
		return fmt.Errorf("parse error in %s: %w", file, err)
	}

	analysis := analyzeNode(parsedNode, file)

	return outputAnalysis([]analysisResult{analysis}, output, format)
}

// treeStats accumulates structural metrics during a single tree walk.
type treeStats struct {
	totalNodes     int
	leafNodes      int
	maxDepth       int
	totalDepth     int // Sum of all node depths, for computing average.
	types          map[string]int
	roles          map[string]int
	nodesWithRoles int
	nodesWithPos   int
	syntheticNodes int
	maxChildren    int
	totalChildren  int // Sum of children counts for non-leaf nodes.
	innerNodes     int // Nodes with at least one child.
}

// analyzeFrame pairs a node with its depth for stack-based traversal.
type analyzeFrame struct {
	n     *node.Node
	depth int
}

// analyzeNode produces structural analysis data for a parsed UAST node.
func analyzeNode(root *node.Node, filename string) analysisResult {
	stats := collectTreeStats(root)

	return buildResult(filename, &stats)
}

// collectTreeStats performs iterative depth-first traversal and accumulates metrics.
func collectTreeStats(root *node.Node) treeStats {
	stats := treeStats{
		types: make(map[string]int, typeMapInitCap),
		roles: make(map[string]int, roleMapInitCap),
	}

	stack := make([]analyzeFrame, 1, analyzeStackInitCap)
	stack[0] = analyzeFrame{n: root, depth: 0}

	for len(stack) > 0 {
		last := len(stack) - 1
		f := stack[last]
		stack = stack[:last]

		stats.recordNode(f.n, f.depth)
		stack = pushChildren(stack, f.n, f.depth)
	}

	return stats
}

// recordNode updates stats for a single visited node.
func (s *treeStats) recordNode(n *node.Node, depth int) {
	s.totalNodes++
	s.totalDepth += depth

	if depth > s.maxDepth {
		s.maxDepth = depth
	}

	if nodeType := string(n.Type); nodeType != "" {
		s.types[nodeType]++
	}

	if n.Type == node.UASTSynthetic {
		s.syntheticNodes++
	}

	for _, r := range n.Roles {
		s.roles[string(r)]++
	}

	if len(n.Roles) > 0 {
		s.nodesWithRoles++
	}

	if n.Pos != nil {
		s.nodesWithPos++
	}

	s.recordChildStats(len(n.Children))
}

// recordChildStats updates leaf/inner/branching metrics.
func (s *treeStats) recordChildStats(childCount int) {
	if childCount == 0 {
		s.leafNodes++

		return
	}

	s.innerNodes++
	s.totalChildren += childCount

	if childCount > s.maxChildren {
		s.maxChildren = childCount
	}
}

// pushChildren appends children in reverse order for depth-first traversal.
func pushChildren(stack []analyzeFrame, n *node.Node, depth int) []analyzeFrame {
	childCount := len(n.Children)
	if childCount == 0 {
		return stack
	}

	if needed := len(stack) + childCount; needed > cap(stack) {
		grown := make([]analyzeFrame, len(stack), needed+analyzeStackInitCap)
		copy(grown, stack)
		stack = grown
	}

	for i := childCount - 1; i >= 0; i-- {
		stack = append(stack, analyzeFrame{n: n.Children[i], depth: depth + 1})
	}

	return stack
}

// buildResult constructs the typed analysis result from accumulated stats.
func buildResult(filename string, stats *treeStats) analysisResult {
	totalF := float64(stats.totalNodes)

	return analysisResult{
		File:           filename,
		TotalNodes:     stats.totalNodes,
		LeafNodes:      stats.leafNodes,
		LeafRatio:      safeDiv(float64(stats.leafNodes), totalF),
		MaxDepth:       stats.maxDepth,
		AvgDepth:       safeDiv(float64(stats.totalDepth), totalF),
		MaxChildren:    stats.maxChildren,
		AvgBranching:   safeDiv(float64(stats.totalChildren), float64(stats.innerNodes)),
		TypeDiversity:  len(stats.types),
		Types:          stats.types,
		Roles:          stats.roles,
		RoleCoverage:   safeDiv(float64(stats.nodesWithRoles), totalF),
		PosCoverage:    safeDiv(float64(stats.nodesWithPos), totalF),
		SyntheticNodes: stats.syntheticNodes,
	}
}

// safeDiv returns numerator/denominator, or 0 if denominator is zero.
func safeDiv(numerator, denominator float64) float64 {
	if denominator == 0 {
		return 0
	}

	return numerator / denominator
}

func outputAnalysis(results []analysisResult, output, format string) error {
	var writer io.Writer = os.Stdout

	if output != "" {
		outputFile, err := os.Create(output)
		if err != nil {
			return fmt.Errorf("failed to create output file: %w", err)
		}
		defer outputFile.Close()

		writer = outputFile
	}

	switch format {
	case formatJSON:
		return outputAnalysisJSON(results, writer)
	case "text":
		outputAnalysisText(results, writer)

		return nil
	case "html":
		return generateHTMLReport(results, writer)
	default:
		return fmt.Errorf("%w: %s", ErrUnsupportedAnaFmt, format)
	}
}

func outputAnalysisJSON(results []analysisResult, writer io.Writer) error {
	enc := json.NewEncoder(writer)
	enc.SetIndent("", "  ")

	err := enc.Encode(results)
	if err != nil {
		return fmt.Errorf("failed to encode JSON: %w", err)
	}

	return nil
}

func outputAnalysisText(results []analysisResult, writer io.Writer) {
	for _, r := range results {
		fmt.Fprintf(writer, "File: %s\n", r.File)
		fmt.Fprintf(writer, "  Tree shape:\n")
		fmt.Fprintf(writer, "    Total nodes:    %d\n", r.TotalNodes)
		fmt.Fprintf(writer, "    Leaf nodes:     %d (%.0f%%)\n",
			r.LeafNodes, r.LeafRatio*coveragePercent)
		fmt.Fprintf(writer, "    Max depth:      %d\n", r.MaxDepth)
		fmt.Fprintf(writer, "    Avg depth:      %.1f\n", r.AvgDepth)
		fmt.Fprintf(writer, "    Max children:   %d\n", r.MaxChildren)
		fmt.Fprintf(writer, "    Avg branching:  %.1f\n", r.AvgBranching)

		fmt.Fprintf(writer, "  Coverage:\n")
		fmt.Fprintf(writer, "    Role coverage:  %.0f%%\n", r.RoleCoverage*coveragePercent)
		fmt.Fprintf(writer, "    Pos coverage:   %.0f%%\n", r.PosCoverage*coveragePercent)
		fmt.Fprintf(writer, "    Synthetic:      %d\n", r.SyntheticNodes)
		fmt.Fprintf(writer, "    Type diversity: %d\n", r.TypeDiversity)

		if len(r.Types) > 0 {
			fmt.Fprintf(writer, "  Node types:\n")
			printSortedMap(writer, r.Types)
		}

		if len(r.Roles) > 0 {
			fmt.Fprintf(writer, "  Roles:\n")
			printSortedMap(writer, r.Roles)
		}

		fmt.Fprintln(writer)
	}
}

// htmlReportTmpl is the compiled HTML template for the analysis report.
var htmlReportTmpl = template.Must(template.New("report").Funcs(template.FuncMap{
	"pct": func(v float64) float64 { return v * coveragePercent },
}).Parse(`<!DOCTYPE html>
<html>
<head>
<title>UAST Structure Report</title>
<style>
body{font-family:Arial,sans-serif;margin:20px;}
table{border-collapse:collapse;width:100%;margin-bottom:20px;}
th,td{border:1px solid #ddd;padding:8px;text-align:left;}
th{background-color:#f2f2f2;}
</style>
</head>
<body>
<h1>UAST Structure Report</h1>
<table>
<tr><th>File</th><th>Nodes</th><th>Depth</th><th>Branching</th><th>Types</th><th>Role%</th><th>Pos%</th></tr>
{{- range .}}
<tr>
<td>{{.File}}</td>
<td>{{.TotalNodes}}</td>
<td>{{.MaxDepth}} (avg {{printf "%.1f" .AvgDepth}})</td>
<td>max {{.MaxChildren}} (avg {{printf "%.1f" .AvgBranching}})</td>
<td>{{.TypeDiversity}}</td>
<td>{{printf "%.0f" (pct .RoleCoverage)}}%</td>
<td>{{printf "%.0f" (pct .PosCoverage)}}%</td>
</tr>
{{- end}}
</table>
</body>
</html>
`))

func generateHTMLReport(results []analysisResult, writer io.Writer) error {
	err := htmlReportTmpl.Execute(writer, results)
	if err != nil {
		return fmt.Errorf("render HTML report: %w", err)
	}

	return nil
}

// printSortedMap prints a map[string]int sorted by descending value.
func printSortedMap(writer io.Writer, m map[string]int) {
	type kv struct {
		key   string
		value int
	}

	sorted := make([]kv, 0, len(m))

	for k, v := range m {
		sorted = append(sorted, kv{k, v})
	}

	sort.Slice(sorted, func(i, j int) bool {
		return sorted[i].value > sorted[j].value
	})

	for _, item := range sorted {
		fmt.Fprintf(writer, "    %-20s %d\n", item.key, item.value)
	}
}
