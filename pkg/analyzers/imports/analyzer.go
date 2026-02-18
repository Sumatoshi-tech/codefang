package imports

import (
	"encoding/json"
	"fmt"
	"io"
	"strings"

	"gopkg.in/yaml.v3"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/renderer"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/reportutil"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/terminal"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

const (
	lenArg2  = 2
	magic2   = 2
	magic2_1 = 2
	magic2_2 = 2
)

// Analyzer analyzes import statements in source code.
type Analyzer struct {
}

// NewAnalyzer creates a new Analyzer.
func NewAnalyzer() *Analyzer {
	return &Analyzer{}
}

// Name returns the name of the analyzer.
func (a *Analyzer) Name() string {
	return "imports"
}

// Flag returns the CLI flag for the analyzer.
func (a *Analyzer) Flag() string {
	return "imports-analysis"
}

// Description returns a human-readable description of the analyzer.
func (a *Analyzer) Description() string {
	return a.Descriptor().Description
}

// Descriptor returns stable analyzer metadata.
func (a *Analyzer) Descriptor() analyze.Descriptor {
	return analyze.NewDescriptor(
		analyze.ModeStatic,
		a.Name(),
		"Extracts and analyzes import statements from code",
	)
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (a *Analyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{}
}

// Configure sets up the analyzer with the provided facts.
func (a *Analyzer) Configure(_ map[string]any) error {
	return nil
}

// Thresholds returns the scoring thresholds for the analysis.
func (a *Analyzer) Thresholds() analyze.Thresholds {
	return nil
}

// CreateAggregator returns a new aggregator for collecting results.
func (a *Analyzer) CreateAggregator() analyze.ResultAggregator {
	return NewAggregator()
}

// Analyze runs the analysis on the given AST root node.
func (a *Analyzer) Analyze(root *node.Node) (analyze.Report, error) {
	imports := extractImportsFromUAST(root)

	return analyze.Report{
		"imports": imports,
		"count":   len(imports),
	}, nil
}

// FormatReport writes the formatted analysis report to the given writer.
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

// FormatReportJSON writes the analysis report in JSON format.
func (a *Analyzer) FormatReportJSON(report analyze.Report, w io.Writer) error {
	metrics, err := ComputeAllMetrics(report)
	if err != nil {
		metrics = &ComputedMetrics{}
	}

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

// FormatReportYAML writes the analysis report in YAML format.
func (a *Analyzer) FormatReportYAML(report analyze.Report, w io.Writer) error {
	metrics, err := ComputeAllMetrics(report)
	if err != nil {
		metrics = &ComputedMetrics{}
	}

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

// FormatReportBinary writes the report in binary envelope format.
func (a *Analyzer) FormatReportBinary(report analyze.Report, w io.Writer) error {
	metrics, err := ComputeAllMetrics(report)
	if err != nil {
		metrics = &ComputedMetrics{}
	}

	err = reportutil.EncodeBinaryEnvelope(metrics, w)
	if err != nil {
		return fmt.Errorf("formatreportbinary: %w", err)
	}

	return nil
}

// extractImportsFromUAST extracts import strings from a UAST node tree.
func extractImportsFromUAST(root *node.Node) []string {
	var imports []string

	seen := make(map[string]bool)

	// Traverse the UAST tree to find import nodes.
	root.VisitPreOrder(func(n *node.Node) {
		// Look for nodes with Import type or Import role.
		if n.Type == node.UASTImport || n.HasAnyRole(node.RoleImport) {
			// Extract import path from token or children.
			if importPath := extractImportPath(n); importPath != "" {
				// Deduplicate imports.
				if !seen[importPath] {
					imports = append(imports, importPath)
					seen[importPath] = true
				}
			}
		}
	})

	return imports
}

// extractImportPath extracts the import path from an import node.
func extractImportPath(importNode *node.Node) string {
	if importNode.Token != "" {
		return cleanImportPath(importNode.Token)
	}

	if len(importNode.Children) == 0 {
		return ""
	}

	return extractImportPathFromChildren(importNode.Children)
}

// extractImportPathFromChildren searches children for import paths by type priority.
func extractImportPathFromChildren(children []*node.Node) string {
	// Look for string literals that contain import paths.
	for _, child := range children {
		if child.Type == node.UASTLiteral && child.Token != "" {
			return cleanImportPath(child.Token)
		}
	}

	// Look for identifier nodes that might contain module names.
	for _, child := range children {
		if child.Type == node.UASTIdentifier && child.Token != "" {
			return cleanImportPath(child.Token)
		}
	}

	// Recursively check children for import paths.
	for _, child := range children {
		if path := extractImportPath(child); path != "" {
			return path
		}
	}

	return ""
}

// cleanImportPath cleans up an import path by removing quotes and extracting module names.
func cleanImportPath(path string) string {
	// Remove surrounding quotes and trailing semicolons.
	path = strings.Trim(path, `"';`)

	// Skip empty or invalid paths.
	if path == "" || path == "{" || path == "}" {
		return ""
	}

	if parsed := parseImportFormat(path); parsed != "" {
		return parsed
	}

	// For simple module names, just return as is.
	return path
}

// parseImportFormat attempts to extract a module name from various import statement formats.
func parseImportFormat(path string) string {
	if strings.HasPrefix(path, "from ") {
		// Python: "from typing import List, Dict" -> "typing".
		parts := strings.Fields(path)
		if len(parts) >= lenArg2 {
			return parts[1]
		}

		return ""
	}

	if strings.Contains(path, " from ") {
		// JavaScript: "React from 'react'" -> "react".
		parts := strings.Split(path, " from ")
		if len(parts) >= magic2 {
			return strings.Trim(parts[1], `"'`)
		}

		return ""
	}

	if strings.HasPrefix(path, "import ") {
		// Python: "import os" -> "os"
		// JavaScript: "import './styles.css'" -> "./styles.css".
		parts := strings.Fields(path)
		if len(parts) >= magic2_1 {
			return strings.Trim(parts[1], `"'`)
		}

		return ""
	}

	if strings.Contains(path, "import ") {
		// JavaScript: "import './styles.css'" -> "./styles.css" (fallback).
		parts := strings.Split(path, "import ")
		if len(parts) >= magic2_2 {
			return strings.Trim(parts[1], `"'`)
		}

		return ""
	}

	// JavaScript destructuring: "{ useState, useEffect }" -> skip this.
	return ""
}
