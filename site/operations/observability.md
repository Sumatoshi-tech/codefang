---
title: Observability
description: OpenTelemetry tracing, RED metrics, structured logging, sampling, attribute filtering, and health checks for Codefang.
---

# Observability

Codefang includes built-in OpenTelemetry support for **distributed tracing**,
**RED metrics** (Rate, Errors, Duration), and **structured logging** with
automatic trace context injection. Observability is zero-cost when disabled
and production-ready when enabled.

---

## Quick Start

By default, observability uses no-op providers with zero export overhead. To
enable telemetry export, set the OTLP endpoint:

```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=localhost:4317
codefang run -a history/burndown .
```

For local debugging without a collector, use `--debug-trace`:

```bash
codefang run --debug-trace -a history/burndown .
```

This forces 100% trace sampling and prints the `trace_id` on completion.

---

## Local Development Stack

A pre-configured OTel stack (Jaeger + OTel Collector + Prometheus) is available
via Docker Compose:

```bash
make otel-up       # Start Jaeger, OTel Collector, Prometheus
make demo          # Run analysis with tracing and print trace link
make otel-down     # Stop the stack
```

| Service | URL | Purpose |
|---------|-----|---------|
| Jaeger UI | [http://localhost:16686](http://localhost:16686) | Trace visualization |
| Prometheus | [http://localhost:9090](http://localhost:9090) | Metrics queries |
| OTel Collector | `localhost:4317` (gRPC) | Telemetry ingestion |

---

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `OTEL_EXPORTER_OTLP_ENDPOINT` | gRPC endpoint for OTLP collector | (empty = no-op) |
| `OTEL_EXPORTER_OTLP_HEADERS` | Comma-separated `key=value` gRPC metadata headers | (empty) |
| `OTEL_EXPORTER_OTLP_INSECURE` | Skip TLS verification (`true`/`false`) | `false` |
| `OTEL_TRACES_SAMPLER` | Sampler type (see [Sampling](#sampling)) | (uses config) |
| `OTEL_TRACES_SAMPLER_ARG` | Sampler argument (ratio for ratio-based samplers) | `1.0` |

### CLI Flags

| Flag | Description |
|------|-------------|
| `--debug-trace` | Force 100% sampling, enable debug logging |

### MCP Mode

The MCP server (`codefang mcp`) uses JSON-structured logs on stderr by default.
When `--debug` is enabled, debug-level logging and full trace sampling are
activated:

```bash
codefang mcp --debug
```

### No-Op Mode

When `OTEL_EXPORTER_OTLP_ENDPOINT` is empty (the default), the SDK returns
no-op tracer and meter providers:

- Zero allocation overhead for span creation
- Zero network overhead (no gRPC connections)
- Structured logging still works (trace_id/span_id omitted when no active span)

---

## Span Hierarchy

The entire analysis pipeline is instrumented with hierarchical spans:

```
codefang.run                              (CLI entry point)
|
+-- codefang.init                         (analyzer + repo initialization)
|
+-- codefang.analysis                     (streaming mode wrapper)
|   |
|   +-- codefang.runner.chunk             (per-chunk processing)
|   |   |
|   |   +-- codefang.pipeline             (coordinator pipeline)
|   |   |
|   |   +-- codefang.analyzer.consume     (per-analyzer, sequential)
|   |   |
|   |   +-- codefang.analyzer.fork        (parallel leaf forking)
|   |   |
|   |   +-- codefang.analyzer.merge       (parallel leaf merging)
|   |
|   +-- checkpoint.saved                  (span event)
|   |
|   +-- checkpoint.resumed                (span event)
|
+-- codefang.report                       (output generation)
|
+-- codefang.git.*                        (git operations)
|   |
|   +-- codefang.git.lookup_commit
|   +-- codefang.git.lookup_blob
|   +-- codefang.git.diff_tree
|   +-- codefang.git.log
|
+-- codefang.uast.*                       (UAST operations)
    |
    +-- codefang.uast.parse
    +-- codefang.uast.parse_dsl
    +-- codefang.uast.changes
```

### Span Attributes

| Span | Key Attributes |
|------|---------------|
| `codefang.run` | `error`, `codefang.duration_class`, `codefang.format`, `codefang.memory_sys_mb`, `codefang.path`, `codefang.analyzers`, `codefang.limit` |
| `codefang.init` | `init.commits`, `init.analyzers` |
| `codefang.analysis` | `analysis.chunks`, `analysis.chunk_size`, `analysis.double_buffered`, `analysis.slowest_chunk_ms`, `analysis.total_chunk_ms` |
| `codefang.pipeline` | `commits.count`, `pipeline.workers` |
| `codefang.runner.chunk` | `chunk.size`, `chunk.offset` |
| `codefang.analyzer.consume` | `analyzer.id` |
| `codefang.analyzer.fork` | `fork.workers`, `fork.leaves` |
| `codefang.report` | `report.format`, `report.analyzers` |
| `codefang.git.*` | `git.hash`, `git.operation` |
| `codefang.uast.parse` | `uast.language`, `file.size` |
| `mcp.*` | `mcp.tool` |

Pipeline cache statistics are also recorded on the analysis span:

| Attribute | Description |
|-----------|-------------|
| `analysis.cache.blob.hits` | Blob cache hit count |
| `analysis.cache.blob.misses` | Blob cache miss count |
| `analysis.cache.blob.hit_pct` | Blob cache hit percentage |
| `analysis.cache.diff.hits` | Diff cache hit count |
| `analysis.cache.diff.misses` | Diff cache miss count |
| `analysis.cache.diff.hit_pct` | Diff cache hit percentage |
| `analysis.pipeline.dominant` | Slowest pipeline stage (`blob`, `diff`, or `uast`) |

---

## RED Metrics

When connected to an OTLP collector, the following metrics are exported:

### Core RED Metrics

| Metric | Type | Unit | Description |
|--------|------|------|-------------|
| `codefang.requests.total` | Counter | `{request}` | Total number of requests (labeled by `op`, `status`) |
| `codefang.request.duration.seconds` | Histogram | `s` | Request duration (labeled by `op`, `status`) |
| `codefang.errors.total` | Counter | `{error}` | Total number of errors (labeled by `op`) |
| `codefang.inflight.requests` | UpDownCounter | `{request}` | Currently in-flight requests (labeled by `op`) |

### Analysis Metrics

| Metric | Type | Unit | Description |
|--------|------|------|-------------|
| `codefang.analysis.commits.total` | Counter | `{commit}` | Total commits analyzed |
| `codefang.analysis.chunks.total` | Counter | `{chunk}` | Total chunks processed |
| `codefang.analysis.chunk.duration.seconds` | Histogram | `s` | Per-chunk processing duration |
| `codefang.analysis.cache.hits.total` | Counter | `{hit}` | Cache hits (labeled by `cache`: `blob` or `diff`) |
| `codefang.analysis.cache.misses.total` | Counter | `{miss}` | Cache misses (labeled by `cache`: `blob` or `diff`) |

### Histogram Buckets

Duration histograms use these bucket boundaries (in seconds), covering
sub-second static checks through multi-minute history pipelines:

```
0.01, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10, 30, 60, 120, 300, 600
```

---

## Sampling

Trace sampling is configurable with the following precedence (highest first):

1. **`--debug-trace` CLI flag**: Forces 100% sampling (AlwaysOn).
2. **`OTEL_TRACES_SAMPLER` env var**: Standard OTel sampler selection.
3. **`Config.SampleRatio`**: Ratio-based sampling via config file.
4. **Default**: `ParentBased(AlwaysOn)` -- samples all root spans, respects parent decisions.

### Supported Sampler Values

| `OTEL_TRACES_SAMPLER` | Description |
|------------------------|-------------|
| `always_on` | Sample every span |
| `always_off` | Drop every span |
| `traceidratio` | Sample based on trace ID ratio (`OTEL_TRACES_SAMPLER_ARG`) |
| `parentbased_always_on` | Parent-based with always-on root sampler |
| `parentbased_always_off` | Parent-based with always-off root sampler |
| `parentbased_traceidratio` | Parent-based with ratio root sampler |

### Example: 10% Sampling

```bash
export OTEL_TRACES_SAMPLER=traceidratio
export OTEL_TRACES_SAMPLER_ARG=0.1
codefang run -a history/burndown .
```

### Production Recommendation

Use `parentbased_traceidratio` with a low ratio (1-5%) to limit trace volume
while preserving distributed context propagation:

```bash
export OTEL_TRACES_SAMPLER=parentbased_traceidratio
export OTEL_TRACES_SAMPLER_ARG=0.01
```

### Tail Sampling (Collector-Side)

Head-based sampling decides at trace start whether to sample. This means slow
or errored traces may be dropped. **Tail sampling** defers the decision until
the trace completes, keeping all interesting traces.

Codefang root spans carry attributes that enable OTel Collector tail-sampling
policies:

| Attribute | Type | Values | Purpose |
|-----------|------|--------|---------|
| `error` | bool | `true`/`false` | Keep all errored traces |
| `codefang.duration_class` | string | `fast`/`normal`/`slow` | Keep slow traces |

**Duration class thresholds:**

| Class | Duration |
|-------|----------|
| `fast` | < 10s |
| `normal` | 10s -- 60s |
| `slow` | > 60s |

**Recommended OTel Collector tail-sampling config:**

```yaml
processors:
  tail_sampling:
    decision_wait: 30s
    policies:
      # Keep all traces with errors.
      - name: errors
        type: boolean_attribute
        boolean_attribute:
          key: error
          value: true

      # Keep all slow traces (>60s).
      - name: slow-traces
        type: string_attribute
        string_attribute:
          key: codefang.duration_class
          values: [slow]

      # Sample 5% of remaining traces.
      - name: probabilistic
        type: probabilistic
        probabilistic:
          sampling_percentage: 5
```

This ensures 100% of error and slow traces are retained while keeping a
representative sample of normal traffic.

---

## Attribute Filter

All exported spans pass through an `AttributeFilter` span processor that
enforces an allow-list of attribute key prefixes. This prevents PII and
high-cardinality data from reaching the collector.

### Allowed Prefixes

| Prefix | Covers |
|--------|--------|
| `codefang.` | All Codefang-specific attributes |
| `error.` | Error type and source classification |
| `http.` | HTTP semantic conventions |
| `mcp.` | MCP tool attributes |
| `analysis.` | Analysis-specific attributes |
| `analyzer.` | Per-analyzer attributes |
| `chunk.` | Chunk processing attributes |
| `init.` | Initialization attributes |
| `pipeline.` | Pipeline stage attributes |
| `report.` | Report generation attributes |
| `runner.` | Runner attributes |
| `cache` | Cache statistics |

### Blocked Keys

| Pattern | Reason |
|---------|--------|
| `user.*` | PII -- user-identifying information |
| `email` | PII -- email addresses |
| `request.body` | Potentially sensitive request content |
| `response.body` | Potentially sensitive response content |

When `--debug-trace` is enabled, blocked attributes are logged as warnings
to stderr, helping developers identify instrumentation issues during
development.

---

## HTTP Middleware

The `observability.HTTPMiddleware` wraps HTTP handlers with:

- **W3C trace context extraction**: Reads `traceparent`/`tracestate`/`baggage` headers from incoming requests.
- **Server span creation**: Creates a child span under the extracted parent context.
- **Status code capture**: Records HTTP response status on the span.
- **Error detection**: Sets error status on 5xx responses.
- **Panic recovery**: Catches panics, records the stack trace as a span event, and returns 500.
- **Access logging**: Structured log line with method, path, status, and duration.

This enables distributed tracing across service boundaries: upstream services
that send a `traceparent` header have their traces continued by Codefang.

---

## Health Checks

For server mode deployments (Kubernetes), the observability package provides
HTTP handlers:

### Liveness Probe

```
GET /healthz  ->  HTTP 200  {"status": "ok"}
```

Always returns 200. Use as a Kubernetes liveness probe.

### Readiness Probe

```
GET /readyz   ->  HTTP 200  {"status": "ok"}
              ->  HTTP 503  {"status": "unavailable"}
```

Runs all registered `ReadyCheck` functions. Returns 200 if all pass, 503
if any fail. Use as a Kubernetes readiness probe.

```go
mux.Handle("/healthz", observability.HealthHandler())
mux.Handle("/readyz", observability.ReadyHandler(dbCheck, cacheCheck))
```

### Prometheus Metrics Endpoint

```
GET /metrics  ->  Prometheus exposition format
```

Creates an OTel Prometheus exporter with an independent registry. Includes
OTel SDK `target_info` and all custom metrics registered through the meter
provider.

```go
metricsHandler, err := observability.PrometheusHandler()
mux.Handle("/metrics", metricsHandler)
```

---

## Structured Logging

All framework-level log messages use `log/slog` with instance loggers (not
the global `slog.Default()`). The `TracingHandler` automatically injects
trace context into every log record:

| Injected Field | Source |
|----------------|--------|
| `trace_id` | Active span's trace ID |
| `span_id` | Active span's span ID |
| `service` | Service name from config |
| `mode` | Operating mode (`cli`, `mcp`) |
| `env` | Deployment environment |

### Log Modes

| Mode | Handler | Use case |
|------|---------|----------|
| CLI | `slog.TextHandler` | Human-readable console output |
| MCP | `slog.JSONHandler` | Machine-parseable structured logs |
| Debug | Level `DEBUG` | Verbose output for troubleshooting |

### Linter Enforcement

The following linters enforce observability correctness in `.golangci.yml`:

| Linter | Rule |
|--------|------|
| `spancheck` | Verifies proper span lifecycle (End calls, error recording) |
| `sloglint` | Enforces `no-global: all` (use instance loggers) and `context: scope` (use `InfoContext` when ctx is available) |
| `contextcheck` | Flags `context.Background()` instead of propagating parent context |

---

## Deep Context Propagation

All hot-path `context.Background()` calls have been eliminated. Context flows
end-to-end from the CLI entry point through every layer:

### Propagation Path

```
CLI (codefang.run)
 |
 +-> framework.RunStreaming(ctx)
      |
      +-> processChunks*(ctx)
           |
           +-> runner.ProcessChunk(ctx)
                |
                +-> coordinator.Process(ctx)
                |    |
                |    +-> worker.handle(ctx)
                |         |
                |         +-> gitlib.TreeDiff(ctx)
                |         +-> gitlib.LookupBlob(ctx)
                |         +-> uast.Parse(ctx)
                |
                +-> analyzer.Consume(ctx)
                     |
                     +-> plumbing.Changes(ctx)
                          |
                          +-> parser.Parse(ctx)
```

### Key Interfaces

The `Consume` method on history analyzers accepts context:

```go
type HistoryProcessor interface {
    Initialize(repository *gitlib.Repository) error
    Consume(ctx context.Context, ac *Context) error
    Finalize() (Report, error)
}
```

The UAST parser accepts context for cancellation support:

```go
type LanguageParser interface {
    Parse(ctx context.Context, filename string, content []byte) (*node.Node, error)
}
```

Git operations accept context for tracing:

```go
func (r *Repository) LookupCommit(ctx context.Context, hash plumbing.Hash) (*Commit, error)
func (r *Repository) LookupBlob(ctx context.Context, hash plumbing.Hash) (*Blob, error)
func TreeDiff(ctx context.Context, repo *Repository, oldTree, newTree *Tree) (Changes, error)
```

---

## Integration Points Summary

| Component | Tracing | Metrics | Logging |
|-----------|---------|---------|---------|
| CLI `codefang run` | Root span + child spans | RED for `cli.run` | Progress logging |
| MCP Server | Per-tool spans with `SpanKindServer` | RED per tool | JSON structured logs |
| Framework Pipeline | Chunk + analyzer spans | Analysis metrics | Chunk progress logs |
| Coordinator | Pipeline span | Cache hit/miss stats | Worker diagnostics |
| Git Operations | Per-operation spans | -- | Error logging |
| UAST Parser | Parse spans | -- | Warning logging |
| HTTP Server | Per-request spans | -- | Access logs |
