package analyze

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"path/filepath"
	"slices"
	"strings"
	"sync"

	"gopkg.in/yaml.v3"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/plotpage"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/reportutil"
)

// UnifiedModelVersion is the schema version for converted run outputs.
const UnifiedModelVersion = "codefang.run.v1"

// ErrInvalidUnifiedModel indicates malformed canonical conversion data.
var ErrInvalidUnifiedModel = errors.New("invalid unified model")

// AnalyzerResult represents one analyzer report in canonical converted output.
type AnalyzerResult struct {
	ID     string       `json:"id"     yaml:"id"`
	Mode   AnalyzerMode `json:"mode"   yaml:"mode"`
	Report Report       `json:"report" yaml:"report"`
}

// UnifiedModel is the canonical intermediate model for run output conversion.
type UnifiedModel struct {
	Version   string           `json:"version"   yaml:"version"`
	Analyzers []AnalyzerResult `json:"analyzers" yaml:"analyzers"`
}

// Validate ensures canonical model constraints are satisfied.
func (m UnifiedModel) Validate() error {
	if m.Version != UnifiedModelVersion {
		return fmt.Errorf("%w: unsupported version %q", ErrInvalidUnifiedModel, m.Version)
	}

	for i, analyzer := range m.Analyzers {
		if strings.TrimSpace(analyzer.ID) == "" {
			return fmt.Errorf("%w: empty analyzer id at index %d", ErrInvalidUnifiedModel, i)
		}

		if !slices.Contains([]AnalyzerMode{ModeStatic, ModeHistory}, analyzer.Mode) {
			return fmt.Errorf("%w: invalid mode %q for analyzer %q", ErrInvalidUnifiedModel, analyzer.Mode, analyzer.ID)
		}

		if analyzer.Report == nil {
			return fmt.Errorf("%w: nil report for analyzer %q", ErrInvalidUnifiedModel, analyzer.ID)
		}
	}

	return nil
}

// ParseUnifiedModelJSON parses canonical JSON into UnifiedModel.
func ParseUnifiedModelJSON(data []byte) (UnifiedModel, error) {
	model := UnifiedModel{}

	err := json.Unmarshal(data, &model)
	if err != nil {
		return UnifiedModel{}, fmt.Errorf("%w: %w", ErrInvalidUnifiedModel, err)
	}

	err = model.Validate()
	if err != nil {
		return UnifiedModel{}, err
	}

	return model, nil
}

// InputFormatAuto is the default input format that triggers extension-based detection.
const InputFormatAuto = "auto"

var (
	// ErrInvalidMixedFormat indicates a format that cannot be used in combined static+history runs.
	ErrInvalidMixedFormat = errors.New("invalid mixed format")
	// ErrInvalidStaticFormat indicates an invalid static analysis output format.
	ErrInvalidStaticFormat = errors.New("invalid static format")
	// ErrInvalidHistoryFormat indicates an invalid history analysis output format.
	ErrInvalidHistoryFormat = errors.New("invalid history format")
	// ErrInvalidInputFormat indicates an unrecognized input format.
	ErrInvalidInputFormat = errors.New("invalid input format")
	// ErrBinaryEnvelopeCount indicates an unexpected number of binary envelopes.
	ErrBinaryEnvelopeCount = errors.New("unexpected binary envelope count")
)

// ResolveFormats determines the output formats for static and history phases based on
// the user-provided format string and whether each phase is active.
func ResolveFormats(format string, hasStatic, hasHistory bool) (staticFmt, historyFmt string, err error) {
	if hasStatic && hasHistory {
		normalizedFormat, validationErr := ValidateUniversalFormat(format)
		if validationErr != nil {
			return "", "", fmt.Errorf("%w: %w", ErrInvalidMixedFormat, validationErr)
		}

		return normalizedFormat, normalizedFormat, nil
	}

	if hasStatic {
		normalizedFormat, validationErr := ValidateFormat(format, staticOutputFormats())
		if validationErr != nil {
			return "", "", fmt.Errorf("%w: %w", ErrInvalidStaticFormat, validationErr)
		}

		return normalizedFormat, "", nil
	}

	if hasHistory {
		normalizedFormat, validationErr := ValidateUniversalFormat(format)
		if validationErr != nil {
			return "", "", fmt.Errorf("%w: %w", ErrInvalidHistoryFormat, validationErr)
		}

		return "", normalizedFormat, nil
	}

	return "", "", nil
}

// staticOutputFormats returns the output formats supported by static analyzers.
func staticOutputFormats() []string {
	return []string{
		FormatText,
		FormatCompact,
		FormatJSON,
		FormatYAML,
		FormatPlot,
		FormatBinary,
	}
}

// ResolveInputFormat determines the input format from the provided path and explicit format hint.
// When the format is empty or InputFormatAuto, the extension of inputPath is used to detect the format.
func ResolveInputFormat(inputPath, inputFormat string) (string, error) {
	normalized := strings.TrimSpace(inputFormat)
	if normalized == "" || normalized == InputFormatAuto {
		if strings.EqualFold(filepath.Ext(inputPath), ".bin") {
			return FormatBinary, nil
		}

		return FormatJSON, nil
	}

	normalized = NormalizeFormat(normalized)

	switch normalized {
	case FormatJSON, FormatBinary:
		return normalized, nil
	default:
		return "", fmt.Errorf("%w: %s", ErrInvalidInputFormat, inputFormat)
	}
}

// DecodeInputModel dispatches input decoding based on the inputFormat.
func DecodeInputModel(
	input []byte,
	inputFormat string,
) (UnifiedModel, error) {
	switch inputFormat {
	case FormatJSON:
		return decodeJSONInputModel(input)
	case FormatBinary:
		return DecodeBinaryInputModel(input)
	default:
		return UnifiedModel{}, fmt.Errorf("%w: %s", ErrInvalidInputFormat, inputFormat)
	}
}

// decodeJSONInputModel parses canonical unified JSON input.
func decodeJSONInputModel(input []byte) (UnifiedModel, error) {
	return ParseUnifiedModelJSON(input)
}

// DecodeBinaryInputModel decodes a single binary envelope containing canonical unified JSON.
func DecodeBinaryInputModel(input []byte) (UnifiedModel, error) {
	payloads, err := reportutil.DecodeBinaryEnvelopes(input)
	if err != nil {
		return UnifiedModel{}, fmt.Errorf("decode binary envelopes: %w", err)
	}

	if len(payloads) != 1 {
		return UnifiedModel{}, fmt.Errorf("%w: expected 1, got %d", ErrBinaryEnvelopeCount, len(payloads))
	}

	return ParseUnifiedModelJSON(payloads[0])
}

// DecodeCombinedBinaryReports decodes multiple binary envelopes, each containing
// a raw Report JSON, and pairs them positionally with the provided analyzer IDs
// and modes to build a UnifiedModel. This is used by the combined static+history
// rendering path where each phase serializes its Reports as separate envelopes.
func DecodeCombinedBinaryReports(input []byte, ids []string, modes []AnalyzerMode) (UnifiedModel, error) {
	payloads, err := reportutil.DecodeBinaryEnvelopes(input)
	if err != nil {
		return UnifiedModel{}, fmt.Errorf("decode binary envelopes: %w", err)
	}

	if len(payloads) != len(ids) {
		return UnifiedModel{}, fmt.Errorf("%w: expected %d, got %d", ErrBinaryEnvelopeCount, len(ids), len(payloads))
	}

	results := make([]AnalyzerResult, len(payloads))

	for i, payload := range payloads {
		var report Report

		jsonErr := json.Unmarshal(payload, &report)
		if jsonErr != nil {
			return UnifiedModel{}, fmt.Errorf("unmarshal report %d: %w", i, jsonErr)
		}

		results[i] = AnalyzerResult{
			ID:     ids[i],
			Mode:   modes[i],
			Report: report,
		}
	}

	return UnifiedModel{
		Version:   UnifiedModelVersion,
		Analyzers: results,
	}, nil
}

// PlotRenderer is a function that renders a UnifiedModel as a plot to the given writer.
// It is provided by the renderer package to avoid import cycles.
type PlotRenderer func(model UnifiedModel, writer io.Writer) error

// plotRendererFn holds the registered plot renderer. Nil until set via RegisterPlotRenderer.
var plotRendererFn PlotRenderer

// RegisterPlotRenderer sets the package-level plot renderer used by WriteConvertedOutput.
// It is intended to be called from the renderer package's init function.
func RegisterPlotRenderer(fn PlotRenderer) {
	plotRendererFn = fn
}

// SectionRendererFunc generates plot sections from a raw report for a specific analyzer.
type SectionRendererFunc func(report Report) ([]plotpage.Section, error)

// plotSectionRenderers maps analyzer IDs to their plot section generators.
var (
	plotSectionRenderersMu sync.RWMutex
	plotSectionRenderers   = make(map[string]SectionRendererFunc)
)

// RegisterPlotSections registers a plot section renderer for the given analyzer ID.
func RegisterPlotSections(analyzerID string, fn SectionRendererFunc) {
	plotSectionRenderersMu.Lock()
	defer plotSectionRenderersMu.Unlock()

	plotSectionRenderers[analyzerID] = fn
}

// PlotSectionsFor returns the registered section renderer for an analyzer ID, or nil.
func PlotSectionsFor(analyzerID string) SectionRendererFunc {
	plotSectionRenderersMu.RLock()
	defer plotSectionRenderersMu.RUnlock()

	return plotSectionRenderers[analyzerID]
}

// StoreSectionRendererFunc renders plot sections from a ReportReader.
// Used by analyzers that implement DirectStoreWriter and emit structured kinds
// instead of a monolithic "report" gob record.
type StoreSectionRendererFunc func(reader ReportReader) ([]plotpage.Section, error)

// storeSectionRenderers maps analyzer IDs to their store-aware section generators.
var (
	storeSectionRenderersMu sync.RWMutex
	storeSectionRenderers   = make(map[string]StoreSectionRendererFunc)
)

// RegisterStorePlotSections registers a store-aware plot section renderer for the given analyzer ID.
func RegisterStorePlotSections(analyzerID string, fn StoreSectionRendererFunc) {
	storeSectionRenderersMu.Lock()
	defer storeSectionRenderersMu.Unlock()

	storeSectionRenderers[analyzerID] = fn
}

// StorePlotSectionsFor returns the registered store section renderer for an analyzer ID, or nil.
func StorePlotSectionsFor(analyzerID string) StoreSectionRendererFunc {
	storeSectionRenderersMu.RLock()
	defer storeSectionRenderersMu.RUnlock()

	return storeSectionRenderers[analyzerID]
}

// WriteConvertedOutput encodes the unified model into the requested output format
// and writes it to the provided writer.
func WriteConvertedOutput(model UnifiedModel, outputFormat string, writer io.Writer) error {
	switch outputFormat {
	case FormatJSON:
		encoder := json.NewEncoder(writer)
		encoder.SetIndent("", "  ")

		err := encoder.Encode(model)
		if err != nil {
			return fmt.Errorf("encode converted json: %w", err)
		}

		return nil
	case FormatYAML:
		data, err := yaml.Marshal(model)
		if err != nil {
			return fmt.Errorf("encode converted yaml: %w", err)
		}

		_, err = writer.Write(data)
		if err != nil {
			return fmt.Errorf("write converted yaml: %w", err)
		}

		return nil
	case FormatBinary:
		err := reportutil.EncodeBinaryEnvelope(model, writer)
		if err != nil {
			return fmt.Errorf("encode converted binary: %w", err)
		}

		return nil
	case FormatTimeSeries:
		return writeConvertedTimeSeries(model, FormatTimeSeries, writer)
	case FormatTimeSeriesNDJSON:
		return writeConvertedTimeSeries(model, FormatTimeSeriesNDJSON, writer)
	case FormatPlot:
		if plotRendererFn == nil {
			return fmt.Errorf("%w: plot renderer not registered", ErrUnsupportedFormat)
		}

		return plotRendererFn(model, writer)
	default:
		return fmt.Errorf("%w: %s", ErrUnsupportedFormat, outputFormat)
	}
}

// writeConvertedTimeSeries builds merged timeseries from a unified model's
// history reports and writes the result to the writer.
func writeConvertedTimeSeries(model UnifiedModel, format string, writer io.Writer) error {
	reports := make(map[string]Report, len(model.Analyzers))

	for _, ar := range model.Analyzers {
		if ar.Mode == ModeHistory {
			reports[ar.ID] = ar.Report
		}
	}

	commitMeta := buildOrderedCommitMetaFromReports(reports)
	ts := BuildMergedTimeSeriesDirect(nil, commitMeta, 0)

	if format == FormatTimeSeriesNDJSON {
		return WriteTimeSeriesNDJSON(ts, writer)
	}

	return WriteMergedTimeSeries(ts, writer)
}
