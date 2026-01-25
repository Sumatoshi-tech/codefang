package main

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"strconv"
	"strings"

	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/spec"
	"github.com/fatih/color"
	"github.com/spf13/cobra"
	"github.com/xeipuuv/gojsonschema"
)

func validateCmd() *cobra.Command {
	var schemaPath string
	var colorize, nocolor bool

	cmd := &cobra.Command{
		Use:   "validate <file.json|->",
		Short: "Validate a UAST JSON file against the UAST schema",
		Long: `Validate a UAST JSON file against the canonical UAST schema.

Examples:
  uast validate mytree.json
  uast validate - < mytree.json
  uast validate --schema custom-schema.json mytree.json
`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return runValidate(args[0], schemaPath, false, colorize, nocolor)
		},
	}

	cmd.Flags().StringVar(&schemaPath, "schema", "pkg/uast/spec/uast-schema.json", "path to UAST JSON schema")
	cmd.Flags().BoolVar(&colorize, "color", false, "force colored output")
	cmd.Flags().BoolVar(&nocolor, "no-color", false, "disable colored output")

	return cmd
}

func runValidate(inputPath, schemaPath string, quiet, colorize, nocolor bool) error {
	// Color setup
	if nocolor {
		color.NoColor = true
	} else if colorize {
		color.NoColor = false
	}

	var inputReader io.Reader
	var inputLabel string
	if inputPath == "-" {
		inputReader = os.Stdin
		inputLabel = "stdin"
	} else {
		f, err := os.Open(inputPath)
		if err != nil {
			fmt.Fprintf(os.Stderr, "✖ Failed to open input: %v\n", err)
			os.Exit(2)
		}
		defer f.Close()
		inputReader = f
		inputLabel = inputPath
	}

	var inputData any
	dec := json.NewDecoder(inputReader)
	dec.UseNumber()
	if err := dec.Decode(&inputData); err != nil {
		fmt.Fprintf(os.Stderr, "✖ Invalid JSON in %s: %v\n", inputLabel, err)
		os.Exit(2)
	}

	var schemaLoader gojsonschema.JSONLoader
	if schemaPath == "" || schemaPath == "pkg/uast/spec/uast-schema.json" {
		schemaBytes, err := spec.UASTSchemaFS.ReadFile("uast-schema.json")
		if err != nil {
			fmt.Fprintf(os.Stderr, "✖ Failed to read embedded schema: %v\n", err)
			os.Exit(2)
		}
		schemaLoader = gojsonschema.NewBytesLoader(schemaBytes)
	} else {
		schemaBytes, err := os.ReadFile(schemaPath)
		if err != nil {
			fmt.Fprintf(os.Stderr, "✖ Failed to read schema file: %v\n", err)
			os.Exit(2)
		}
		schemaLoader = gojsonschema.NewBytesLoader(schemaBytes)
	}

	inputLoader := gojsonschema.NewGoLoader(inputData)
	result, err := gojsonschema.Validate(schemaLoader, inputLoader)
	if err != nil {
		fmt.Fprintf(os.Stderr, "✖ Schema validation error: %v\n", err)
		os.Exit(2)
	}

	if result.Valid() {
		if !quiet {
			color.New(color.FgGreen).Fprintf(os.Stdout, "✔ UAST is valid (%s)\n", inputLabel)
			color.New(color.FgGreen).Fprintf(os.Stdout, "  Compliance: 100%%\n")
		}
		return nil
	}

	// Calculate compliance percentage
	compliance := calculateCompliance(inputData, result.Errors())

	// Print validation results
	color.New(color.FgRed).Fprintf(os.Stdout, "✖ UAST validation failed (%s)\n", inputLabel)
	color.New(color.FgYellow).Fprintf(os.Stdout, "  Compliance: %d%%\n", compliance)

	fmt.Fprintf(os.Stdout, "\nErrors:\n")
	for _, verr := range result.Errors() {
		actualValue := getActualValue(inputData, verr.Field())
		if actualValue != "" {
			color.New(color.FgRed).Fprintf(os.Stdout, "  - %s: %s (got %q)\n", verr.Field(), verr.Description(), actualValue)
		} else {
			color.New(color.FgRed).Fprintf(os.Stdout, "  - %s: %s\n", verr.Field(), verr.Description())
		}
	}

	// Provide recommendations
	fmt.Fprintf(os.Stdout, "\nRecommendations:\n")
	provideRecommendations(result.Errors())

	os.Exit(1)
	return nil
}

func provideRecommendations(errors []gojsonschema.ResultError) {
	recommendations := make(map[string]string)

	for _, err := range errors {
		field := err.Field()
		description := err.Description()

		// Provide specific recommendations based on error type
		switch {
		case strings.Contains(description, "is not a valid NodeType"):
			recommendations["node_type"] = "Use canonical UAST node types like 'Function', 'Class', 'Identifier', etc."

		case strings.Contains(description, "is not a valid Role"):
			recommendations["role"] = "Use canonical UAST roles like 'Declaration', 'Name', 'Body', etc."

		case strings.Contains(description, "is required"):
			if strings.Contains(field, "type") {
				recommendations["required_type"] = "Every UAST node must have a 'type' field"
			}

		case strings.Contains(description, "start_line") || strings.Contains(description, "start_col"):
			recommendations["position"] = "Position fields should use snake_case: start_line, start_col, start_offset, end_line, end_col, end_offset"

		case strings.Contains(description, "additionalProperties"):
			recommendations["props"] = "Properties in 'props' field must be string key-value pairs"

		case strings.Contains(description, "children"):
			recommendations["children"] = "Children field must be an array of UAST nodes"

		case strings.Contains(description, "roles"):
			recommendations["roles"] = "Roles field must be an array of valid UAST roles"
		}
	}

	// Print unique recommendations
	seen := make(map[string]bool)
	for _, rec := range recommendations {
		if !seen[rec] {
			color.New(color.FgCyan).Fprintf(os.Stdout, "  • %s\n", rec)
			seen[rec] = true
		}
	}

	// General recommendations
	if len(errors) > 0 {
		fmt.Fprintf(os.Stdout, "\nGeneral tips:\n")
		color.New(color.FgCyan).Fprintf(os.Stdout, "  • Check the UAST specification at pkg/uast/spec/SPEC.md\n")
		color.New(color.FgCyan).Fprintf(os.Stdout, "  • Use the schema at pkg/uast/spec/uast-schema.json as reference\n")
		color.New(color.FgCyan).Fprintf(os.Stdout, "  • Ensure all required fields are present\n")
		color.New(color.FgCyan).Fprintf(os.Stdout, "  • Validate field types and values against the schema\n")
	}
}

func calculateCompliance(inputData any, errors []gojsonschema.ResultError) int {
	// Count total nodes in the UAST
	totalNodes := countNodes(inputData)
	if totalNodes == 0 {
		return 0
	}

	// Count valid nodes (nodes without errors)
	validNodes := totalNodes - len(errors)
	compliance := int(float64(validNodes) / float64(totalNodes) * 100)

	// Ensure compliance is between 0 and 100
	if compliance < 0 {
		compliance = 0
	} else if compliance > 100 {
		compliance = 100
	}

	return compliance
}

func countNodes(data any) int {
	count := 1 // Count this node

	switch v := data.(type) {
	case map[string]any:
		if children, ok := v["children"].([]any); ok {
			for _, child := range children {
				count += countNodes(child)
			}
		}
	case []any:
		for _, item := range v {
			count += countNodes(item)
		}
	}

	return count
}

func getActualValue(data any, fieldPath string) string {
	// Parse the field path (e.g., "children.0.roles.0")
	parts := strings.Split(fieldPath, ".")

	current := data
	for _, part := range parts {
		switch v := current.(type) {
		case map[string]any:
			if val, ok := v[part]; ok {
				current = val
			} else {
				return ""
			}
		case []any:
			if idx, err := strconv.Atoi(part); err == nil && idx >= 0 && idx < len(v) {
				current = v[idx]
			} else {
				return ""
			}
		default:
			return ""
		}
	}

	// Convert the final value to string
	switch v := current.(type) {
	case string:
		return v
	case float64:
		return strconv.FormatFloat(v, 'f', -1, 64)
	case int:
		return strconv.Itoa(v)
	case bool:
		return strconv.FormatBool(v)
	default:
		return fmt.Sprintf("%v", v)
	}
}
