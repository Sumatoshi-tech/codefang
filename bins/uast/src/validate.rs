//! `uast validate <file.json|->` — UAST schema validation (port of
//! `cmd/uast/validate.go`).
//!
//! Validates a UAST JSON document against the canonical UAST JSON Schema. The Go
//! command exits via `os.Exit` with three codes:
//!   * `0` — valid;
//!   * `1` — validation FAILED (document is well-formed JSON but violates schema);
//!   * `2` — error (bad JSON, schema read failure, open failure, engine error).
//!
//! `--no-color` wins over `--color` (cosmetic). The schema defaults to the
//! embedded `cf-uast-spec` schema when `--schema` is empty or the Go default
//! path string; otherwise the path is read from disk. The compliance percentage
//! mirrors `calculateCompliance`/`countNodes`.

use std::io::{self, Read};
use std::process::exit;

use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::Value;

/// Exit code for validation failure (validate.go `os.Exit(1)`).
const EXIT_INVALID: i32 = 1;
/// Exit code for errors (validate.go `exitCodeValidationFailure = 2`).
const EXIT_ERROR: i32 = 2;
/// Maximum compliance percentage (validate.go `complianceMax`).
const COMPLIANCE_MAX: i64 = 100;
/// The Go default `--schema` value, which selects the embedded schema.
const DEFAULT_SCHEMA_PATH: &str = "pkg/uast/spec/uast-schema.json";

/// Builds the `validate` subcommand (validate.go:24-43).
pub fn command() -> Command {
    Command::new("validate")
        .about("Validate a UAST JSON file against the UAST schema")
        .override_usage("uast validate <file.json|-> [flags]")
        .arg(Arg::new("file").required(true).index(1))
        .arg(
            Arg::new("schema")
                .long("schema")
                .help("path to UAST JSON schema")
                .default_value(DEFAULT_SCHEMA_PATH)
                .action(ArgAction::Set),
        )
        .arg(Arg::new("color").long("color").help("force colored output").action(ArgAction::SetTrue))
        .arg(
            Arg::new("no-color")
                .long("no-color")
                .help("disable colored output")
                .action(ArgAction::SetTrue),
        )
}

/// Runs `validate`. Always exits via [`exit`] with code 0/1/2 (validate.go uses
/// `os.Exit` directly), so the returned `Result` is `Ok(())` only on the
/// unreachable fall-through; in practice this function diverges.
pub fn run(m: &ArgMatches) -> Result<(), String> {
    let file = m.get_one::<String>("file").map(String::as_str).unwrap_or("");
    let schema_path = m.get_one::<String>("schema").map(String::as_str).unwrap_or(DEFAULT_SCHEMA_PATH);
    // --no-color wins over --color (validate.go); both are cosmetic.
    let _color = !m.get_flag("no-color") && m.get_flag("color");

    let (input_bytes, label) = load_input(file);
    let input: Value = match serde_json::from_slice(&input_bytes) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Invalid JSON in {label}: {e}");
            exit(EXIT_ERROR);
        }
    };

    let schema_bytes = load_schema(schema_path);
    let schema: Value = match serde_json::from_slice(&schema_bytes) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Schema validation error: {e}");
            exit(EXIT_ERROR);
        }
    };

    let compiled = match jsonschema::JSONSchema::compile(&schema) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Schema validation error: {e}");
            exit(EXIT_ERROR);
        }
    };

    let errors: Vec<String> = match compiled.validate(&input) {
        Ok(()) => Vec::new(),
        Err(iter) => iter.map(|e| e.to_string()).collect(),
    };

    if errors.is_empty() {
        println!("UAST is valid ({label})");
        println!("  Compliance: 100%");
        exit(0);
    }

    let compliance = calculate_compliance(&input, errors.len());
    println!("UAST validation failed ({label})");
    println!("  Compliance: {compliance}%");
    println!();
    println!("Errors:");
    for e in &errors {
        println!("  - {e}");
    }
    println!();
    println!("Recommendations:");
    println!("\nGeneral tips:");
    println!("  - Check the UAST specification at pkg/uast/spec/SPEC.md");
    println!("  - Use the schema at pkg/uast/spec/uast-schema.json as reference");
    println!("  - Ensure all required fields are present");
    println!("  - Validate field types and values against the schema");
    exit(EXIT_INVALID);
}

/// Loads the input document and its label (validate.go `loadInput`). `-` reads
/// stdin (label `stdin`); otherwise the file is read (label = path). A read
/// failure exits with code 2.
fn load_input(path: &str) -> (Vec<u8>, String) {
    if path == "-" {
        let mut buf = Vec::new();
        if let Err(e) = io::stdin().read_to_end(&mut buf) {
            eprintln!("Failed to open input: {e}");
            exit(EXIT_ERROR);
        }
        return (buf, "stdin".to_string());
    }
    match std::fs::read(path) {
        Ok(b) => (b, path.to_string()),
        Err(e) => {
            eprintln!("Failed to open input: {e}");
            exit(EXIT_ERROR);
        }
    }
}

/// Loads the schema bytes (validate.go `loadSchema`). An empty path or the Go
/// default path selects the embedded `cf-uast-spec` schema; otherwise the file
/// is read (failure exits 2).
fn load_schema(path: &str) -> Vec<u8> {
    if path.is_empty() || path == DEFAULT_SCHEMA_PATH {
        return cf_uast_spec::schema().as_bytes().to_vec();
    }
    match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to read schema file: {e}");
            exit(EXIT_ERROR);
        }
    }
}

/// Computes the compliance percentage (validate.go `calculateCompliance`):
/// `int(validNodes / totalNodes * 100)` clamped to `0..=100`, where
/// `validNodes = totalNodes - errorCount`.
fn calculate_compliance(input: &Value, error_count: usize) -> i64 {
    let total = count_nodes(input);
    if total == 0 {
        return 0;
    }
    let valid = total - error_count as i64;
    let compliance =
        ((valid as f64 / total as f64) * COMPLIANCE_MAX as f64) as i64;
    compliance.clamp(0, COMPLIANCE_MAX)
}

/// Counts nodes recursively (validate.go `countNodes`): this node plus all
/// nodes under `children` arrays (and array elements).
fn count_nodes(data: &Value) -> i64 {
    let mut count = 1;
    match data {
        Value::Object(map) => {
            if let Some(Value::Array(children)) = map.get("children") {
                for child in children {
                    count += count_nodes(child);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                count += count_nodes(item);
            }
        }
        _ => {}
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn count_nodes_counts_self_and_children() {
        let v = json!({"type":"File","children":[{"type":"A"},{"type":"B","children":[{"type":"C"}]}]});
        // File + A + B + C = 4.
        assert_eq!(count_nodes(&v), 4);
    }

    #[test]
    fn compliance_clamped_and_computed() {
        let v = json!({"type":"File","children":[{"type":"A"},{"type":"B"}]}); // 3 nodes
        assert_eq!(calculate_compliance(&v, 0), 100);
        assert_eq!(calculate_compliance(&v, 3), 0);
        // 1 error of 3 nodes => 2/3 => 66.
        assert_eq!(calculate_compliance(&v, 1), 66);
        // More errors than nodes clamps to 0.
        assert_eq!(calculate_compliance(&v, 10), 0);
    }

    #[test]
    fn compliance_zero_when_no_nodes() {
        // A scalar counts as 1 node; 0 errors of 1 node => 100% (matches Go
        // `calculateCompliance`: 1/1 * 100 = 100).
        assert_eq!(calculate_compliance(&json!("x"), 0), 100);
        assert_eq!(calculate_compliance(&json!([]), 0), 100); // 1 node (the array), 0 errors
    }

    #[test]
    fn embedded_schema_is_loaded_for_default_path() {
        let bytes = load_schema(DEFAULT_SCHEMA_PATH);
        assert!(!bytes.is_empty());
        assert!(String::from_utf8_lossy(&bytes).contains("$schema"));
    }
}
