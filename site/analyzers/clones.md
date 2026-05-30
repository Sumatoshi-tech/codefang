# Clone detection analyzer reference

The clone detection analyzer finds **duplicate and near-duplicate functions**
across your entire codebase using MinHash signatures and Locality-Sensitive
Hashing (LSH). It operates on the UAST representation and detects clones
**cross-file** — not just within a single file.

For the conceptual model — the clone-type taxonomy, the methodology, and why
MinHash + LSH — see
[Understanding clone detection](../explanation/clones.md). To run it, see the
[Quick start](../getting-started/quickstart.md).

---

## Configuration options

The clone detection analyzer uses the UAST directly and has no analyzer-specific configuration options.

| Option | Type | Default | Description |
|---|---|---|---|
| *(none)* | -- | -- | Uses UAST; no analyzer-specific config |

---

## Example output

=== "Text"

    ```
    +----------------------------------------------+
    |              CLONE DETECTION                  |
    |               Score: 7/10                     |
    |  Low duplication - few clone pairs detected   |
    +----------------------------------------------+

    Key Metrics
      Total Functions ....... 346
      Clone Pairs ........... 12
      Clone Ratio ........... 0.03

    Distribution
      Type-1 (Exact) ....... 17%  (2)
      Type-2 (Renamed) ..... 50%  (6)
      Type-3 (Near-miss) ... 33%  (4)

    Issues (sorted worst-first)
      pkg/a.go::Handler <-> pkg/b.go::Handler   Type-1   1.00
      cmd/run.go::Execute <-> cmd/serve.go::Execute   Type-2   0.92
    ```

=== "JSON"

    ```json
    {
      "total_functions": 346,
      "total_clone_pairs": 12,
      "clone_ratio": 0.035,
      "clone_pairs": [
        {
          "func_a": "pkg/a.go::Handler",
          "func_b": "pkg/b.go::Handler",
          "similarity": 1.0,
          "clone_type": "Type-1"
        },
        {
          "func_a": "cmd/run.go::Execute",
          "func_b": "cmd/serve.go::Execute",
          "similarity": 0.92,
          "clone_type": "Type-2"
        }
      ],
      "message": "Low duplication - few clone pairs detected"
    }
    ```

=== "HTML"

    The HTML plot output includes a **Clone Type Distribution** pie chart showing the breakdown of detected clones by type (Type-1, Type-2, Type-3).

---

## See also

- [Understanding clone detection](../explanation/clones.md) — the mental model, methodology, references, and limitations.
- [Quick start](../getting-started/quickstart.md) — run static analysis.
