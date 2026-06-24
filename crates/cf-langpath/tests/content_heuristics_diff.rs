//! Differential test of [`cf_langpath::content_heuristics::languages_by_content`]
//! against a live Go `src-d/enry@v2.1.0` oracle.
//!
//! The oracle (a Go program linking the real enry library) walks a real corpus,
//! and for every file whose last extension is one of the 80 content-heuristic
//! extensions emits `path<TAB>isBinary<TAB>json(GetLanguagesByContent(base, content))`.
//! This test replays the same files through the Rust port and requires the
//! ordered language list to match exactly on every non-binary file.
//!
//! It is `#[ignore]`d by default because it depends on the machine-local oracle
//! output at `CF_LANGPATH_ORACLE` (default `/tmp/oracle_out.tsv`) and the
//! corpus files it references. Run with:
//!
//! ```text
//! cargo test -p cf-langpath --test content_heuristics_diff -- --ignored
//! ```

use std::fs;

use cf_langpath::content_heuristics::languages_by_content;

/// Minimal JSON-array-of-strings parser for the oracle's `json.Marshal([]string)`
/// output (`[]`, `["A"]`, `["A","B"]`). Language names in enry never contain a
/// double quote or backslash, so no escape handling is needed.
fn parse_json_str_array(s: &str) -> Vec<String> {
    let s = s.trim();
    if s == "[]" || s == "null" {
        return Vec::new();
    }
    let inner = &s[1..s.len() - 1]; // strip [ ]
    inner
        .split(',')
        .map(|tok| tok.trim().trim_matches('"').to_string())
        .collect()
}

#[test]
#[ignore = "requires the Go oracle output and corpus (see module docs)"]
fn matches_go_enry_oracle() {
    let path =
        std::env::var("CF_LANGPATH_ORACLE").unwrap_or_else(|_| "/tmp/oracle_out.tsv".to_string());
    let tsv = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("oracle file {path}: {e}; run the Go oracle first"));

    let mut compared = 0usize;
    let mut mismatches = Vec::new();

    for line in tsv.lines() {
        if line.is_empty() {
            continue;
        }
        let mut cols = line.splitn(3, '\t');
        let file = cols.next().unwrap();
        let is_binary = cols.next().unwrap();
        let want = parse_json_str_array(cols.next().unwrap());

        // The Rust caller (devs_detect_language) returns early on binary; the
        // task only requires parity on non-binary content.
        if is_binary == "true" {
            continue;
        }

        // Skip files that vanished since the oracle ran.
        let Ok(content) = fs::read(file) else {
            continue;
        };
        // Pass the full path: enry computes filepath.Ext on it, and the Rust
        // port mirrors filepath.Ext including separator handling.
        let got = languages_by_content(file, &content);

        compared += 1;
        if got != want && mismatches.len() < 50 {
            mismatches.push(format!("{file}\n  want={want:?}\n   got={got:?}"));
        }
    }

    assert!(
        mismatches.is_empty(),
        "compared {compared} non-binary files, {} mismatches:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );

    assert!(compared > 0, "oracle produced no comparable rows");
    eprintln!("compared {compared} non-binary files, 0 mismatches");
}
