package analyze

// FieldMeta describes a single field in an analyzer's output schema.
type FieldMeta struct {
	Type        string `json:"type"                  yaml:"type"`
	Grain       string `json:"grain,omitempty"       yaml:"grain,omitempty"`
	Description string `json:"description,omitempty" yaml:"description,omitempty"`
}

// AnalyzerSchema maps output field names to their metadata.
type AnalyzerSchema map[string]FieldMeta

// SchemaForAnalyzer returns the output schema for the given analyzer ID,
// or nil if the analyzer is not registered.
func SchemaForAnalyzer(analyzerID string) AnalyzerSchema {
	schema, ok := analyzerSchemas[analyzerID]
	if !ok {
		return nil
	}

	return schema
}

// analyzerSchemas is the static registry of output schemas for all analyzers.
var analyzerSchemas = map[string]AnalyzerSchema{
	"static/complexity": {
		"function_complexity": {Type: "list", Grain: "function", Description: "Per-function cyclomatic and cognitive complexity"},
		"distribution":        {Type: "aggregate", Description: "Complexity distribution (simple/moderate/complex)"},
		"high_risk_functions": {Type: "risk", Grain: "function", Description: "Functions exceeding complexity thresholds"},
		"aggregate":           {Type: "aggregate", Description: "Summary statistics"},
	},
	"static/halstead": {
		"function_halstead":     {Type: "list", Grain: "function", Description: "Per-function Halstead volume, effort, and bugs"},
		"distribution":          {Type: "aggregate", Description: "Effort distribution (low/medium/high/very_high)"},
		"high_effort_functions": {Type: "risk", Grain: "function", Description: "Functions with high Halstead effort"},
		"aggregate":             {Type: "aggregate", Description: "Summary statistics"},
	},
	"static/cohesion": {
		"function_cohesion":      {Type: "list", Grain: "function", Description: "Per-function LCOM cohesion score"},
		"distribution":           {Type: "aggregate", Description: "Cohesion distribution"},
		"low_cohesion_functions": {Type: "risk", Grain: "function", Description: "Functions with poor cohesion"},
		"aggregate":              {Type: "aggregate", Description: "Summary statistics"},
	},
	"static/comments": {
		"comment_quality":        {Type: "list", Grain: "comment", Description: "Per-comment quality assessment"},
		"function_documentation": {Type: "list", Grain: "function", Description: "Per-function documentation status"},
		"undocumented_functions": {Type: "risk", Grain: "function", Description: "Functions lacking documentation"},
		"aggregate":              {Type: "aggregate", Description: "Summary statistics"},
	},
	"static/clones": {
		"clone_pairs":             {Type: "list", Grain: "pair", Description: "Detected clone pairs with similarity"},
		"clone_type_distribution": {Type: "aggregate", Description: "Clone type breakdown (Type-1/2/3)"},
		"total_functions":         {Type: "scalar", Description: "Total functions analyzed"},
		"total_clone_pairs":       {Type: "scalar", Description: "Total clone pairs (uncapped)"},
		"clone_ratio":             {Type: "scalar", Description: "Fraction of functions involved in duplication"},
	},
	"static/imports": {
		"import_list":  {Type: "list", Grain: "import", Description: "All import statements"},
		"dependencies": {Type: "list", Grain: "dependency", Description: "External dependencies with risk"},
		"categories":   {Type: "aggregate", Description: "Import category breakdown"},
		"aggregate":    {Type: "aggregate", Description: "Summary statistics"},
	},
	"static/composition": {
		"breakdown":   {Type: "aggregate", Description: "File count per category"},
		"percentages": {Type: "aggregate", Description: "Percentage per category"},
		"total_files": {Type: "scalar", Description: "Total files analyzed"},
	},
	"history/sentiment": {
		"time_series":           {Type: "time_series", Grain: "tick", Description: "Per-tick sentiment scores"},
		"trend":                 {Type: "aggregate", Description: "Sentiment trend direction"},
		"low_sentiment_periods": {Type: "risk", Grain: "tick", Description: "Ticks with negative sentiment"},
		"aggregate":             {Type: "aggregate", Description: "Summary statistics"},
	},
	"history/anomaly": {
		"anomalies":   {Type: "list", Grain: "tick", Description: "Detected anomalous ticks"},
		"time_series": {Type: "time_series", Grain: "tick", Description: "Per-tick anomaly metrics and z-scores"},
		"aggregate":   {Type: "aggregate", Description: "Summary statistics"},
	},
	"history/devs": {
		"developers": {Type: "list", Grain: "developer", Description: "Per-developer contribution statistics"},
		"languages":  {Type: "list", Grain: "language", Description: "Per-language contribution breakdown"},
		"busfactor":  {Type: "list", Grain: "language", Description: "Bus factor per language"},
		"activity":   {Type: "time_series", Grain: "tick", Description: "Per-tick commit activity by developer"},
		"churn":      {Type: "time_series", Grain: "tick", Description: "Per-tick lines added/removed"},
		"aggregate":  {Type: "aggregate", Description: "Summary statistics"},
	},
	"history/file-history": {
		"file_churn":        {Type: "list", Grain: "file", Description: "Per-file change frequency and contributors"},
		"file_contributors": {Type: "list", Grain: "file", Description: "Per-file contributor breakdown"},
		"hotspots":          {Type: "risk", Grain: "file", Description: "High-churn files"},
		"composition":       {Type: "aggregate", Description: "File type composition"},
		"composition_ts":    {Type: "time_series", Grain: "tick", Description: "File composition over time"},
		"aggregate":         {Type: "aggregate", Description: "Summary statistics"},
	},
	"history/couples": {
		"file_coupling":      {Type: "list", Grain: "pair", Description: "Co-changed file pairs"},
		"developer_coupling": {Type: "list", Grain: "pair", Description: "Developer collaboration pairs"},
		"file_ownership":     {Type: "list", Grain: "file", Description: "Per-file ownership"},
		"aggregate":          {Type: "aggregate", Description: "Summary statistics"},
	},
	"history/shotness": {
		"node_hotness":  {Type: "list", Grain: "node", Description: "AST node change frequency"},
		"node_coupling": {Type: "list", Grain: "pair", Description: "Co-changed AST node pairs"},
		"hotspot_nodes": {Type: "risk", Grain: "node", Description: "Frequently changed nodes"},
		"aggregate":     {Type: "aggregate", Description: "Summary statistics"},
	},
	"history/burndown": {
		"global_survival":    {Type: "time_series", Grain: "sample", Description: "Global code survival curve"},
		"file_survival":      {Type: "list", Grain: "file", Description: "Per-file survival data"},
		"developer_survival": {Type: "list", Grain: "developer", Description: "Per-developer survival data"},
		"aggregate":          {Type: "aggregate", Description: "Summary statistics"},
	},
	"history/quality": {
		"time_series": {Type: "time_series", Grain: "tick", Description: "Per-tick code quality metrics"},
		"aggregate":   {Type: "aggregate", Description: "Summary statistics"},
	},
	"history/imports": {
		"import_list":  {Type: "list", Grain: "import", Description: "Import statements (requires UAST mode)"},
		"dependencies": {Type: "list", Grain: "dependency", Description: "Dependencies (requires UAST mode)"},
		"categories":   {Type: "aggregate", Description: "Import category breakdown"},
		"aggregate":    {Type: "aggregate", Description: "Summary statistics"},
	},
	"history/typos": {
		"typos": {Type: "list", Grain: "identifier", Description: "Detected identifier typos (requires UAST mode)"},
	},
}
