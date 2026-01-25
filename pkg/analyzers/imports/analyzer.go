package imports

import (
	"encoding/json"
	"fmt"
	"io"
	"strings"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/renderer"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/terminal"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

type ImportsAnalyzer struct {
}

// NewImportsAnalyzer creates a new ImportsAnalyzer
func NewImportsAnalyzer() *ImportsAnalyzer {
	return &ImportsAnalyzer{}
}

func (a *ImportsAnalyzer) Name() string {
	return "imports"
}

func (a *ImportsAnalyzer) Flag() string {
	return "imports-analysis"
}

func (a *ImportsAnalyzer) Description() string {
	return "Extracts and analyzes import statements from code"
}

func (a *ImportsAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{}
}

func (a *ImportsAnalyzer) Configure(facts map[string]interface{}) error {
	return nil
}

func (a *ImportsAnalyzer) Thresholds() analyze.Thresholds {
	return nil
}

func (a *ImportsAnalyzer) CreateAggregator() analyze.ResultAggregator {
	return NewImportsAggregator()
}

func (a *ImportsAnalyzer) Analyze(root *node.Node) (analyze.Report, error) {
	imports := extractImportsFromUAST(root)
	return analyze.Report{
		"imports": imports,
		"count":   len(imports),
	}, nil
}

func (a *ImportsAnalyzer) FormatReport(report analyze.Report, w io.Writer) error {
	section := NewImportsReportSection(report)
	config := terminal.NewConfig()
	r := renderer.NewSectionRenderer(config.Width, false, config.NoColor)
	_, err := fmt.Fprint(w, r.Render(section))
	return err
}

func (a *ImportsAnalyzer) FormatReportJSON(report analyze.Report, w io.Writer) error {
	return json.NewEncoder(w).Encode(report)
}

// extractImportsFromUAST extracts import strings from a UAST node tree
func extractImportsFromUAST(root *node.Node) []string {
	var imports []string
	seen := make(map[string]bool)

	// Traverse the UAST tree to find import nodes
	root.VisitPreOrder(func(n *node.Node) {
		// Look for nodes with Import type or Import role
		if n.Type == node.UASTImport || n.HasAnyRole(node.RoleImport) {
			// Extract import path from token or children
			if importPath := extractImportPath(n); importPath != "" {
				// Deduplicate imports
				if !seen[importPath] {
					imports = append(imports, importPath)
					seen[importPath] = true
				}
			}
		}
	})

	return imports
}

// extractImportPath extracts the import path from an import node
func extractImportPath(importNode *node.Node) string {
	// First try to get the import path from the token
	if importNode.Token != "" {
		return cleanImportPath(importNode.Token)
	}

	// For JavaScript imports, look for specific patterns in children
	if len(importNode.Children) > 0 {
		// Look for string literals that contain import paths
		for _, child := range importNode.Children {
			if child.Type == node.UASTLiteral && child.Token != "" {
				// This is likely a string literal containing the import path
				return cleanImportPath(child.Token)
			}
		}

		// Look for identifier nodes that might contain module names
		for _, child := range importNode.Children {
			if child.Type == node.UASTIdentifier && child.Token != "" {
				// This might be a module name
				return cleanImportPath(child.Token)
			}
		}

		// Recursively check children for import paths
		for _, child := range importNode.Children {
			if path := extractImportPath(child); path != "" {
				return path
			}
		}
	}

	return ""
}

// cleanImportPath cleans up an import path by removing quotes and extracting module names
func cleanImportPath(path string) string {
	// Remove surrounding quotes and trailing semicolons
	path = strings.Trim(path, `"';`)

	// Skip empty or invalid paths
	if path == "" || path == "{" || path == "}" {
		return ""
	}

	// Handle different import statement formats
	if strings.HasPrefix(path, "from ") {
		// Python: "from typing import List, Dict" -> "typing"
		parts := strings.Fields(path)
		if len(parts) >= 2 {
			return parts[1]
		}
	} else if strings.Contains(path, " from ") {
		// JavaScript: "React from 'react'" -> "react"
		parts := strings.Split(path, " from ")
		if len(parts) >= 2 {
			return strings.Trim(parts[1], `"'`)
		}
	} else if strings.HasPrefix(path, "import ") {
		// Python: "import os" -> "os"
		// JavaScript: "import './styles.css'" -> "./styles.css"
		parts := strings.Fields(path)
		if len(parts) >= 2 {
			return strings.Trim(parts[1], `"'`)
		}
	} else if strings.Contains(path, "import ") {
		// JavaScript: "import './styles.css'" -> "./styles.css" (fallback)
		parts := strings.Split(path, "import ")
		if len(parts) >= 2 {
			return strings.Trim(parts[1], `"'`)
		}
	} else if strings.HasPrefix(path, "{") && strings.Contains(path, "}") {
		// JavaScript destructuring: "{ useState, useEffect }" -> skip this
		return ""
	}

	// For simple module names, just return as is
	return path
}
