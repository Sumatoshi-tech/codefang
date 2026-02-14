# Observability

Codefang includes built-in OpenTelemetry support for distributed tracing, RED
metrics, and structured logging with trace context injection.

## Quick Start

By default, observability uses no-op providers with zero export overhead. To
enable telemetry export, set the OTLP endpoint:

```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=localhost:4317
codefang run -a history/burndown .
```

For local debugging without an OTLP collector, use the `--debug-trace` flag to
force 100% trace sampling and print the `trace_id` on completion:

```bash
codefang run --debug-trace -a history/burndown .
```

## Configuration

### Environment Variables

| Variable | Description | Default |
|---|---|---|
| `OTEL_EXPORTER_OTLP_ENDPOINT` | gRPC endpoint for OTLP collector | (empty = no-op) |
| `OTEL_EXPORTER_OTLP_HEADERS` | Comma-separated `key=value` gRPC metadata headers | (empty) |
| `OTEL_TRACES_SAMPLER` | Sampler type (see Sampling section) | (uses config) |
| `OTEL_TRACES_SAMPLER_ARG` | Sampler argument (ratio for ratio-based samplers) | `1.0` |

### CLI Flags

| Flag | Description |
|---|---|
| `--debug-trace` | Force 100% sampling, print `trace_id` on completion |

### MCP Mode

The MCP server (`codefang mcp`) uses JSON-structured logs on stderr by default.
When `--debug` is enabled, debug-level logging and full trace sampling are
activated.

## Architecture

### Package Layout

```
pkg/observability/
  config.go      - Config struct with defaults
  init.go        - Init()/Shutdown, provider construction, sampler resolution
  logger.go      - TracingHandler (slog.Handler with trace context)
  metrics.go     - REDMetrics (Rate, Errors, Duration)
  middleware.go  - HTTP middleware with W3C trace context extraction
  health.go      - /healthz (liveness) and /readyz (readiness) handlers
  prometheus.go  - /metrics Prometheus scrape endpoint
```

### Providers

`observability.Init(cfg)` returns a `Providers` struct containing:

- **Tracer** (`trace.Tracer`) - for creating spans
- **Meter** (`metric.Meter`) - for recording metrics
- **Logger** (`*slog.Logger`) - structured logger with trace context injection
- **Shutdown** (`func(ctx) error`) - flushes pending telemetry

### No-Op Mode

When `OTEL_EXPORTER_OTLP_ENDPOINT` is empty (default), the SDK returns no-op
tracer and meter providers. This means:

- Zero allocation overhead for span creation
- Zero network overhead (no gRPC connections)
- Structured logging still works (trace_id/span_id omitted when no active span)

### Structured Logging

All framework-level log messages use `log/slog` with instance loggers (not the
global `slog.Default()`). The `TracingHandler` automatically injects:

- `trace_id` and `span_id` from the active span context
- `service`, `mode`, and `env` metadata

The `sloglint` linter enforces `no-global: all` and `context: scope` rules.

### RED Metrics

When connected to an OTLP collector, the following metrics are exported:

| Metric | Type | Description |
|---|---|---|
| `codefang.requests.total` | Counter | Total requests |
| `codefang.request.duration.seconds` | Histogram | Request duration |
| `codefang.errors.total` | Counter | Total errors |
| `codefang.inflight.requests` | UpDownCounter | In-flight requests |

### HTTP Middleware

`observability.HTTPMiddleware(tracer, handler)` wraps an `http.Handler` to:

- Extract W3C `traceparent`/`tracestate` headers from incoming requests
- Create a child server span under the extracted parent context
- Capture HTTP status codes
- Set error status on 5xx responses

This enables distributed tracing across service boundaries: upstream services
that send a `traceparent` header will have their traces continued by Codefang.

## Trace Instrumentation

The entire analysis pipeline is instrumented with OpenTelemetry spans,
providing end-to-end visibility from CLI invocation through to individual
analyzer execution.

### Span Hierarchy

```
codefang.run                          (CLI entry point)
├── codefang.runner.run               (full run with init+process+finalize)
│   ├── codefang.pipeline             (coordinator pipeline orchestration)
│   ├── codefang.runner.chunk         (per-chunk processing)
│   │   ├── codefang.analyzer.consume     (per-analyzer, sequential)
│   │   ├── codefang.analyzer.fork        (parallel leaf forking)
│   │   └── codefang.analyzer.merge       (parallel leaf merging)
│   ├── codefang.runner.init          (analyzer initialization)
│   └── codefang.runner.finalize      (report generation)
├── codefang.streaming.run            (streaming mode wrapper)
│   └── codefang.streaming.chunk      (streaming chunk)
├── codefang.git.*                    (git operations)
│   ├── codefang.git.lookup_commit
│   ├── codefang.git.lookup_blob
│   ├── codefang.git.diff_tree
│   └── codefang.git.log
└── codefang.uast.*                   (UAST operations)
    ├── codefang.uast.parse
    ├── codefang.uast.parse_dsl
    └── codefang.uast.changes
```

### Span Attributes

| Span | Attributes |
|---|---|
| `codefang.runner.run` | `commits.count` |
| `codefang.pipeline` | `commits.count`, `pipeline.workers` |
| `codefang.runner.chunk` | `chunk.size`, `chunk.offset` |
| `codefang.analyzer.consume` | `analyzer.id` |
| `codefang.analyzer.fork` | `fork.workers`, `fork.leaves` |
| `codefang.git.*` | `git.hash`, `git.operation` |
| `codefang.uast.parse` | `uast.language`, `file.size` |
| `mcp.{tool}` | `mcp.tool` |

All spans record errors via `span.RecordError(err)` and set error status
via `span.SetStatus(codes.Error, ...)` on failure.

## Integration Points

### CLI (`codefang run`)

The `run` command initializes observability via `--debug-trace` flag and
`OTEL_EXPORTER_OTLP_ENDPOINT` env var. A root span `"codefang.run"` wraps
the entire pipeline execution. RED metrics are recorded for `cli.run` with
status and duration.

### MCP Server (`codefang mcp`)

The MCP server initializes observability with `ModeMCP` and JSON logging.
Each tool invocation is wrapped with:

- **`withTracing`** — creates a root span (`mcp.codefang_analyze`, etc.)
  with `SpanKindServer` and includes `trace_id` in the response content
  when the span is sampled.
- **`withMetrics`** — records RED metrics (`mcp.codefang_analyze`, etc.)
  with request rate, error rate, duration, and in-flight gauge.

### Framework Pipeline

`framework.StreamingConfig` accepts:
- `Logger *slog.Logger` — pipeline-level logging (chunk processing,
  checkpoint operations). When nil, a discard logger is used.
- `Metrics *observability.REDMetrics` — for recording `streaming.chunk`
  metrics. When nil, metrics are skipped.

## Sampling

Trace sampling is configurable with the following precedence (highest first):

1. `--debug-trace` CLI flag: forces 100% sampling (AlwaysOn)
2. `OTEL_TRACES_SAMPLER` env var: standard OTel sampler selection
3. `Config.SampleRatio`: ratio-based sampling via config file
4. Default: `ParentBased(AlwaysOn)` — samples all root spans, respects parent decisions

### Supported OTEL_TRACES_SAMPLER Values

| Value | Description |
|---|---|
| `always_on` | Sample every span |
| `always_off` | Drop every span |
| `traceidratio` | Sample based on trace ID ratio (`OTEL_TRACES_SAMPLER_ARG`) |
| `parentbased_always_on` | Parent-based with always-on root sampler |
| `parentbased_always_off` | Parent-based with always-off root sampler |
| `parentbased_traceidratio` | Parent-based with ratio root sampler |

Example:

```bash
export OTEL_TRACES_SAMPLER=traceidratio
export OTEL_TRACES_SAMPLER_ARG=0.1
codefang run -a history/burndown .
```

## Server Mode Endpoints

For server mode deployments (k8s), the `pkg/observability` package provides
HTTP handlers for operational endpoints. Consumers mount these on their mux.

### Health Checks

```go
mux.Handle("/healthz", observability.HealthHandler())
mux.Handle("/readyz", observability.ReadyHandler(dbCheck, cacheCheck))
```

- `/healthz` — liveness probe, always returns HTTP 200 with `{"status":"ok"}`
- `/readyz` — readiness probe, runs all `ReadyCheck` functions; returns
  HTTP 200 if all pass, HTTP 503 with `{"status":"unavailable"}` if any fail

`ReadyCheck` is `func(ctx context.Context) error` — return nil for healthy,
error for unhealthy.

### Prometheus Metrics

```go
metricsHandler, err := observability.PrometheusHandler()
mux.Handle("/metrics", metricsHandler)
```

Creates an OTel Prometheus exporter with an independent registry. Serves
metrics in Prometheus exposition format at `/metrics`. Includes OTel SDK
`target_info` and any custom metrics registered through the meter provider.

## Linter Support

The following linters enforce observability correctness in `.golangci.yml`:

- `spancheck` — verifies proper span lifecycle (End() calls, error recording)
- `sloglint` — enforces `no-global: all` (no `slog.Info()`, use instance
  loggers) and `context: scope` (use `InfoContext` when `ctx` is in scope)
- `contextcheck` — flags functions that create `context.Background()`
  instead of propagating a parent context

## Deep Context Propagation

All hot-path `context.Background()` calls have been eliminated. Context flows
end-to-end from the framework coordinator through every layer:

### Consume Interface

The `HistoryProcessor.Consume` method accepts `context.Context` as its first
parameter:

```go
type HistoryProcessor interface {
    Initialize(repository *gitlib.Repository) error
    Consume(ctx context.Context, ac *Context) error
    Finalize() (Report, error)
}
```

The framework's `consumeAnalyzers` creates per-analyzer OTel spans and passes
the span context to each `Consume` call.

### Worker & Pipeline Context

Worker request types (`TreeDiffRequest`, `BlobBatchRequest`, `DiffBatchRequest`)
carry a `Ctx context.Context` field for channel-transported context. The UAST
pipeline threads context from `Process(ctx)` through `startWorkers` →
`parseCommitChanges` → `parseBlob` → `Parser.Parse(ctx)`.

### Plumbing Analyzer Internals

Plumbing analyzers propagate context through their internal call chains:

- **TreeDiff**: `Consume(ctx)` → `computeTreeDiff(ctx)` → `diffTrees(ctx)`,
  `filterChanges(ctx)` → `checkLanguage(ctx)`
- **BlobCache**: `Consume(ctx)` → `consumeParallel(ctx)` →
  `handleInsert/Delete/Modify(ctx)`
- **UASTChanges**: `Consume(ctx)` stores context for lazy `Changes()` →
  `changesSequential/Parallel(ctx)` → `parseBlob(ctx)` → `Parser.Parse(ctx)`

### Gitlib Public API

Gitlib types provide `*Context(ctx)` variants that accept `context.Context`:

- `Commit.FilesContext(ctx)` / `Tree.FilesContext(ctx)`
- `File.ContentsContext(ctx)` / `File.BlobContext(ctx)`

The zero-argument methods (`Files()`, `Contents()`, `Blob()`) delegate to
their `*Context` variants with `context.Background()` for backward
compatibility.
