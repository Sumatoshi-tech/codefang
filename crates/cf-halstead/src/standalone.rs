//! Standalone per-file Halstead analysis — the entry point the
//! `history/quality` analyzer consumes.
//!
//! It runs the same detector and the same union aggregate as the static
//! `static/halstead` report ([`crate::aggregate`]), so a file measured by
//! `quality` and by `static/halstead` yields the same volume, effort and bug
//! estimate. Only the three file-level scalars the quality analyzer reads
//! are returned; a file with no functions yields all zeros.

use cf_uast_node::Node;

use crate::aggregate::{analyze_functions, summarize};

/// File-level Halstead measures consumed by the quality analyzer.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FileHalstead {
    /// V — file-level volume (union of every function's multisets).
    pub volume: f64,
    /// E — file-level effort.
    pub effort: f64,
    /// B — file-level delivered bugs.
    pub delivered_bugs: f64,
}

/// Runs the file-level analysis over `root` without the file's source bytes
/// (token-less nodes are recovered through verbatim tokens only; see
/// [`crate::detector::collect_functions`]). Prefer [`analyze_with_source`]
/// when the bytes are at hand.
#[must_use]
pub fn analyze(root: &Node) -> FileHalstead {
    analyze_with_source(root, None)
}

/// Runs the file-level analysis over `root`, reading token-less nodes back
/// from `source` by span.
#[must_use]
pub fn analyze_with_source(root: &Node, source: Option<&[u8]>) -> FileHalstead {
    let functions = analyze_functions(root, source);
    let summary = summarize(&functions);
    FileHalstead {
        volume: summary.derived.volume,
        effort: summary.derived.effort,
        delivered_bugs: summary.derived.delivered_bugs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tree_is_zero() {
        assert_eq!(
            analyze(&Node::with_token("File", "")),
            FileHalstead::default()
        );
    }

    /// `func same() int { return 1 }` has one operator and one operand:
    /// n = 2, N = 2, V = 2 · log2(2) = 2.
    #[test]
    fn one_return_function_has_positive_volume() {
        let mut lit = Node::with_token("Literal", "1");
        lit.roles = vec!["Literal".into()];
        let mut ret = Node::with_token("Return", "return 1");
        ret.roles = vec!["Return".into()];
        ret.add_child(lit);
        let mut body = Node::with_token("Block", "");
        body.roles = vec!["Body".into()];
        body.add_child(ret);
        let mut f = Node::with_token("Function", "");
        f.roles = vec!["Function".into(), "Declaration".into()];
        f.props.insert("name".into(), "same".into());
        f.add_child(body);
        let mut root = Node::with_token("File", "");
        root.add_child(f);

        let h = analyze(&root);
        assert_eq!(h.volume, 2.0);
        assert_eq!(h.effort, 1.0); // D = (1/2) · (1/1) = 0.5
        assert_eq!(h.delivered_bugs, 2.0 / 3000.0);
    }
}
