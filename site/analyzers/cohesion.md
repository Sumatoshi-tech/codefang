# Cohesion Analyzer

The cohesion analyzer computes **LCOM4** (Lack of Cohesion of Methods, variant 4) metrics to identify classes and modules with low internal cohesion. Low cohesion is a strong indicator of "god classes" that bundle unrelated responsibilities.

---

## Quick Start

```bash
uast parse main.go | codefang analyze -a cohesion
```

Or analyze an entire directory:

```bash
codefang analyze -a cohesion ./src/
```

---

## What It Measures

### LCOM4 (Lack of Cohesion of Methods)

LCOM4 builds a graph where:

- **Nodes** are methods of a class
- **Edges** connect two methods if they both access at least one shared field

The LCOM4 value is the **number of connected components** in this graph.

!!! info "Interpretation"
    - **LCOM4 = 1**: Perfectly cohesive. All methods are connected through shared field access.
    - **LCOM4 = 2-3**: Moderate. The class may contain two or three distinct responsibilities.
    - **LCOM4 >= 4**: Low cohesion. Strong candidate for splitting into multiple classes.

### Method-Field Usage Graph

The analyzer also reports which methods access which fields, enabling visualization of the internal dependency structure of each class.

---

## Configuration Options

The cohesion analyzer uses the UAST directly and has no analyzer-specific configuration options.

| Option | Type | Default | Description |
|---|---|---|---|
| *(none)* | -- | -- | Uses UAST; no analyzer-specific config |

---

## Example Output

=== "JSON"

    ```json
    {
      "cohesion": {
        "classes": [
          {
            "name": "UserService",
            "file": "service.go",
            "lcom4": 1,
            "methods": 5,
            "fields": 3,
            "components": [
              ["getUser", "updateUser", "deleteUser", "validateUser", "saveUser"]
            ]
          },
          {
            "name": "AppController",
            "file": "controller.go",
            "lcom4": 3,
            "methods": 12,
            "fields": 8,
            "components": [
              ["handleAuth", "validateToken", "refreshSession"],
              ["renderDashboard", "loadWidgets", "cacheLayout"],
              ["exportCSV", "formatReport", "sendEmail",
               "scheduleJob", "retryFailed", "logMetrics"]
            ]
          }
        ],
        "summary": {
          "total_classes": 2,
          "avg_lcom4": 2.0,
          "max_lcom4": 3,
          "low_cohesion_count": 1
        }
      }
    }
    ```

=== "Text"

    ```
    Cohesion Analysis (LCOM4)
      UserService    (service.go)     LCOM4=1  methods=5   fields=3
      AppController  (controller.go)  LCOM4=3  methods=12  fields=8  *** low cohesion

    Summary: 2 classes, avg LCOM4=2.0, 1 low-cohesion class
    ```

---

## Use Cases

- **God class detection**: Find classes with LCOM4 >= 3 that should be split.
- **Architecture reviews**: Validate that classes follow the Single Responsibility Principle.
- **Refactoring targets**: Use the connected components to see how a class naturally splits.
- **Design quality tracking**: Monitor LCOM4 trends over time to catch cohesion degradation.

---

## Limitations

- **Language scope**: Works best with object-oriented languages that have explicit class definitions (Go structs with methods, Java/Python/TypeScript classes). Functional code may produce less meaningful results.
- **Accessor methods**: Simple getters/setters can inflate cohesion scores artificially since each accessor touches only one field.
- **Inherited fields**: LCOM4 considers only fields defined within the class itself, not inherited fields.
- **Free functions**: Standalone functions outside classes are not included in the LCOM4 computation.
