package analyze

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"strings"

	"gopkg.in/yaml.v3"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/reportutil"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

// ErrMissingComputeMetrics is returned when Serialize is called but ComputeMetricsFn is nil.
var ErrMissingComputeMetrics = errors.New("missing ComputeMetricsFn hook")

// metricsSerializer matches metricsSerializer to break the import cycle
// between analyze and renderer. Go's structural typing ensures all ComputedMetrics
// types satisfy this interface without an explicit dependency.
type metricsSerializer interface {
	ToJSON() any
	ToYAML() any
}

// MetricComputer defines how raw report data is converted to typed metrics.
type MetricComputer[M any] func(Report) (M, error)

// BaseHistoryAnalyzer provides a complete default implementation for HistoryAnalyzer and Parallelizable.
// It is intended to be embedded by concrete analyzers to reduce boilerplate.
type BaseHistoryAnalyzer[M any] struct {
	Desc               Descriptor
	Sequential         bool
	CPUHeavyFlag       bool
	EstimatedStateSize int64
	EstimatedTCSize    int64
	ConfigOptions      []pipeline.ConfigurationOption

	// Hooks.
	ComputeMetricsFn MetricComputer[M]
	TicksToReportFn  func(ctx context.Context, ticks []TICK) Report
	AggregatorFn     func(opts AggregatorOptions) Aggregator

	// Custom format hooks — set these to handle FormatText and FormatPlot
	// without overriding Serialize/SerializeTICKs. When set, the base
	// dispatch checks them before falling through to JSON/YAML/Binary.
	SerializeTextFn func(result Report, writer io.Writer) error
	SerializePlotFn func(result Report, writer io.Writer) error
}

// Name returns the analyzer name (derived from the descriptor ID).
func (b *BaseHistoryAnalyzer[M]) Name() string {
	return b.Desc.ID
}

// Flag returns the CLI flag for the analyzer, typically the part after "history/".
func (b *BaseHistoryAnalyzer[M]) Flag() string {
	parts := strings.Split(b.Desc.ID, "/")
	if len(parts) > 1 {
		return parts[1]
	}

	return b.Desc.ID
}

// Description returns the analyzer description from the descriptor.
func (b *BaseHistoryAnalyzer[M]) Description() string {
	return b.Desc.Description
}

// Descriptor returns stable analyzer metadata.
func (b *BaseHistoryAnalyzer[M]) Descriptor() Descriptor {
	return b.Desc
}

// SequentialOnly returns true if this analyzer cannot be parallelized.
func (b *BaseHistoryAnalyzer[M]) SequentialOnly() bool {
	return b.Sequential
}

// CPUHeavy returns true if this analyzer's Consume() is CPU-intensive.
func (b *BaseHistoryAnalyzer[M]) CPUHeavy() bool {
	return b.CPUHeavyFlag
}

// WorkingStateSize returns the estimated bytes of analyzer-internal working state.
func (b *BaseHistoryAnalyzer[M]) WorkingStateSize() int64 {
	return b.EstimatedStateSize
}

// AvgTCSize returns the estimated bytes of TC payload emitted per commit.
func (b *BaseHistoryAnalyzer[M]) AvgTCSize() int64 {
	return b.EstimatedTCSize
}

// ListConfigurationOptions returns the configurable options for this analyzer.
func (b *BaseHistoryAnalyzer[M]) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return b.ConfigOptions
}

// Configure is a default implementation that does nothing.
func (b *BaseHistoryAnalyzer[M]) Configure(_ map[string]any) error {
	return nil
}

// NewAggregator creates an aggregator using the configured AggregatorFn hook.
// Returns nil when no AggregatorFn is set (e.g., plumbing analyzers).
// Concrete analyzers that need runtime state at aggregator creation time
// (e.g., couples, burndown) should override this method.
func (b *BaseHistoryAnalyzer[M]) NewAggregator(opts AggregatorOptions) Aggregator {
	if b.AggregatorFn != nil {
		return b.AggregatorFn(opts)
	}

	return nil
}

// resolveSerializeTarget returns the format-appropriate representation of metrics.
// If metrics implements metricsSerializer, the format-specific method is called.
func resolveSerializeTarget(metrics any, toFn func(metricsSerializer) any) any {
	if mo, ok := metrics.(metricsSerializer); ok {
		return toFn(mo)
	}

	return metrics
}

// Serialize dynamically uses ComputeMetricsFn and standard encodings.
// Custom format hooks (SerializeTextFn, SerializePlotFn) are checked first,
// allowing analyzers to support text/plot output without overriding this method.
func (b *BaseHistoryAnalyzer[M]) Serialize(result Report, format string, writer io.Writer) error {
	// Custom format hooks take priority.
	if format == FormatText && b.SerializeTextFn != nil {
		return b.SerializeTextFn(result, writer)
	}

	if format == FormatPlot && b.SerializePlotFn != nil {
		return b.SerializePlotFn(result, writer)
	}

	if b.ComputeMetricsFn == nil {
		return ErrMissingComputeMetrics
	}

	metrics, err := b.ComputeMetricsFn(result)
	if err != nil {
		return err
	}

	return writeMetricsToFormat(metrics, format, writer)
}

// writeMetricsToFormat encodes metrics in the requested format and writes to writer.
func writeMetricsToFormat(metrics any, format string, writer io.Writer) error {
	switch format {
	case FormatJSON:
		target := resolveSerializeTarget(metrics, metricsSerializer.ToJSON)

		return marshalAndWrite(target, json.Marshal, writer, "json")
	case FormatYAML:
		target := resolveSerializeTarget(metrics, metricsSerializer.ToYAML)

		return marshalAndWrite(target, yaml.Marshal, writer, "yaml")
	case FormatBinary:
		errBinary := reportutil.EncodeBinaryEnvelope(metrics, writer)
		if errBinary != nil {
			return fmt.Errorf("binary encode: %w", errBinary)
		}

		return nil
	default:
		return fmt.Errorf("%w: %s", ErrUnsupportedFormat, format)
	}
}

// marshalAndWrite marshals data and writes the result to writer.
func marshalAndWrite(data any, marshal func(any) ([]byte, error), writer io.Writer, label string) error {
	encoded, err := marshal(data)
	if err != nil {
		return fmt.Errorf("%s encode: %w", label, err)
	}

	_, writeErr := writer.Write(encoded)
	if writeErr != nil {
		return fmt.Errorf("%s write: %w", label, writeErr)
	}

	return nil
}

// SerializeTICKs uses TicksToReportFn and delegates to Serialize.
func (b *BaseHistoryAnalyzer[M]) SerializeTICKs(ticks []TICK, format string, writer io.Writer) error {
	if b.TicksToReportFn == nil {
		return ErrNotImplemented
	}

	report := b.TicksToReportFn(context.Background(), ticks)

	return b.Serialize(report, format, writer)
}

// ReportFromTICKs converts aggregated TICKs into a Report.
func (b *BaseHistoryAnalyzer[M]) ReportFromTICKs(ctx context.Context, ticks []TICK) (Report, error) {
	if b.TicksToReportFn == nil {
		return nil, ErrNotImplemented
	}

	return b.TicksToReportFn(ctx, ticks), nil
}

// SnapshotPlumbing provides a default no-op implementation.
func (b *BaseHistoryAnalyzer[M]) SnapshotPlumbing() PlumbingSnapshot {
	return nil
}

// ApplySnapshot provides a default no-op implementation.
func (b *BaseHistoryAnalyzer[M]) ApplySnapshot(_ PlumbingSnapshot) {
}

// ReleaseSnapshot provides a default no-op implementation.
func (b *BaseHistoryAnalyzer[M]) ReleaseSnapshot(_ PlumbingSnapshot) {
}
