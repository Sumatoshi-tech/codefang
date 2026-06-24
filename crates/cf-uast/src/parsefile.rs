//! [`Parser::parse_file`]: read a source file from disk and parse it.

use std::path::Path;

use cf_uast_node::Node;

use crate::parser::Parser;
use crate::types::ParseError;

impl Parser {
    /// Reads a source file and returns its UAST.
    ///
    /// If `lang` is non-empty it overrides extension-derived language
    /// detection: the resolved path's extension is replaced with `.<lang>`
    /// before parsing.
    ///
    /// # Errors
    ///
    /// I/O and parse failures are wrapped as `read <path>: <e>` /
    /// `parse <path>: <e>` (CLI compatibility contract).
    ///
    /// # Note on path resolution
    ///
    /// The reference implementation reads through a safety layer that returns
    /// a *resolved* path (after symlink/`..` checks). This reads the path
    /// directly with [`std::fs::read`] and treats the input path as
    /// already-resolved. Behavior matches for ordinary (non-symlinked,
    /// in-tree) paths.
    pub fn parse_file(&self, path: &str, lang: &str) -> Result<Node, ParseError> {
        let code =
            std::fs::read(path).map_err(|e| ParseError::Other(format!("read {path}: {e}")))?;

        let resolved_path = path;

        let filename = if lang.is_empty() {
            resolved_path.to_string()
        } else {
            let ext = Path::new(resolved_path)
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy()))
                .unwrap_or_default();
            let stem = resolved_path.strip_suffix(&ext).unwrap_or(resolved_path);
            format!("{stem}.{lang}")
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
        // With the go grammar wired, the override makes the `.txt` file parse as
        // Go: a `.txt` extension would otherwise yield NoParser, so a successful
        // Go parse proves the override picked the go parser.
        let mut tmp = std::env::temp_dir();
        tmp.push(format!("cf_uast_parsefile_{}.txt", std::process::id()));
        let mut f = std::fs::File::create(&tmp).unwrap();
        f.write_all(b"package main").unwrap();
        drop(f);

        let p = Parser::new();
        let res = p.parse_file(tmp.to_str().unwrap(), "go");
        let _ = std::fs::remove_file(&tmp);

        // The go grammar parsed `package main` into a non-empty tree rooted at
        // the `source_file` (a `package_clause` child). The override selected the
        // go parser; a `.txt` extension alone would have yielded `NoParser`.
        let node = res.expect("go-forced parse of a .txt file should succeed");
        // The go mapping lowers the `source_file` root to type `File`.
        assert_eq!(node.node_type, "File");
        assert!(
            !node.children.is_empty(),
            "parsed tree should have children"
        );
    }
}
