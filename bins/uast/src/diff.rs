//! `uast diff file1 file2` — structural UAST diff (port of `cmd/uast/diff.go`).
//!
//! Parses both files, runs [`cf_uast::detect_changes`], and emits the changes.
//! The JSON form is a `[]Change` array; `Change` is a Go **struct**, so its keys
//! emit in declaration order (`type`, `before`, `after`, `file`) with
//! `before`/`after` omitted when empty — built here as a struct-origin object
//! (declaration order preserved, NOT byte-sorted). `unified`/`summary` are
//! human-format text.

use std::fs::File;
use std::io::{self, Write};

use clap::{Arg, ArgAction, ArgMatches, Command};
use cf_textutil::{GoMap, GoValue};
use cf_uast::{detect_changes, ChangeType, Parser};

use crate::govalue_bridge::node_to_value;
use crate::FORMAT_JSON;

/// A structural change between two files (diff.go `Change`).
struct Change {
    change_type: String,
    before: Option<GoValue>,
    after: Option<GoValue>,
    file: String,
}

/// Builds the `diff` subcommand (diff.go:29-47).
pub fn command() -> Command {
    Command::new("diff")
        .about("Compare two files and detect changes")
        .arg(Arg::new("file1").required(true).index(1))
        .arg(Arg::new("file2").required(true).index(2))
        .arg(
            Arg::new("output")
                .long("output")
                .short('o')
                .help("output file (default: stdout)")
                .default_value("")
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new("format")
                .long("format")
                .short('f')
                .help("output format (unified, summary, json)")
                .default_value("unified")
                .action(ArgAction::Set),
        )
}

/// Runs `diff` (diff.go `runDiff`).
pub fn run(m: &ArgMatches) -> Result<(), String> {
    let file1 = m.get_one::<String>("file1").map(String::as_str).unwrap_or("");
    let file2 = m.get_one::<String>("file2").map(String::as_str).unwrap_or("");
    let output = m.get_one::<String>("output").map(String::as_str).unwrap_or("");
    let format = m.get_one::<String>("format").map(String::as_str).unwrap_or("unified");

    let parser = Parser::new();

    if !parser.is_supported(file1) {
        return Err(format!("unsupported file type: {file1}"));
    }
    let node1 = parser.parse_file(file1, "").map_err(|e| format!("failed to parse {file1}: {e}"))?;

    if !parser.is_supported(file2) {
        return Err(format!("unsupported file type: {file2}"));
    }
    let node2 = parser.parse_file(file2, "").map_err(|e| format!("failed to parse {file2}: {e}"))?;

    let changes = detect(&node1, &node2, file1);
    output_changes(&changes, output, format)
}

/// Builds the local `Change` list from `detect_changes` (diff.go `detectChanges`).
fn detect(node1: &cf_uast::Node, node2: &cf_uast::Node, file1: &str) -> Vec<Change> {
    detect_changes(Some(node1), Some(node2))
        .into_iter()
        .map(|c| Change {
            change_type: change_type_string(c.change_type),
            file: file1.to_string(),
            before: c.before.as_ref().map(node_to_value),
            after: c.after.as_ref().map(node_to_value),
        })
        .collect()
}

/// Renders a [`ChangeType`] as Go's `String()` (`added`/`removed`/`modified`).
fn change_type_string(ct: ChangeType) -> String {
    ct.as_str().to_string()
}

/// Serializes the changes (diff.go `outputChanges`).
fn output_changes(changes: &[Change], output: &str, format: &str) -> Result<(), String> {
    let mut writer: Box<dyn Write> = if output.is_empty() {
        Box::new(io::stdout())
    } else {
        Box::new(File::create(output).map_err(|e| format!("failed to create output file: {e}"))?)
    };

    match format {
        FORMAT_JSON => {
            let value = changes_to_value(changes);
            cf_textutil::write_json(&mut writer, &value, true).map_err(|e| e.to_string())
        }
        "unified" => {
            print_unified(changes, &mut writer);
            Ok(())
        }
        "summary" => {
            print_summary(changes, &mut writer);
            Ok(())
        }
        other => Err(format!("unsupported format: {other}")),
    }
}

/// Builds the `[]Change` JSON value. Each `Change` is a Go struct, so its keys
/// are in declaration order with `before`/`after` omitempty (diff.go `Change`).
fn changes_to_value(changes: &[Change]) -> GoValue {
    GoValue::Array(
        changes
            .iter()
            .map(|c| {
                // Struct-origin: declaration order type, before, after, file.
                let mut m = GoMap::new_struct();
                m.push("type", GoValue::Str(c.change_type.clone()));
                if let Some(b) = &c.before {
                    m.push("before", b.clone());
                }
                if let Some(a) = &c.after {
                    m.push("after", a.clone());
                }
                m.push("file", GoValue::Str(c.file.clone()));
                GoValue::Object(m)
            })
            .collect(),
    )
}

/// Prints a unified-diff-like rendering (diff.go `printUnifiedDiff`).
fn print_unified(changes: &[Change], writer: &mut dyn Write) {
    for c in changes {
        let _ = writeln!(writer, "--- {}", c.file);
        let _ = writeln!(writer, "+++ {}", c.file);
        let _ = writeln!(writer, "@@ -1,1 +1,1 @@");
        let _ = writeln!(writer, "-{}", c.change_type);
        let _ = writeln!(writer, "+{}", c.change_type);
    }
}

/// Prints a change-type summary (diff.go `printChangeSummary`). The Go version
/// ranges a `map[string]int` (nondeterministic iteration); this output is
/// human-format (non-binding), so a stable BTreeMap is used here.
fn print_summary(changes: &[Change], writer: &mut dyn Write) {
    use std::collections::BTreeMap;
    let mut summary: BTreeMap<&str, i64> = BTreeMap::new();
    for c in changes {
        *summary.entry(c.change_type.as_str()).or_insert(0) += 1;
    }
    let _ = writeln!(writer, "Change Summary:");
    for (ct, count) in summary {
        let _ = writeln!(writer, "  {ct}: {count}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ported from diff_test.go TestLocalDetectChangesWired: detectChanges must
    // wire to uast::detect_changes and return real changes for differing nodes.
    #[test]
    fn detect_returns_change_for_differing_nodes() {
        let before = cf_uast::Node::with_token("Function", "oldFunction");
        let after = cf_uast::Node::with_token("Function", "newFunction");
        let changes = detect(&before, &after, "test1.go");
        assert!(!changes.is_empty(), "expected at least one change");
        assert_eq!(changes[0].file, "test1.go");
    }

    // Ported from diff_test.go TestChangeTypeStringValues.
    #[test]
    fn change_type_string_values() {
        assert_eq!(change_type_string(ChangeType::Added), "added");
        assert_eq!(change_type_string(ChangeType::Removed), "removed");
        assert_eq!(change_type_string(ChangeType::Modified), "modified");
    }

    #[test]
    fn change_json_is_struct_order_with_omitempty() {
        let changes = vec![Change {
            change_type: "modified".to_string(),
            before: Some(GoValue::Str("b".to_string())),
            after: None,
            file: "f.go".to_string(),
        }];
        let v = changes_to_value(&changes);
        let s = String::from_utf8(cf_textutil::marshal_json(&v, false).unwrap()).unwrap();
        // Declaration order: type, before, (after omitted), file.
        assert_eq!(s, "[{\"type\":\"modified\",\"before\":\"b\",\"file\":\"f.go\"}]\n");
    }

    #[test]
    fn unsupported_format_errors() {
        let err = output_changes(&[], "", "xml").unwrap_err();
        assert_eq!(err, "unsupported format: xml");
    }
}
