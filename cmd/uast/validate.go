package main

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"strconv"
	"strings"

	"github.com/fatih/color"
	"github.com/spf13/cobra"
	"github.com/xeipuuv/gojsonschema"

	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/spec"
)

// complianceMax is the maximum compliance percentage.
const complianceMax = 100

// exitCodeValidationFailure is the exit code for validation failures.
const exitCodeValidationFailure = 2

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
		RunE: func(_ *cobra.Command, args []string) error {
			return runValidate(args[0], schemaPath, false, colorize, nocolor)
		},
	}

	cmd.Flags().StringVar(&schemaPath, "schema", "pkg/uast/spec/uast-schema.json", "path to UAST JSON schema")
	cmd.Flags().BoolVar(&colorize, "color", false, "force colored output")
	cmd.Flags().BoolVar(&nocolor, "no-color", false, "disable colored output")

	return cmd
}

func runValidate(inputPath, schemaPath string, quiet, colorize, nocolor bool) error {
	// Color setup.
	if nocolor {
		color.NoColor = true //nolint:reassign // intentional override of library global
	} else if colorize {
		color.NoColor = false //nolint:reassign // intentional override of library global
	}

	inputReader, inputLabel := loadInput(inputPath)

	var inputData any

	dec := json.NewDecoder(inputReader)
	dec.UseNumber()

	decodeErr := dec.Decode(&inputData)
	if decodeErr != nil {
		fmt.Fprintf(os.Stderr, "Invalid JSON in %s: %v\n", inputLabel, decodeErr)
		os.Exit(exitCodeValidationFailure)
	}

	schemaLoader := loadSchema(schemaPath)

	inputLoader := gojsonschema.NewGoLoader(inputData)

	result, err := gojsonschema.Validate(schemaLoader, inputLoader)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Schema validation error: %v\n", err)
		os.Exit(exitCodeValidationFailure)
	}

	if result.Valid() {
		if !quiet {
			color.New(color.FgGreen).Fprintf(os.Stdout, "UAST is valid (%s)\n", inputLabel)
			color.New(color.FgGreen).Fprintf(os.Stdout, "  Compliance: 100%%\n")
		}

		return nil
	}

	// Calculate compliance percentage.
	compliance := calculateCompliance(inputData, result.Errors())

	// Print validation results.
	color.New(color.FgRed).Fprintf(os.Stdout, "UAST validation failed (%s)\n", inputLabel)
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

	// Provide recommendations.
	fmt.Fprintf(os.Stdout, "\nRecommendations:\n")
	provideRecommendations(result.Errors())

	os.Exit(1)

	return nil
}

//nolint:nonamedreturns // named returns needed for gocritic unnamedResult
func loadInput(inputPath string) (inputReader io.Reader, inputLabel string) {
	if inputPath == "-" {
		return os.Stdin, "stdin"
	}

	inputFile, err := os.Open(inputPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Failed to open input: %v\n", err)
		os.Exit(exitCodeValidationFailure)
	}

	return inputFile, inputPath
}

func loadSchema(schemaPath string) gojsonschema.JSONLoader {
	if schemaPath == "" || schemaPath == "pkg/uast/spec/uast-schema.json" {
		schemaBytes, err := spec.UASTSchemaFS.ReadFile("uast-schema.json")
		if err != nil {
			fmt.Fprintf(os.Stderr, "Failed to read embedded schema: %v\n", err)
			os.Exit(exitCodeValidationFailure)
		}

		return gojsonschema.NewBytesLoader(schemaBytes)
	}

	schemaBytes, err := os.ReadFile(schemaPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Failed to read schema file: %v\n", err)
		os.Exit(exitCodeValidationFailure)
	}

	return gojsonschema.NewBytesLoader(schemaBytes)
}

func provideRecommendations(validationErrors []gojsonschema.ResultError) {
	recommendations := make(map[string]string)

	for _, validationErr := range validationErrors {
		field := validationErr.Field()
		description := validationErr.Description()

		classifyRecommendation(recommendations, field, description)
	}

	// Print unique recommendations.
	seen := make(map[string]bool)

	for _, rec := range recommendations {
		if !seen[rec] {
			color.New(color.FgCyan).Fprintf(os.Stdout, "  - %s\n", rec)
			seen[rec] = true
		}
	}

	// General recommendations.
	if len(validationErrors) > 0 {
		fmt.Fprintf(os.Stdout, "\nGeneral tips:\n")
		color.New(color.FgCyan).Fprintf(os.Stdout, "  - Check the UAST specification at pkg/uast/spec/SPEC.md\n")
		color.New(color.FgCyan).Fprintf(os.Stdout, "  - Use the schema at pkg/uast/spec/uast-schema.json as reference\n")
		color.New(color.FgCyan).Fprintf(os.Stdout, "  - Ensure all required fields are present\n")
		color.New(color.FgCyan).Fprintf(os.Stdout, "  - Validate field types and values against the schema\n")
	}
}

func classifyRecommendation(recommendations map[string]string, field, description string) {
	// Provide specific recommendations based on error type.
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
		recommendations["position"] = "Position fields should use snake_case: " +
			"start_line, start_col, start_offset, end_line, end_col, end_offset"

	case strings.Contains(description, "additionalProperties"):
		recommendations["props"] = "Properties in 'props' field must be string key-value pairs"

	case strings.Contains(description, "children"):
		recommendations["children"] = "Children field must be an array of UAST nodes"

	case strings.Contains(description, "roles"):
		recommendations["roles"] = "Roles field must be an array of valid UAST roles"
	}
}

func calculateCompliance(inputData any, validationErrors []gojsonschema.ResultError) int {
	// Count total nodes in the UAST.
	totalNodes := countNodes(inputData)
	if totalNodes == 0 {
		return 0
	}

	// Count valid nodes (nodes without errors).
	validNodes := totalNodes - len(validationErrors)
	compliance := int(float64(validNodes) / float64(totalNodes) * complianceMax)

	// Ensure compliance is between 0 and 100.
	if compliance < 0 {
		compliance = 0
	} else if compliance > complianceMax {
		compliance = complianceMax
	}

	return compliance
}

func countNodes(data any) int {
	count := 1 // Count this node.

	switch typedData := data.(type) {
	case map[string]any:
		if children, hasChildren := typedData["children"].([]any); hasChildren {
			for _, child := range children {
				count += countNodes(child)
			}
		}
	case []any:
		for _, item := range typedData {
			count += countNodes(item)
		}
	}

	return count
}

func getActualValue(data any, fieldPath string) string {
	// Parse the field path (e.g., "children.0.roles.0").
	parts := strings.Split(fieldPath, ".")

	current := data

	for _, part := range parts {
		switch typedVal := current.(type) {
		case map[string]any:
			val, found := typedVal[part]
			if !found {
				return ""
			}

			current = val
		case []any:
			idx, convErr := strconv.Atoi(part)
			if convErr != nil || idx < 0 || idx >= len(typedVal) {
				return ""
			}

			current = typedVal[idx]
		default:
			return ""
		}
	}

	// Convert the final value to string.
	return formatValue(current)
}

func formatValue(value any) string {
	switch typedVal := value.(type) {
	case string:
		return typedVal
	case float64:
		return strconv.FormatFloat(typedVal, 'f', -1, 64)
	case int:
		return strconv.Itoa(typedVal)
	case bool:
		return strconv.FormatBool(typedVal)
	default:
		return fmt.Sprintf("%v", typedVal)
	}
}
