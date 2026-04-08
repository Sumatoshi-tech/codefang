package analyze

// FRD: specs/frds/FRD-20260311-typed-report-items.md.

// ItemConverter converts a typed items slice and source file path into []map[string]any.
// The sourceFile parameter is the path stamped by StampSourceFile; when non-empty, the
// converter should include it as "_source_file" in each output map.
type ItemConverter func(items any, sourceFile string) []map[string]any

// TypedCollection wraps a typed struct slice for deferred map conversion.
// Per-file analyzers place a TypedCollection in the report instead of []map[string]any.
// Conversion to maps is deferred to the serialization boundary (e.g., AddToResult).
type TypedCollection struct {
	Items      any           // concrete typed slice (e.g., []FunctionMetrics).
	SourceFile string        // stamped by StampSourceFile.
	Language   string        // stamped by StampLanguage.
	Directory  string        // stamped by StampSourceFile (filepath.Dir of relative path).
	ToMaps     ItemConverter // deferred converter.
}

// MapSlice converts the typed items to []map[string]any using the stored converter.
func (tc TypedCollection) MapSlice() []map[string]any {
	if tc.ToMaps == nil {
		return nil
	}

	return tc.ToMaps(tc.Items, tc.SourceFile)
}

// SourceFileKey is the report key used to stamp the originating source file.
const SourceFileKey = "_source_file"

// LanguageKey is the report key used to stamp the detected programming language.
const LanguageKey = "_language"

// DirectoryKey is the report key used to stamp the parent directory of the source file.
const DirectoryKey = "_directory"
