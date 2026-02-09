package analyze

import (
	"bytes"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"math"
	"path/filepath"
	"slices"
	"strings"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
	"gopkg.in/yaml.v3"
)

// ---------------------------------------------------------------------------
// Unified model types
// ---------------------------------------------------------------------------

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

// NewUnifiedModel builds a canonical model from analyzer results.
func NewUnifiedModel(results []AnalyzerResult) UnifiedModel {
	copied := make([]AnalyzerResult, len(results))
	copy(copied, results)

	return UnifiedModel{
		Version:   UnifiedModelVersion,
		Analyzers: copied,
	}
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

// ---------------------------------------------------------------------------
// Sentinel errors for format resolution and input conversion
// ---------------------------------------------------------------------------

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
	// ErrLegacyInputAmbiguous indicates legacy input requires exactly one analyzer.
	ErrLegacyInputAmbiguous = errors.New("legacy input requires exactly one analyzer id")
	// ErrLegacyBinaryCount indicates a binary envelope count mismatch.
	ErrLegacyBinaryCount = errors.New("legacy binary envelope count mismatch")
)

// ---------------------------------------------------------------------------
// Format resolution
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Input format resolution
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Ordered run IDs
// ---------------------------------------------------------------------------

// OrderedRunIDs splits the provided analyzer IDs by mode via the registry and returns
// them in static-first, history-second order.
func OrderedRunIDs(registry *Registry, ids []string) ([]string, error) {
	staticIDs, historyIDs, err := registry.Split(ids)
	if err != nil {
		return nil, err
	}

	ordered := make([]string, 0, len(staticIDs)+len(historyIDs))
	ordered = append(ordered, staticIDs...)
	ordered = append(ordered, historyIDs...)

	return ordered, nil
}

// ---------------------------------------------------------------------------
// Input model decoding
// ---------------------------------------------------------------------------

// DecodeInputModel dispatches input decoding based on the inputFormat.
func DecodeInputModel(
	input []byte,
	inputFormat string,
	orderedIDs []string,
	registry *Registry,
) (UnifiedModel, error) {
	switch inputFormat {
	case FormatJSON:
		return decodeJSONInputModel(input, orderedIDs, registry)
	case FormatBinary:
		return DecodeBinaryInputModel(input, orderedIDs, registry)
	default:
		return UnifiedModel{}, fmt.Errorf("%w: %s", ErrInvalidInputFormat, inputFormat)
	}
}

// decodeJSONInputModel attempts to parse canonical unified JSON first, falling back to
// single-analyzer legacy JSON when the canonical parse fails.
func decodeJSONInputModel(
	input []byte,
	orderedIDs []string,
	registry *Registry,
) (UnifiedModel, error) {
	model, err := ParseUnifiedModelJSON(input)
	if err == nil {
		return model, nil
	}

	if len(orderedIDs) != 1 {
		return UnifiedModel{}, ErrLegacyInputAmbiguous
	}

	report := Report{}

	unmarshalErr := json.Unmarshal(input, &report)
	if unmarshalErr != nil {
		return UnifiedModel{}, fmt.Errorf("decode legacy json input: %w", unmarshalErr)
	}

	descriptor, ok := registry.Descriptor(orderedIDs[0])
	if !ok {
		return UnifiedModel{}, fmt.Errorf("%w: %s", ErrUnknownAnalyzerID, orderedIDs[0])
	}

	return NewUnifiedModel([]AnalyzerResult{
		{
			ID:     descriptor.ID,
			Mode:   descriptor.Mode,
			Report: report,
		},
	}), nil
}

// DecodeBinaryInputModel decodes concatenated binary envelopes into a unified model.
// It first tries to interpret a single envelope as canonical unified JSON and falls back
// to per-analyzer legacy decoding when that fails.
func DecodeBinaryInputModel(
	input []byte,
	orderedIDs []string,
	registry *Registry,
) (UnifiedModel, error) {
	payloads, err := decodeBinaryEnvelopes(input)
	if err != nil {
		return UnifiedModel{}, fmt.Errorf("decode binary envelopes: %w", err)
	}

	if len(payloads) == 1 {
		model, parseErr := ParseUnifiedModelJSON(payloads[0])
		if parseErr == nil {
			return model, nil
		}
	}

	if len(payloads) != len(orderedIDs) {
		return UnifiedModel{}, fmt.Errorf(
			"%w: payloads=%d analyzers=%d",
			ErrLegacyBinaryCount,
			len(payloads),
			len(orderedIDs),
		)
	}

	results := make([]AnalyzerResult, 0, len(payloads))

	for i, payload := range payloads {
		report := Report{}

		unmarshalErr := json.Unmarshal(payload, &report)
		if unmarshalErr != nil {
			return UnifiedModel{}, fmt.Errorf("decode binary payload %d: %w", i, unmarshalErr)
		}

		descriptor, ok := registry.Descriptor(orderedIDs[i])
		if !ok {
			return UnifiedModel{}, fmt.Errorf("%w: %s", ErrUnknownAnalyzerID, orderedIDs[i])
		}

		results = append(results, AnalyzerResult{
			ID:     descriptor.ID,
			Mode:   descriptor.Mode,
			Report: report,
		})
	}

	return NewUnifiedModel(results), nil
}

// ---------------------------------------------------------------------------
// Output writing
// ---------------------------------------------------------------------------

// PlotRenderer is a function that renders a UnifiedModel as a plot to the given writer.
// It is provided by the renderer package to avoid import cycles.
type PlotRenderer func(model UnifiedModel, writer io.Writer) error

// plotRendererFn holds the registered plot renderer. Nil until set via RegisterPlotRenderer.
var plotRendererFn PlotRenderer //nolint:gochecknoglobals // package-level registration, set once

// RegisterPlotRenderer sets the package-level plot renderer used by WriteConvertedOutput.
// It is intended to be called from the renderer package's init function.
func RegisterPlotRenderer(fn PlotRenderer) {
	plotRendererFn = fn
}

// SectionRendererFunc generates plot sections from a raw report for a specific analyzer.
type SectionRendererFunc func(report Report) ([]plotpage.Section, error)

// plotSectionRenderers maps analyzer IDs to their plot section generators.
var plotSectionRenderers = make(map[string]SectionRendererFunc) //nolint:gochecknoglobals // package-level registration

// RegisterPlotSections registers a plot section renderer for the given analyzer ID.
func RegisterPlotSections(analyzerID string, fn SectionRendererFunc) {
	plotSectionRenderers[analyzerID] = fn
}

// PlotSectionsFor returns the registered section renderer for an analyzer ID, or nil.
func PlotSectionsFor(analyzerID string) SectionRendererFunc {
	return plotSectionRenderers[analyzerID]
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
		err := encodeBinaryEnvelope(model, writer)
		if err != nil {
			return fmt.Errorf("encode converted binary: %w", err)
		}

		return nil
	case FormatPlot:
		if plotRendererFn == nil {
			return fmt.Errorf("%w: plot renderer not registered", ErrUnsupportedFormat)
		}

		return plotRendererFn(model, writer)
	default:
		return fmt.Errorf("%w: %s", ErrUnsupportedFormat, outputFormat)
	}
}

// ---------------------------------------------------------------------------
// Binary envelope helpers (inlined from reportutil to avoid import cycles)
// ---------------------------------------------------------------------------

const (
	binaryMagic      = "CFB1"
	binaryHeaderSize = 8
)

var (
	errInvalidBinaryEnvelope = errors.New("invalid binary envelope")
	errBinaryPayloadTooLarge = errors.New("binary payload too large")
)

func encodeBinaryEnvelope(value any, writer io.Writer) error {
	payload, err := json.Marshal(value)
	if err != nil {
		return fmt.Errorf("marshal binary payload: %w", err)
	}

	if len(payload) > math.MaxUint32 {
		return fmt.Errorf("%w: %d bytes", errBinaryPayloadTooLarge, len(payload))
	}

	header := make([]byte, binaryHeaderSize)
	copy(header[:4], binaryMagic)
	binary.LittleEndian.PutUint32(header[4:], uint32(len(payload))) //nolint:gosec // bounded by MaxUint32 check above

	_, err = writer.Write(header)
	if err != nil {
		return fmt.Errorf("write binary header: %w", err)
	}

	_, err = writer.Write(payload)
	if err != nil {
		return fmt.Errorf("write binary payload: %w", err)
	}

	return nil
}

func decodeBinaryEnvelope(reader io.Reader) ([]byte, error) {
	header := make([]byte, binaryHeaderSize)

	_, err := io.ReadFull(reader, header)
	if err != nil {
		return nil, errors.Join(errInvalidBinaryEnvelope, err)
	}

	if !bytes.Equal(header[:4], []byte(binaryMagic)) {
		return nil, fmt.Errorf("%w: bad magic", errInvalidBinaryEnvelope)
	}

	payloadLen := binary.LittleEndian.Uint32(header[4:])
	payload := make([]byte, payloadLen)

	_, err = io.ReadFull(reader, payload)
	if err != nil {
		return nil, errors.Join(errInvalidBinaryEnvelope, err)
	}

	return payload, nil
}

func decodeBinaryEnvelopes(data []byte) ([][]byte, error) {
	reader := bytes.NewReader(data)
	payloads := make([][]byte, 0)

	for reader.Len() > 0 {
		payload, err := decodeBinaryEnvelope(reader)
		if err != nil {
			return nil, err
		}

		payloads = append(payloads, payload)
	}

	return payloads, nil
}
