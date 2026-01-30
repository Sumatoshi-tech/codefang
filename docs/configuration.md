# Configuration Reference

Codefang supports configuration via a `.codefang.yaml` file in your project root or home directory. This allows you to tune thresholds and enable/disable specific checks.

## Default Configuration

If no config file is found, Codefang defaults to the following values:

```yaml
analyzers:
  complexity:
    cyclomatic_threshold: 15    # Warn if complexity > 15
    cognitive_threshold: 20     # Warn if cognitive complexity > 20
  
  cohesion:
    lcom_threshold: 2.0         # Warn if LCOM > 2 (low cohesion)
  
  comments:
    min_ratio: 0.1              # Expect at least 10% comments
```

## Example `.codefang.yaml`

Below is a comprehensive configuration file example.

```yaml
# Global settings
verbose: false
no_color: false

# Analyzer-specific settings
analyzers:
  
  # Complexity Analyzer
  # Measures control flow complexity.
  complexity:
    enabled: true
    # Cyclomatic Complexity: Number of independent paths
    cyclomatic_threshold: 10    # Strict mode (default is 15)
    # Cognitive Complexity: Mental effort to understand
    cognitive_threshold: 15
    exclude_patterns:
      - "*_test.go"             # Ignore tests
      - "generated/*"           # Ignore generated code

  # Cohesion Analyzer
  # Measures how well class methods relate to each other.
  cohesion:
    enabled: true
    lcom_threshold: 1.5         # Warn if methods don't share state
    ignore_getters_setters: true

  # Halstead Metrics
  # Measures volume and difficulty.
  halstead:
    enabled: false              # Disabled by default
    volume_threshold: 1000

  # Imports Analyzer
  # Checks dependency graph.
  imports:
    enabled: true
    max_depth: 5                # Max import depth
    allow_cycles: false         # Fail on circular dependencies

# Output settings
output:
  format: "text"                # Options: text, json, compact
  file: "report.txt"            # Optional: write to file
```

## Environment Variables

All configuration options can be overridden via environment variables using the `CODEFANG_` prefix. Structure is `CODEFANG_<SECTION>_<KEY>`.

*   `CODEFANG_ANALYZERS_COMPLEXITY_CYCLOMATIC_THRESHOLD=20`
*   `CODEFANG_OUTPUT_FORMAT=json`
