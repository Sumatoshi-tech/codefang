package observability

import (
	"context"
	"fmt"
	"log/slog"
	"strings"

	"go.opentelemetry.io/otel/attribute"
	sdktrace "go.opentelemetry.io/otel/sdk/trace"
)

// allowedPrefixes are attribute key prefixes that pass through the filter.
// Any key starting with one of these prefixes is allowed.
var allowedPrefixes = []string{
	"codefang.",
	"error.",
	"http.",
	"mcp.",
	"analysis.",
	"analyzer.",
	"chunk.",
	"init.",
	"pipeline.",
	"report.",
	"runner.",
	"cache",
	"worker_index",
	"stall_count",
	"request_type",
	"stack",
	"hits",
	"misses",
}

// blockedPrefixes are attribute key prefixes that are always stripped.
var blockedPrefixes = []string{
	"user.",
}

// blockedKeys are exact attribute keys that are always stripped.
var blockedKeys = map[string]bool{
	"email":         true,
	"request.body":  true,
	"response.body": true,
}

// attributeFilter is a SpanProcessor that strips blocked/unknown attributes
// before forwarding to a delegate processor. It enforces an allow-list to
// prevent PII and high-cardinality data from reaching the exporter.
type attributeFilter struct {
	delegate sdktrace.SpanProcessor
	logger   *slog.Logger
}

// NewAttributeFilter returns a SpanProcessor that filters span attributes.
// Allowed attributes pass through; blocked attributes (user.*, email,
// request.body, response.body) are stripped. When logger is non-nil, blocked
// attributes are logged as warnings (intended for dev mode).
func NewAttributeFilter(delegate sdktrace.SpanProcessor, logger *slog.Logger) sdktrace.SpanProcessor {
	return &attributeFilter{delegate: delegate, logger: logger}
}

// OnStart delegates to the wrapped processor.
func (f *attributeFilter) OnStart(parent context.Context, s sdktrace.ReadWriteSpan) {
	f.delegate.OnStart(parent, s)
}

// OnEnd filters attributes, then delegates to the wrapped processor.
func (f *attributeFilter) OnEnd(s sdktrace.ReadOnlySpan) {
	// ReadOnlySpan attributes cannot be mutated; wrap with filtered view.
	f.delegate.OnEnd(&filteredSpan{ReadOnlySpan: s, filter: f})
}

// Shutdown delegates to the wrapped processor.
func (f *attributeFilter) Shutdown(ctx context.Context) error {
	err := f.delegate.Shutdown(ctx)
	if err != nil {
		return fmt.Errorf("attribute filter shutdown: %w", err)
	}

	return nil
}

// ForceFlush delegates to the wrapped processor.
func (f *attributeFilter) ForceFlush(ctx context.Context) error {
	err := f.delegate.ForceFlush(ctx)
	if err != nil {
		return fmt.Errorf("attribute filter flush: %w", err)
	}

	return nil
}

func (f *attributeFilter) isAllowed(key string) bool {
	if blockedKeys[key] {
		f.warn(key)

		return false
	}

	for _, prefix := range blockedPrefixes {
		if strings.HasPrefix(key, prefix) {
			f.warn(key)

			return false
		}
	}

	for _, prefix := range allowedPrefixes {
		if strings.HasPrefix(key, prefix) {
			return true
		}

		if key == prefix {
			return true
		}
	}

	// Allow OTel semantic convention keys (e.g. "error", "service.name").
	if key == "error" {
		return true
	}

	f.warn(key)

	return false
}

func (f *attributeFilter) warn(key string) {
	if f.logger != nil {
		f.logger.Warn("attribute blocked by filter", "key", key)
	}
}

// filteredSpan wraps a ReadOnlySpan and returns only allowed attributes.
type filteredSpan struct {
	sdktrace.ReadOnlySpan

	filter *attributeFilter
}

// Attributes returns only the allowed attributes.
func (s *filteredSpan) Attributes() []attribute.KeyValue {
	orig := s.ReadOnlySpan.Attributes()
	filtered := make([]attribute.KeyValue, 0, len(orig))

	for _, kv := range orig {
		if s.filter.isAllowed(string(kv.Key)) {
			filtered = append(filtered, kv)
		}
	}

	return filtered
}
