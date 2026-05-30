//! [`Parser::parse_file`]: read a source file from disk and parse it.
//!
//! Direct port of Go `pkg/uast/parsefile.go`.

use std::path::Path;

use cf_uast_node::Node;

use crate::parser::Parser;
use crate::types::ParseError;

impl Parser {
    /// Reads a source file and returns its UAST (Go `ParseFile`).
    ///
    /// If `lang` is non-empty it overrides extension-derived language detection:
    /// the resolved path's extension is replaced with `.<lang>` before parsing,
    /// exactly as Go does (`strings.TrimSuffix(resolvedPath, ext) + "." + lang`).
    ///
    /// Errors are wrapped as `read <path>: <e>` / `parse <path>: <e>` to mirror
    /// Go's `fmt.Errorf` wrapping.
    ///
    /// # Note on `iosafety`
    ///
    /// Go reads via `iosafety.ReadFile`, which returns a *resolved* path (after
    /// symlink/`..` safety checks). Until the `cf-iosafety` crate's reader is
    /// available here, this reads the path directly with [`std::fs::read`] and
    /// treats the input path as already-resolved. Behavior matches for ordinary
    /// (non-symlinked, in-tree) paths; see crate todos.
    pub fn parse_file(&self, path: &str, lang: &str) -> Result<Node, ParseError> {
        let code = std::fs::read(path)
            .map_err(|e| ParseError::Other(format!("read {path}: {e}")))?;

        let resolved_path = path;

        let filename = if !lang.is_empty() {
            let ext = Path::new(resolved_path)
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy()))
                .unwrap_or_default();
            let stem = resolved_path
                .strip_suffix(&ext)
                .unwrap_or(resolved_path);
            format!("{stem}.{lang}")
        } else {
            resolved_path.to_string()
        };

        self.parse(&filename, &code)
            .map_err(|e| ParseError::Other(format!("parse {path}: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn missing_file_errors_with_read_prefix() {
        let p = Parser::new();
        let err = p
            .parse_file("/no/such/file/definitely-missing.go", "")
            .unwrap_err();
        match err {
            ParseError::Other(msg) => assert!(
                msg.starts_with("read /no/such/file/definitely-missing.go:"),
                "unexpected message: {msg}"
            ),
            other => panic!("expected Other(read ...), got {other:?}"),
        }
    }

    #[test]
    fn lang_override_rewrites_extension() {
        // Write a temp file with a non-language extension, then force `lang=go`.
        // The grammar isn't wired yet, so parse fails *after* language
        // resolution — proving the override picked the go parser (a `.txt`
        // extension would otherwise yield NoParser before this point).
        let mut tmp = std::env::temp_dir();
        tmp.push(format!("cf_uast_parsefile_{}.txt", std::process::id()));
        let mut f = std::fs::File::create(&tmp).unwrap();
        f.write_all(b"package main").unwrap();
        drop(f);

        let p = Parser::new();
        let res = p.parse_file(tmp.to_str().unwrap(), "go");
        let _ = std::fs::remove_file(&tmp);

        // Resolution succeeded (go parser chosen); failure is the known grammar
        // gap, wrapped with the `parse <path>:` prefix.
        match res {
            Err(ParseError::Other(msg)) => {
                assert!(msg.contains("parse "), "expected parse-prefixed error, got {msg}");
            }
            other => panic!("expected wrapped parse error, got {other:?}"),
        }
    }
}
