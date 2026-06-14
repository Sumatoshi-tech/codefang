//! `go run`-gated differential parity test against `gopkg.in/yaml.v3` v3.0.1.
//!
//! This is the Step 4 DoD oracle: it asserts that [`cf_goyaml::marshal`] is
//! byte-identical to `yaml.Marshal` for a battery of cases covering
//! int / float / bool / null / string-quoting parity, scalar quoting, key
//! ordering, block folding (literal `|-`), and the single trailing newline.
//!
//! The truth side is generated *at test time* by compiling and running a tiny
//! Go program against the repository's own `gopkg.in/yaml.v3` dependency, so the
//! oracle can never drift from a stale fixture. Each case carries a
//! [`GoValue`] fed to the Rust emitter and the equivalent Go source expression
//! fed to `yaml.Marshal`, kept in lock-step here.
//!
//! When the Go toolchain (or the module dir) is unavailable the test skips
//! (passes trivially) so a normal offline `cargo test` is never blocked, while
//! the parity gate runs it for real wherever `go` exists.

use std::process::Command;

use cf_gojson::{GoMap, GoValue};

/// A single parity case: the Rust value and the equivalent Go expression
/// (of static type `interface{}`) whose `yaml.Marshal` output must match.
struct Case {
    name: &'static str,
    value: GoValue,
    go_expr: String,
}

fn c(name: &'static str, value: GoValue, go_expr: impl Into<String>) -> Case {
    Case { name, value, go_expr: go_expr.into() }
}

/// Build the shared battery of cases. Each Go expression is wrapped as
/// `interface{}(<expr>)` so the values marshal exactly as a generic value
/// (the shape codefang reports emit), matching how the Rust side resolves tags.
#[allow(clippy::vec_init_then_push)]
fn cases() -> Vec<Case> {
    let mut v = Vec::new();

    // --- integers (signed / unsigned, sign, zero) -------------------------
    v.push(c("int_zero", GoValue::Int(0), "int(0)"));
    v.push(c("int_pos", GoValue::Int(42), "int(42)"));
    v.push(c("int_neg", GoValue::Int(-17), "int(-17)"));
    v.push(c("int_big", GoValue::Int(9_223_372_036_854_775_807), "int64(9223372036854775807)"));
    v.push(c("uint_big", GoValue::Uint(18_446_744_073_709_551_615), "uint64(18446744073709551615)"));

    // --- floats (g-format, integer-valued, exponents, zero) ---------------
    v.push(c("float_int_valued", GoValue::Float(1.0), "float64(1)"));
    v.push(c("float_zero", GoValue::Float(0.0), "float64(0)"));
    v.push(c("float_frac", GoValue::Float(0.7142857142857143), "float64(0.7142857142857143)"));
    v.push(c("float_big_exp", GoValue::Float(1e20), "float64(1e20)"));
    v.push(c("float_small_exp", GoValue::Float(1e-7), "float64(1e-7)"));
    v.push(c(
        "float_round_trip",
        GoValue::Float(123_456_789.123_456_79),
        "float64(123456789.123456789)",
    ));
    v.push(c("float_neg", GoValue::Float(-2.5), "float64(-2.5)"));

    // --- bool / null ------------------------------------------------------
    v.push(c("bool_true", GoValue::Bool(true), "true"));
    v.push(c("bool_false", GoValue::Bool(false), "false"));
    v.push(c("null", GoValue::Null, "interface{}(nil)"));

    // --- string quoting: words that would re-resolve to non-str tags ------
    // (yaml.v3 double-quotes these so a round-trip stays a string)
    for w in [
        "true", "false", "null", "yes", "no", "on", "off", "~", "True", "NULL",
    ] {
        v.push(c(
            "quote_resolveword",
            GoValue::Str(w.into()),
            go_string(w),
        ));
    }
    // numeric-looking strings -> double quoted
    for w in ["123", "1.5", "+5", "-3", "0", "0x1F", "1e3", "0o17"] {
        v.push(c("quote_numeric", GoValue::Str(w.into()), go_string(w)));
    }
    // timestamp / base-60 looking -> double quoted
    v.push(c(
        "quote_timestamp",
        GoValue::Str("2026-01-26T21:53:53Z".into()),
        go_string("2026-01-26T21:53:53Z"),
    ));

    // --- scalar quoting: structural indicators -> single quoted -----------
    for w in [
        "a: b", "a #b", "@foo", "!x", "[x]", "{x}", "&x", "*x", "%x", " x", "x ",
        "`x", "|x", ">x", "\"x", "'x", ",x", // leading flow/indicator chars
    ] {
        v.push(c("quote_indicator", GoValue::Str(w.into()), go_string(w)));
    }
    // --- scalar quoting: plain (no quoting) -------------------------------
    for w in [
        "hello", "a:b", "-x", "?x", "a,b", "CRITICAL", ".", "x: y is fine here too",
        "it's", "say \"hi\"", "<unknown>", "a/b/c", "v1.2.3",
    ] {
        v.push(c("quote_plain", GoValue::Str(w.into()), go_string(w)));
    }
    // empty string -> double quoted ""
    v.push(c("quote_empty", GoValue::Str(String::new()), go_string("")));

    // --- control chars -> double quoted with escapes ----------------------
    v.push(c("ctrl_x01", GoValue::Str("a\u{01}b".into()), go_string("a\u{01}b")));
    v.push(c("ctrl_tab", GoValue::Str("a\tb".into()), go_string("a\tb")));

    // --- block folding: newline-bearing strings -> literal block |- -------
    v.push(c("block_literal", GoValue::Str("a\nb".into()), go_string("a\nb")));
    v.push(c(
        "block_literal_multi",
        GoValue::Str("line one\nline two\nline three".into()),
        go_string("line one\nline two\nline three"),
    ));
    // a long single-line string stays UNfolded (Marshal best_width is unlimited)
    let long = "x".repeat(120);
    v.push(c("long_unfolded", GoValue::Str(long.clone()), go_string(&long)));

    // --- key ordering: map-origin keys byte-sort (yaml.v3 sorts map keys) --
    {
        let mut m = GoMap::from_map(vec![
            ("b".into(), GoValue::Int(2)),
            ("a".into(), GoValue::Int(1)),
            ("c".into(), GoValue::Int(3)),
        ]);
        let _ = &mut m;
        v.push(c(
            "map_sorted_keys",
            GoValue::Map(m),
            r#"map[string]interface{}{"b": int(2), "a": int(1), "c": int(3)}"#,
        ));
    }
    {
        // byte-order: uppercase before lowercase, digits before letters
        let m = GoMap::from_map(vec![
            ("Zeta".into(), GoValue::Int(1)),
            ("alpha".into(), GoValue::Int(2)),
            ("10".into(), GoValue::Int(3)),
            ("2".into(), GoValue::Int(4)),
        ]);
        v.push(c(
            "map_byte_order",
            GoValue::Map(m),
            r#"map[string]interface{}{"Zeta": int(1), "alpha": int(2), "10": int(3), "2": int(4)}"#,
        ));
    }

    {
        // yaml.v3 sorts embedded digit runs numerically ("a2" < "a10").
        let m = GoMap::from_map(vec![
            ("a10".into(), GoValue::Int(1)),
            ("a2".into(), GoValue::Int(2)),
            ("a1".into(), GoValue::Int(3)),
        ]);
        v.push(c(
            "map_numeric_runs",
            GoValue::Map(m),
            r#"map[string]interface{}{"a10": int(1), "a2": int(2), "a1": int(3)}"#,
        ));
    }
    {
        // letter-vs-digit and leading-zero handling.
        let m = GoMap::from_map(vec![
            ("10a".into(), GoValue::Int(1)),
            ("9z".into(), GoValue::Int(2)),
            ("10".into(), GoValue::Int(3)),
        ]);
        v.push(c(
            "map_numeric_mixed",
            GoValue::Map(m),
            r#"map[string]interface{}{"10a": int(1), "9z": int(2), "10": int(3)}"#,
        ));
    }
    {
        // dotted version-like keys.
        let m = GoMap::from_map(vec![
            ("v2.10".into(), GoValue::Int(1)),
            ("v2.9".into(), GoValue::Int(2)),
            ("v2.1".into(), GoValue::Int(3)),
        ]);
        v.push(c(
            "map_version_keys",
            GoValue::Map(m),
            r#"map[string]interface{}{"v2.10": int(1), "v2.9": int(2), "v2.1": int(3)}"#,
        ));
    }

    // --- sequences (top-level and as map values) --------------------------
    v.push(c(
        "seq_scalars",
        GoValue::Array(vec![GoValue::Int(1), GoValue::Int(2), GoValue::Int(3)]),
        "[]interface{}{int(1), int(2), int(3)}",
    ));
    v.push(c(
        "empty_seq",
        GoValue::Array(vec![]),
        "[]interface{}{}",
    ));
    {
        let m = GoMap::from_map(vec![(
            "nums".into(),
            GoValue::Array(vec![GoValue::Int(112_539)]),
        )]);
        v.push(c(
            "map_with_seq",
            GoValue::Map(m),
            r#"map[string]interface{}{"nums": []interface{}{int(112539)}}"#,
        ));
    }

    // --- mixed nested map (exercises indent + sorting + scalar quoting) ----
    {
        let m = GoMap::from_map(vec![
            ("label".into(), GoValue::Str("yes".into())),
            ("ratio".into(), GoValue::Float(0.5)),
            ("count".into(), GoValue::Int(7)),
            ("ok".into(), GoValue::Bool(true)),
            ("note".into(), GoValue::Str("a: b".into())),
        ]);
        v.push(c(
            "map_mixed",
            GoValue::Map(m),
            r#"map[string]interface{}{"label": "yes", "ratio": float64(0.5), "count": int(7), "ok": true, "note": "a: b"}"#,
        ));
    }

    v
}

/// Render a Go double-quoted string literal byte-for-byte from a Rust `&str`.
fn go_string(s: &str) -> String {
    let mut out = String::from("\"");
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

const SENTINEL: &str = "\n@@CF_GOYAML_CASE@@\n";

/// Path to the Go module root (repository root, two levels above `rust/`).
fn go_module_dir() -> std::path::PathBuf {
    // CARGO_MANIFEST_DIR = .../crates/cf-goyaml
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent() // crates
        .and_then(|p| p.parent()) // rust
        .and_then(|p| p.parent()) // repo root
        .expect("repo root")
        .to_path_buf()
}

/// Emit the Go oracle program: marshals every case in order, separating the
/// raw `yaml.Marshal` bytes of each with `SENTINEL`.
fn build_go_program(cases: &[Case]) -> String {
    let mut prog = String::new();
    prog.push_str("package main\n\n");
    prog.push_str("import (\n\t\"os\"\n\t\"gopkg.in/yaml.v3\"\n)\n\n");
    prog.push_str("func main() {\n");
    prog.push_str("\tcases := []interface{}{\n");
    for case in cases {
        prog.push_str("\t\tinterface{}(");
        prog.push_str(&case.go_expr);
        prog.push_str("),\n");
    }
    prog.push_str("\t}\n");
    prog.push_str("\tfor i, v := range cases {\n");
    prog.push_str("\t\tif i > 0 {\n");
    prog.push_str(&format!("\t\t\tos.Stdout.WriteString({:?})\n", SENTINEL));
    prog.push_str("\t\t}\n");
    prog.push_str("\t\tb, err := yaml.Marshal(v)\n");
    prog.push_str("\t\tif err != nil { panic(err) }\n");
    prog.push_str("\t\tos.Stdout.Write(b)\n");
    prog.push_str("\t}\n");
    prog.push_str("}\n");
    prog
}

#[test]
fn yaml_v3_byte_parity_via_go_run() {
    // Skip cleanly if there's no Go toolchain.
    if Command::new("go").arg("version").output().is_err() {
        eprintln!("`go` not found; skipping yaml.v3 parity oracle");
        return;
    }
    let module_dir = go_module_dir();
    if !module_dir.join("go.mod").exists() {
        eprintln!("go.mod not found at {module_dir:?}; skipping yaml.v3 parity oracle");
        return;
    }

    let cases = cases();
    let program = build_go_program(&cases);

    // Write the program into a temp dir, but run it from the module dir so it
    // resolves `gopkg.in/yaml.v3` from the repository's own go.mod/go.sum.
    let tmp = std::env::temp_dir().join(format!("cf_goyaml_oracle_{}.go", std::process::id()));
    std::fs::write(&tmp, &program).expect("write go oracle program");

    let output = Command::new("go")
        .current_dir(&module_dir)
        .arg("run")
        .arg(&tmp)
        .env("GOFLAGS", "-mod=mod")
        .output()
        .expect("run go oracle");
    let _ = std::fs::remove_file(&tmp);

    if !output.status.success() {
        // No network / module download? Treat as a skip rather than a failure
        // so offline CI is never blocked, but surface the reason.
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("cannot find") || stderr.contains("no required module")
            || stderr.contains("dial tcp") || stderr.contains("connection refused")
            || stderr.contains("verifying") || stderr.contains("download")
        {
            eprintln!("go run could not resolve yaml.v3 (offline?); skipping:\n{stderr}");
            return;
        }
        panic!("go oracle failed:\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout), stderr);
    }

    let truth = output.stdout;
    let expected: Vec<&[u8]> = split_sentinel(&truth);
    assert_eq!(
        expected.len(),
        cases.len(),
        "oracle produced {} chunks for {} cases",
        expected.len(),
        cases.len()
    );

    let mut failures = 0usize;
    for (case, want) in cases.iter().zip(expected.iter()) {
        let got = cf_goyaml::marshal(&case.value);
        if got != *want {
            failures += 1;
            eprintln!(
                "MISMATCH [{}] go_expr={}\n  expected={:?}\n  got     ={:?}",
                case.name,
                case.go_expr,
                String::from_utf8_lossy(want),
                String::from_utf8_lossy(&got),
            );
        }
    }
    assert_eq!(failures, 0, "{failures}/{} yaml.v3 parity mismatches", cases.len());
    eprintln!("yaml.v3 oracle: {} cases, all byte-identical", cases.len());
}

fn split_sentinel(data: &[u8]) -> Vec<&[u8]> {
    let sep = SENTINEL.as_bytes();
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i + sep.len() <= data.len() {
        if &data[i..i + sep.len()] == sep {
            out.push(&data[start..i]);
            i += sep.len();
            start = i;
        } else {
            i += 1;
        }
    }
    out.push(&data[start..]);
    out
}
