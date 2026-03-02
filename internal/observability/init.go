package observability

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"os"
	"strconv"
	"strings"
	"time"

	"go.opentelemetry.io/otel"
	"go.opentelemetry.io/otel/attribute"
	"go.opentelemetry.io/otel/exporters/otlp/otlpmetric/otlpmetricgrpc"
	"go.opentelemetry.io/otel/exporters/otlp/otlptrace/otlptracegrpc"
	"go.opentelemetry.io/otel/metric"
	noopmetric "go.opentelemetry.io/otel/metric/noop"
	"go.opentelemetry.io/otel/propagation"
	sdkmetric "go.opentelemetry.io/otel/sdk/metric"
	"go.opentelemetry.io/otel/sdk/resource"
	sdktrace "go.opentelemetry.io/otel/sdk/trace"
	semconv "go.opentelemetry.io/otel/semconv/v1.26.0"
	"go.opentelemetry.io/otel/trace"
	nooptrace "go.opentelemetry.io/otel/trace/noop"
)

const (
	tracerName = "codefang"
	meterName  = "codefang"

	// envTracesSampler is the standard OTel env var for selecting a sampler.
	envTracesSampler = "OTEL_TRACES_SAMPLER"

	// envTracesSamplerArg is the standard OTel env var for sampler arguments.
	envTracesSamplerArg = "OTEL_TRACES_SAMPLER_ARG"

	// samplerAlwaysOn selects the always-on sampler.
	samplerAlwaysOn = "always_on"

	// samplerAlwaysOff selects the always-off sampler.
	samplerAlwaysOff = "always_off"

	// samplerTraceIDRatio selects the TraceIDRatio sampler.
	samplerTraceIDRatio = "traceidratio"

	// samplerParentBasedAlwaysOn selects parent-based with always-on root.
	samplerParentBasedAlwaysOn = "parentbased_always_on"

	// samplerParentBasedAlwaysOff selects parent-based with always-off root.
	samplerParentBasedAlwaysOff = "parentbased_always_off"

	// samplerParentBasedTraceIDRatio selects parent-based with TraceIDRatio root.
	samplerParentBasedTraceIDRatio = "parentbased_traceidratio"
)

// Providers holds the initialized observability providers.
type Providers struct {
	// Tracer is the named tracer for creating spans.
	Tracer trace.Tracer

	// Meter is the named meter for creating instruments.
	Meter metric.Meter

	// Logger is the context-aware structured logger.
	Logger *slog.Logger

	// Shutdown flushes all pending telemetry and releases resources.
	// Must be called before process exit.
	Shutdown func(ctx context.Context) error
}

// Init initializes OpenTelemetry tracing, metrics, and structured logging.
// When OTLPEndpoint is empty, no-op providers are used with zero export overhead.
func Init(cfg Config) (Providers, error) {
	ctx := context.Background()

	res, err := buildResource(cfg)
	if err != nil {
		return Providers{}, err
	}

	tp, tpShutdown, err := buildTracerProvider(ctx, cfg, res)
	if err != nil {
		return Providers{}, fmt.Errorf("build tracer provider: %w", err)
	}

	mp, mpShutdown, err := buildMeterProvider(ctx, cfg, res)
	if err != nil {
		shutdownErr := tpShutdown(ctx)

		return Providers{}, errors.Join(fmt.Errorf("build meter provider: %w", err), shutdownErr)
	}

	if cfg.OTLPEndpoint != "" && !cfg.TraceVerbose {
		tp = NewFilteringTracerProvider(tp)
	}

	otel.SetTracerProvider(tp)
	otel.SetMeterProvider(mp)
	otel.SetTextMapPropagator(propagation.NewCompositeTextMapPropagator(
		propagation.TraceContext{},
		propagation.Baggage{},
	))

	logger := buildLogger(cfg)

	shutdown := func(shutdownCtx context.Context) error {
		timeoutDur := time.Duration(cfg.ShutdownTimeoutSec) * time.Second
		if timeoutDur <= 0 {
			timeoutDur = time.Duration(defaultShutdownTimeoutSec) * time.Second
		}

		deadlineCtx, cancel := context.WithTimeout(shutdownCtx, timeoutDur)
		defer cancel()

		return errors.Join(tpShutdown(deadlineCtx), mpShutdown(deadlineCtx))
	}

	return Providers{
		Tracer:   tp.Tracer(tracerName),
		Meter:    mp.Meter(meterName),
		Logger:   logger,
		Shutdown: shutdown,
	}, nil
}

func buildResource(cfg Config) (*resource.Resource, error) {
	attrs := []resource.Option{
		resource.WithAttributes(
			semconv.ServiceName(cfg.ServiceName),
		),
	}

	if cfg.ServiceVersion != "" {
		attrs = append(attrs, resource.WithAttributes(semconv.ServiceVersion(cfg.ServiceVersion)))
	}

	if cfg.Environment != "" {
		attrs = append(attrs, resource.WithAttributes(semconv.DeploymentEnvironment(cfg.Environment)))
	}

	if cfg.Mode != "" {
		attrs = append(attrs, resource.WithAttributes(attribute.String("app.mode", string(cfg.Mode))))
	}

	res, err := resource.New(context.Background(), attrs...)
	if err != nil {
		return nil, fmt.Errorf("build otel resource: %w", err)
	}

	return res, nil
}

type shutdownFunc func(ctx context.Context) error

func noopShutdown(_ context.Context) error { return nil }

func buildTracerProvider(
	ctx context.Context, cfg Config, res *resource.Resource,
) (trace.TracerProvider, shutdownFunc, error) {
	if cfg.OTLPEndpoint == "" {
		return nooptrace.NewTracerProvider(), noopShutdown, nil
	}

	opts := []otlptracegrpc.Option{
		otlptracegrpc.WithEndpoint(cfg.OTLPEndpoint),
	}

	if cfg.OTLPInsecure {
		opts = append(opts, otlptracegrpc.WithInsecure())
	}

	if len(cfg.OTLPHeaders) > 0 {
		opts = append(opts, otlptracegrpc.WithHeaders(cfg.OTLPHeaders))
	}

	exporter, err := otlptracegrpc.New(ctx, opts...)
	if err != nil {
		return nil, nil, fmt.Errorf("create trace exporter: %w", err)
	}

	batcher := sdktrace.NewBatchSpanProcessor(exporter)

	var filterLogger *slog.Logger
	if cfg.DebugTrace {
		filterLogger = slog.New(slog.NewTextHandler(os.Stderr, &slog.HandlerOptions{Level: slog.LevelWarn}))
	}

	tp := sdktrace.NewTracerProvider(
		sdktrace.WithSpanProcessor(NewAttributeFilter(batcher, filterLogger)),
		sdktrace.WithResource(res),
		sdktrace.WithSampler(selectSampler(cfg)),
	)

	return tp, tp.Shutdown, nil
}

func selectSampler(cfg Config) sdktrace.Sampler {
	if cfg.DebugTrace {
		return sdktrace.AlwaysSample()
	}

	if envSampler := os.Getenv(envTracesSampler); envSampler != "" {
		return envSampler2Sampler(envSampler, os.Getenv(envTracesSamplerArg))
	}

	if cfg.SampleRatio > 0 {
		return sdktrace.ParentBased(sdktrace.TraceIDRatioBased(cfg.SampleRatio))
	}

	return sdktrace.ParentBased(sdktrace.AlwaysSample())
}

func envSampler2Sampler(name, arg string) sdktrace.Sampler {
	switch name {
	case samplerAlwaysOn:
		return sdktrace.AlwaysSample()
	case samplerAlwaysOff:
		return sdktrace.NeverSample()
	case samplerTraceIDRatio:
		return sdktrace.TraceIDRatioBased(parseRatio(arg))
	case samplerParentBasedAlwaysOn:
		return sdktrace.ParentBased(sdktrace.AlwaysSample())
	case samplerParentBasedAlwaysOff:
		return sdktrace.ParentBased(sdktrace.NeverSample())
	case samplerParentBasedTraceIDRatio:
		return sdktrace.ParentBased(sdktrace.TraceIDRatioBased(parseRatio(arg)))
	default:
		return sdktrace.ParentBased(sdktrace.AlwaysSample())
	}
}

func buildLogger(cfg Config) *slog.Logger {
	handlerOpts := &slog.HandlerOptions{Level: cfg.LogLevel}

	var inner slog.Handler
	if cfg.LogJSON {
		inner = slog.NewJSONHandler(os.Stderr, handlerOpts)
	} else {
		inner = slog.NewTextHandler(os.Stderr, handlerOpts)
	}

	handler := NewTracingHandler(inner, cfg.ServiceName, cfg.Environment, cfg.Mode)

	return slog.New(handler)
}

func buildMeterProvider(
	ctx context.Context,
	cfg Config,
	res *resource.Resource,
) (metric.MeterProvider, shutdownFunc, error) {
	if cfg.OTLPEndpoint == "" {
		return noopmetric.NewMeterProvider(), noopShutdown, nil
	}

	opts := []otlpmetricgrpc.Option{
		otlpmetricgrpc.WithEndpoint(cfg.OTLPEndpoint),
	}

	if cfg.OTLPInsecure {
		opts = append(opts, otlpmetricgrpc.WithInsecure())
	}

	if len(cfg.OTLPHeaders) > 0 {
		opts = append(opts, otlpmetricgrpc.WithHeaders(cfg.OTLPHeaders))
	}

	exporter, err := otlpmetricgrpc.New(ctx, opts...)
	if err != nil {
		return nil, nil, fmt.Errorf("create metric exporter: %w", err)
	}

	mp := sdkmetric.NewMeterProvider(
		sdkmetric.WithReader(sdkmetric.NewPeriodicReader(exporter)),
		sdkmetric.WithResource(res),
	)

	return mp, mp.Shutdown, nil
}

// ParseOTLPHeaders parses an OTLP headers string in "key=value,key=value"
// format. Returns nil for empty or invalid input.
func ParseOTLPHeaders(raw string) map[string]string {
	if raw == "" {
		return nil
	}

	result := make(map[string]string)

	for pair := range strings.SplitSeq(raw, ",") {
		k, v, ok := strings.Cut(strings.TrimSpace(pair), "=")
		if !ok {
			continue
		}

		result[strings.TrimSpace(k)] = strings.TrimSpace(v)
	}

	if len(result) == 0 {
		return nil
	}

	return result
}

func parseRatio(s string) float64 {
	if s == "" {
		return 1.0
	}

	ratio, err := strconv.ParseFloat(s, 64)
	if err != nil {
		return 1.0
	}

	return ratio
}
