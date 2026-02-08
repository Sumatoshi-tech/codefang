# Metrics-First Pipeline Architecture

This document describes the metrics-first architecture used by Codefang for all analyzer output. The key principle is that **metrics are the single source of truth** for all report generation.

## Overview

The metrics-first pipeline ensures consistent, structured output across all formats (JSON, YAML, Plot) by routing all data through computed metrics. Raw analyzer data is never directly serialized to output.

```
Before (Legacy):  Raw Data ──┬──► JSON
                             ├──► YAML
                             └──► Plot

After (Current):  Raw Data ──► Metrics ──┬──► JSON
                                         ├──► YAML
                                         └──► Plot
```

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           LAYER 1: INGEST                               │
├─────────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────────────┐  │
│  │   Git History   │  │  UAST Parsing   │  │  Other Sources (future) │  │
│  │  (history cmd)  │  │  (analyze cmd)  │  │                         │  │
│  └────────┬────────┘  └────────┬────────┘  └────────────┬────────────┘  │
│           │                    │                        │               │
│           └────────────────────┼────────────────────────┘               │
│                                ▼                                        │
│                        Raw Analyzer Data                                │
│                   (hercules output, UAST nodes)                         │
└────────────────────────────────┬────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                        LAYER 2: METRICS                                 │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│    ParseReportData(raw) ──► data struct                                 │
│                                 │                                       │
│                                 ▼                                       │
│    ComputeAllMetrics(data) ──► ComputedMetrics                         │
│                                                                         │
│    Each analyzer produces typed metrics:                                │
│    - Aggregate metrics (summary statistics)                             │
│    - List metrics (detailed items)                                      │
│    - Time-series metrics (temporal data)                                │
│    - Distribution metrics (categorized counts)                          │
│                                                                         │
└────────────────────────────────┬────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                        LAYER 3: OUTPUT                                  │
├─────────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────────────┐  │
│  │      JSON       │  │      YAML       │  │         Plot            │  │
│  │  ToJSON() any   │  │  ToYAML() any   │  │   (chart generation)    │  │
│  └─────────────────┘  └─────────────────┘  └─────────────────────────┘  │
│                                                                         │
│  ALL outputs consume ONLY from ComputedMetrics, never raw data          │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

## Layer 1: Ingest

The ingest layer collects raw data from various sources.

### History Analyzers

History analyzers process git repository data:
- Commit history traversal
- File changes and diffs
- Author information
- Timestamps

**Command**: `codefang history <repo> -a <analyzer>`

**Analyzers**: devs, burndown, file-history, couples, shotness, sentiment

### Static Analyzers

Static analyzers process UAST (Universal Abstract Syntax Tree) data:
- Code structure analysis
- Metric computation from AST nodes

**Command**: `uast parse <file> | codefang analyze -a <analyzer>`

**Analyzers**: complexity, cohesion, halstead, comments, imports, typos

## Layer 2: Metrics

The metrics layer transforms raw data into structured, typed metrics.

### The ComputedMetrics Pattern

Every analyzer defines a `ComputedMetrics` struct that contains all computed values:

```go
type ComputedMetrics struct {
    Aggregate   AggregateMetrics   `json:"aggregate" yaml:"aggregate"`
    Items       []ItemMetric       `json:"items" yaml:"items"`
    TimeSeries  []TimePoint        `json:"time_series,omitempty" yaml:"time_series,omitempty"`
    // ... analyzer-specific fields
}
```

### Data Transformation Functions

Each analyzer implements two key functions:

**ParseReportData**: Converts raw analyzer output to a typed data structure.

```go
func ParseReportData(raw any) (*ReportData, error) {
    // Parse raw hercules/UAST output
    // Return typed data structure
}
```

**ComputeAllMetrics**: Transforms parsed data into computed metrics.

```go
func ComputeAllMetrics(data *ReportData) *ComputedMetrics {
    // Compute aggregates, lists, time series
    // Return structured metrics
}
```

### Metric Types

| Type | Purpose | Example |
|------|---------|---------|
| Aggregate | Summary statistics | Total commits, average complexity |
| List | Detailed item data | Per-developer stats, per-file metrics |
| TimeSeries | Temporal data | Burndown curves, activity over time |
| Distribution | Categorized counts | Complexity buckets, language breakdown |

## Layer 3: Output

The output layer serializes ComputedMetrics to various formats.

### MetricsOutput Interface

All ComputedMetrics structs implement the `MetricsOutput` interface:

```go
// pkg/analyzers/common/renderer/metrics_output.go

type MetricsOutput interface {
    // AnalyzerName returns the analyzer identifier (e.g., "devs", "burndown").
    AnalyzerName() string

    // ToJSON returns a struct suitable for JSON marshaling.
    ToJSON() any

    // ToYAML returns a struct suitable for YAML marshaling.
    ToYAML() any
}
```

### Render Helpers

The renderer package provides helper functions:

```go
import "github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/renderer"

// Render to JSON
jsonBytes, err := renderer.RenderMetricsJSON(computedMetrics)

// Render to YAML
yamlBytes, err := renderer.RenderMetricsYAML(computedMetrics)
```

### JSON/YAML Output

The Serialize method uses ComputeAllMetrics for JSON/YAML:

```go
func (a *MyAnalyzer) Serialize(format string, w io.Writer) error {
    metrics := ComputeAllMetrics(a.reportData)
    
    switch format {
    case "json":
        data, err := renderer.RenderMetricsJSON(metrics)
        // ...
    case "yaml":
        data, err := renderer.RenderMetricsYAML(metrics)
        // ...
    }
}
```

### Plot Output

Plot generators also consume ComputedMetrics:

```go
func generateDashboard(metrics *ComputedMetrics) *Dashboard {
    // Use metrics.Aggregate, metrics.TimeSeries, etc.
    // Generate visualization data
}
```

## Adding a New Analyzer

Follow these steps to add metrics support to a new analyzer.

### Step 1: Define ComputedMetrics Struct

Create the metrics struct with appropriate JSON/YAML tags:

```go
// pkg/analyzers/myanalyzer/metrics.go

type ComputedMetrics struct {
    Aggregate AggregateMetrics `json:"aggregate" yaml:"aggregate"`
    Items     []ItemMetric     `json:"items" yaml:"items"`
}

type AggregateMetrics struct {
    TotalCount int     `json:"total_count" yaml:"total_count"`
    Average    float64 `json:"average" yaml:"average"`
}

type ItemMetric struct {
    Name  string  `json:"name" yaml:"name"`
    Value float64 `json:"value" yaml:"value"`
}
```

### Step 2: Implement ParseReportData

Parse raw analyzer output into a typed structure:

```go
func ParseReportData(raw any) (*ReportData, error) {
    data, ok := raw.(map[string]any)
    if !ok {
        return nil, errors.New("invalid report data")
    }
    
    // Extract and validate fields
    return &ReportData{
        // ... parsed fields
    }, nil
}
```

### Step 3: Implement ComputeAllMetrics

Transform parsed data into metrics:

```go
func ComputeAllMetrics(data *ReportData) *ComputedMetrics {
    metrics := &ComputedMetrics{
        Aggregate: AggregateMetrics{
            TotalCount: len(data.Items),
            Average:    calculateAverage(data.Items),
        },
    }
    
    for _, item := range data.Items {
        metrics.Items = append(metrics.Items, ItemMetric{
            Name:  item.Name,
            Value: item.Value,
        })
    }
    
    return metrics
}
```

### Step 4: Implement MetricsOutput Interface

Add the interface methods to ComputedMetrics:

```go
func (m *ComputedMetrics) AnalyzerName() string {
    return "myanalyzer"
}

func (m *ComputedMetrics) ToJSON() any {
    return m
}

func (m *ComputedMetrics) ToYAML() any {
    return m
}
```

### Step 5: Update Serialize Method

Use ComputeAllMetrics in the analyzer's Serialize method:

```go
func (a *MyAnalyzer) Serialize(format string, w io.Writer) error {
    metrics := ComputeAllMetrics(a.reportData)
    
    switch format {
    case "json":
        data, err := renderer.RenderMetricsJSON(metrics)
        if err != nil {
            return err
        }
        _, err = w.Write(data)
        return err
        
    case "yaml":
        data, err := renderer.RenderMetricsYAML(metrics)
        if err != nil {
            return err
        }
        _, err = w.Write(data)
        return err
        
    default:
        return fmt.Errorf("unsupported format: %s", format)
    }
}
```

## Analyzer Reference

All analyzers implement the metrics-first pattern:

| Analyzer | Type | ComputedMetrics Location | Key Metrics |
|----------|------|--------------------------|-------------|
| devs | History | `pkg/analyzers/devs/metrics.go` | developers, languages, busfactor, activity, churn |
| burndown | History | `pkg/analyzers/burndown/metrics.go` | global_survival, file_survival, developer_survival |
| file-history | History | `pkg/analyzers/file_history/metrics.go` | file_churn, file_contributors, hotspots |
| couples | History | `pkg/analyzers/couples/metrics.go` | file_coupling, developer_coupling, file_ownership |
| shotness | History | `pkg/analyzers/shotness/metrics.go` | node_hotness, node_coupling, hotspot_nodes |
| sentiment | History | `pkg/analyzers/sentiment/metrics.go` | time_series, trend, low_sentiment_periods |
| complexity | Static | `pkg/analyzers/complexity/metrics.go` | function_complexity, distribution, high_risk_functions |
| cohesion | Static | `pkg/analyzers/cohesion/metrics.go` | class_cohesion, low_cohesion_classes |
| halstead | Static | `pkg/analyzers/halstead/metrics.go` | function_metrics, high_effort_functions |
| comments | Static | `pkg/analyzers/comments/metrics.go` | comment_density, uncommented_functions |
| imports | Static | `pkg/analyzers/imports/metrics.go` | import_counts, dependency_analysis |
| typos | Static | `pkg/analyzers/typos/metrics.go` | typo_list, severity_distribution |

## Benefits of Metrics-First Architecture

1. **Consistency**: All output formats share the same underlying data
2. **Testability**: ComputedMetrics can be unit tested independently
3. **Extensibility**: Adding new output formats only requires implementing a renderer
4. **Type Safety**: Strongly typed metrics prevent serialization errors
5. **Documentation**: Metrics structs serve as self-documenting schemas
