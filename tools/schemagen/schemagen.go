// Package main generates JSON schemas for analyzer ComputedMetrics structs.
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"time"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/burndown"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/cohesion"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/comments"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/complexity"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/couples"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/devs"
	filehistory "github.com/Sumatoshi-tech/codefang/internal/analyzers/file_history"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/halstead"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/imports"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/sentiment"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/shotness"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/typos"
)

// Schema represents a JSON Schema.
type Schema struct {
	Schema      string             `json:"$schema,omitempty"`
	Title       string             `json:"title,omitempty"`
	Description string             `json:"description,omitempty"`
	Type        string             `json:"type,omitempty"`
	Properties  map[string]*Schema `json:"properties,omitempty"`
	Items       *Schema            `json:"items,omitempty"`
	Required    []string           `json:"required,omitempty"`
	Ref         string             `json:"$ref,omitempty"`
	Definitions map[string]*Schema `json:"definitions,omitempty"`
}

var outputDir string

func main() {
	flag.StringVar(&outputDir, "o", "docs/schemas", "Output directory for schemas")
	flag.Parse()

	if err := os.MkdirAll(outputDir, 0o755); err != nil {
		fmt.Fprintf(os.Stderr, "Error creating output directory: %v\n", err)
		os.Exit(1)
	}

	analyzers := map[string]any{
		"devs":         &devs.ComputedMetrics{},
		"burndown":     &burndown.ComputedMetrics{},
		"file_history": &filehistory.ComputedMetrics{},
		"couples":      &couples.ComputedMetrics{},
		"shotness":     &shotness.ComputedMetrics{},
		"sentiment":    &sentiment.ComputedMetrics{},
		"complexity":   &complexity.ComputedMetrics{},
		"cohesion":     &cohesion.ComputedMetrics{},
		"halstead":     &halstead.ComputedMetrics{},
		"comments":     &comments.ComputedMetrics{},
		"imports":      &imports.ComputedMetrics{},
		"typos":        &typos.ComputedMetrics{},
	}

	for name, metrics := range analyzers {
		schema := generateSchema(name, metrics)
		if err := writeSchema(name, schema); err != nil {
			fmt.Fprintf(os.Stderr, "Error writing schema for %s: %v\n", name, err)
			os.Exit(1)
		}

		fmt.Printf("Generated schema for %s\n", name)
	}

	fmt.Println("All schemas generated successfully")
}

func generateSchema(name string, v any) *Schema {
	t := reflect.TypeOf(v)
	if t.Kind() == reflect.Ptr {
		t = t.Elem()
	}

	defs := make(map[string]*Schema)
	props, required := structToProperties(t, defs)

	schema := &Schema{
		Schema:      "https://json-schema.org/draft-07/schema#",
		Title:       fmt.Sprintf("%s Analyzer Output", strings.Title(name)),
		Description: fmt.Sprintf("JSON schema for the %s analyzer ComputedMetrics output", name),
		Type:        "object",
		Properties:  props,
		Required:    required,
	}

	if len(defs) > 0 {
		schema.Definitions = defs
	}

	return schema
}

func structToProperties(t reflect.Type, defs map[string]*Schema) (map[string]*Schema, []string) {
	props := make(map[string]*Schema)

	var required []string

	for i := 0; i < t.NumField(); i++ {
		field := t.Field(i)
		jsonTag := field.Tag.Get("json")

		if jsonTag == "-" || jsonTag == "" {
			continue
		}

		parts := strings.Split(jsonTag, ",")
		jsonName := parts[0]
		isOmitempty := len(parts) > 1 && parts[1] == "omitempty"

		fieldSchema := typeToSchema(field.Type, defs)
		props[jsonName] = fieldSchema

		if !isOmitempty {
			required = append(required, jsonName)
		}
	}

	return props, required
}

func typeToSchema(t reflect.Type, defs map[string]*Schema) *Schema {
	switch t.Kind() {
	case reflect.String:
		return &Schema{Type: "string"}

	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		if t == reflect.TypeOf(time.Duration(0)) {
			return &Schema{Type: "integer", Description: "Duration in nanoseconds"}
		}

		return &Schema{Type: "integer"}

	case reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64:
		return &Schema{Type: "integer"}

	case reflect.Float32, reflect.Float64:
		return &Schema{Type: "number"}

	case reflect.Bool:
		return &Schema{Type: "boolean"}

	case reflect.Slice:
		return &Schema{
			Type:  "array",
			Items: typeToSchema(t.Elem(), defs),
		}

	case reflect.Map:
		return &Schema{
			Type: "object",
			Description: fmt.Sprintf("Map with %s keys and %s values",
				t.Key().Kind().String(), t.Elem().Kind().String()),
		}

	case reflect.Struct:
		if t == reflect.TypeOf(time.Time{}) {
			return &Schema{Type: "string", Description: "ISO 8601 timestamp"}
		}

		defName := t.Name()
		if defName == "" {
			props, required := structToProperties(t, defs)

			return &Schema{Type: "object", Properties: props, Required: required}
		}

		if _, exists := defs[defName]; !exists {
			props, required := structToProperties(t, defs)
			defs[defName] = &Schema{Type: "object", Properties: props, Required: required}
		}

		return &Schema{Ref: "#/definitions/" + defName}

	case reflect.Ptr:
		return typeToSchema(t.Elem(), defs)

	default:
		return &Schema{Type: "object"}
	}
}

func writeSchema(name string, schema *Schema) error {
	data, err := json.MarshalIndent(schema, "", "  ")
	if err != nil {
		return fmt.Errorf("marshal schema: %w", err)
	}

	path := filepath.Join(outputDir, name+".json")

	return os.WriteFile(path, data, 0o644)
}
