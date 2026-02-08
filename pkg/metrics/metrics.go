// Package metrics provides interfaces for defining self-contained, reusable metrics.
//
// Each metric is a computation unit that:
//   - Declares its input requirements
//   - Computes a typed output
//   - Provides metadata for documentation and serialization
//
// This design allows metrics to be reused across analyzers and output formats.
package metrics

// Metric is the core interface that all metrics must implement.
// Each metric is a self-contained computation with metadata.
type Metric[In, Out any] interface {
	// Name returns the machine-readable identifier (snake_case, unique).
	Name() string

	// DisplayName returns a human-readable name for UI/reports.
	DisplayName() string

	// Description returns detailed documentation including:
	// - What the metric measures.
	// - How to interpret the value.
	// - Units (if applicable).
	// - Any caveats or limitations.
	Description() string

	// Type returns the metric category (e.g., "aggregate", "time_series", "risk").
	Type() string

	// Compute calculates the metric value from input data.
	Compute(input In) Out
}

// TimeSeriesPoint is a single data point in a time series.
type TimeSeriesPoint struct {
	Tick  int     `json:"tick"`
	Value float64 `json:"value"`
}

// RiskLevel represents severity levels.
type RiskLevel string

// Risk level constants.
const (
	RiskCritical RiskLevel = "CRITICAL"
	RiskHigh     RiskLevel = "HIGH"
	RiskMedium   RiskLevel = "MEDIUM"
	RiskLow      RiskLevel = "LOW"
)

// RiskResult is the output of a risk metric.
type RiskResult struct {
	Value     any       `json:"value"`
	Level     RiskLevel `json:"risk_level"`
	Threshold float64   `json:"threshold,omitempty"`
	Message   string    `json:"message,omitempty"`
}

// MetricMeta holds the common metadata for a metric.
// Embed this in metric implementations to satisfy metadata methods.
type MetricMeta struct {
	MetricName        string
	MetricDisplayName string
	MetricDescription string
	MetricType        string
}

// Name returns the machine-readable identifier.
func (m MetricMeta) Name() string { return m.MetricName }

// DisplayName returns a human-readable name for UI/reports.
func (m MetricMeta) DisplayName() string { return m.MetricDisplayName }

// Description returns detailed documentation.
func (m MetricMeta) Description() string { return m.MetricDescription }

// Type returns the metric category.
func (m MetricMeta) Type() string { return m.MetricType }

// Registry holds a collection of metrics that can be computed together.
type Registry struct {
	metrics map[string]any // name -> Metric[any, any].
}

// NewRegistry creates an empty metric registry.
func NewRegistry() *Registry {
	return &Registry{metrics: make(map[string]any)}
}

// Register adds a metric to the registry.
func Register[In, Out any](r *Registry, m Metric[In, Out]) {
	r.metrics[m.Name()] = m
}

// Get retrieves a metric by name.
func (r *Registry) Get(name string) (any, bool) {
	m, ok := r.metrics[name]

	return m, ok
}

// Names returns all registered metric names.
func (r *Registry) Names() []string {
	names := make([]string, 0, len(r.metrics))

	for name := range r.metrics {
		names = append(names, name)
	}

	return names
}
