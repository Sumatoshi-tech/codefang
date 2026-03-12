package analyze

// ReportMeta describes metadata for a single analyzer's report store entry.
type ReportMeta struct {
	AnalyzerID string `json:"analyzer_id"`
	Version    string `json:"version"`
	SchemaHash string `json:"schema_hash"`
}

// ReportWriter appends typed records for one analyzer.
// Data becomes visible only after Close completes successfully.
type ReportWriter interface {
	// Write appends one typed record under the given kind.
	Write(kind string, record any) error

	// Close finalizes the write. After Close, the data is durable.
	// Idempotent: second call is a no-op.
	Close() error
}

// ReportReader streams records for one analyzer.
// Memory footprint: one decoded record at a time.
type ReportReader interface {
	// Meta returns the metadata for this analyzer's report.
	Meta() ReportMeta

	// Kinds returns the list of record kinds stored for this analyzer.
	Kinds() []string

	// Iter calls fn for each raw record of the given kind.
	// Iteration stops early if fn returns a non-nil error.
	Iter(kind string, fn func(raw []byte) error) error

	// Close releases resources. Idempotent.
	Close() error
}

// ReportStore manages per-analyzer report artifacts.
// Writers and readers are created one at a time; no concurrent access.
type ReportStore interface {
	// Begin starts writing records for the given analyzer.
	Begin(analyzerID string, meta ReportMeta) (ReportWriter, error)

	// Open opens a reader for the given analyzer's stored records.
	Open(analyzerID string) (ReportReader, error)

	// AnalyzerIDs returns the ordered list of analyzer IDs in the store.
	AnalyzerIDs() []string

	// Close releases store-level resources. Idempotent.
	Close() error
}
