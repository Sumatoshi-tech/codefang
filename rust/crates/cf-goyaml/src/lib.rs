//! `cf-goyaml` — the report-format block YAML emitter over
//! [`cf_gojson::GoValue`].
//!
//! The emitted bytes are a frozen contract for the value shapes codefang
//! reports use:
//!
//! * 2..9-space block indent (default **4**), with the reference emitter's
//!   exact indent-rounding and "skip the `- `" rule;
//! * **no** leading `---` and a single trailing `\n`;
//! * map keys in [`GoMap::encode_order`] order — struct-origin keeps
//!   declaration order, map-origin sorts;
//! * scalar quoting: plain unless the value would *resolve* to a non-`str`
//!   tag (numbers / bools / null / yes-no-on-off / timestamps / base-60), then
//!   double-quoted; structural-indicator strings fall back to single quotes;
//! * plain/single/double-quoted writers including line folding and
//!   `\xNN`/`\uNNNN` escaping;
//! * floats in the shortest-precision `'g'` layout with a two-digit exponent
//!   (see the `float` module; NOT the JSON float layout).
//!
//! The internals deliberately mirror the reference emitter's state machine so
//! parity can be audited mechanically; treat the algorithms as frozen. The
//! public entry point is [`marshal`].
//!
//! Compatibility: output bytes are pinned against the reference implementation
//! (`gopkg.in/yaml.v3` v3.0.1 `Marshal`) by this crate's oracle tests and the
//! differential gate in `rust/tests/compat`.

use cf_gojson::GoValue;

mod emitter;
mod float;
mod resolve;
mod scalar;

/// Crate name, used by smoke tests to confirm the module links.
pub const CRATE_NAME: &str = "cf-goyaml";

/// Serializes a [`GoValue`] to report-contract block YAML with the default
/// 4-space indent.
#[must_use]
pub fn marshal(value: &GoValue) -> Vec<u8> {
    marshal_indent(value, 4)
}

/// Like [`marshal`] but with a caller-chosen indent (clamped to `2..=9`,
/// defaulting to 2 outside that range — reference-emitter behavior).
#[must_use]
pub fn marshal_indent(value: &GoValue, indent: i32) -> Vec<u8> {
    let mut e = emitter::Emitter::new(indent);
    e.marshal_document(value);
    e.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_gojson::{GoMap, GoValue};

    fn s(value: &GoValue) -> String {
        String::from_utf8(marshal(value)).unwrap()
    }

    fn smap(pairs: Vec<(&str, GoValue)>) -> GoValue {
        let mut m = GoMap::new_struct();
        for (k, v) in pairs {
            m.push(k, v);
        }
        GoValue::Map(m)
    }

    #[test]
    fn scalar_document() {
        assert_eq!(s(&GoValue::Int(42)), "42\n");
        assert_eq!(s(&GoValue::Str("hi".into())), "hi\n");
        assert_eq!(s(&GoValue::Bool(true)), "true\n");
        assert_eq!(s(&GoValue::Str("<unknown>".into())), "<unknown>\n");
    }

    #[test]
    fn struct_keeps_order() {
        let v = smap(vec![
            ("total_commits", GoValue::Int(1)),
            ("total_lines_added", GoValue::Int(0)),
        ]);
        assert_eq!(s(&v), "total_commits: 1\ntotal_lines_added: 0\n");
    }

    #[test]
    fn nil_slice_emits_empty_sequence() {
        // YAML marshals a nil slice as `[]`, identical to an empty slice (the
        // JSON encoder writes `null`). As a struct field it appears inline.
        assert_eq!(s(&GoValue::NilSlice), "[]\n");
        let v = smap(vec![("anomalies", GoValue::NilSlice)]);
        assert_eq!(s(&v), "anomalies: []\n");
    }

    #[test]
    fn map_origin_sorts() {
        let m = GoMap::from_map(vec![
            ("b".into(), GoValue::Int(2)),
            ("a".into(), GoValue::Int(1)),
        ]);
        assert_eq!(s(&GoValue::Map(m)), "a: 1\nb: 2\n");
    }

    #[test]
    fn top_level_seq_indents_four() {
        let item = smap(vec![("name", GoValue::Str("X".into())), ("v", GoValue::Int(17))]);
        let v = smap(vec![("function_complexity", GoValue::Array(vec![item]))]);
        assert_eq!(s(&v), "function_complexity:\n    - name: X\n      v: 17\n");
    }

    #[test]
    fn nested_seq_indent() {
        let bydev = smap(vec![("dev_id", GoValue::Int(0)), ("commits", GoValue::Int(1))]);
        let act = smap(vec![
            ("tick", GoValue::Int(0)),
            ("by_developer", GoValue::Array(vec![bydev])),
            ("total_commits", GoValue::Int(1)),
        ]);
        let v = smap(vec![("activity", GoValue::Array(vec![act]))]);
        let expect = "activity:\n    - tick: 0\n      by_developer:\n        - dev_id: 0\n          commits: 1\n      total_commits: 1\n";
        assert_eq!(s(&v), expect);
    }

    #[test]
    fn empty_containers() {
        let v = smap(vec![
            ("languages", GoValue::Array(vec![])),
            ("m", GoValue::Map(GoMap::new_struct())),
        ]);
        assert_eq!(s(&v), "languages: []\nm: {}\n");
    }

    #[test]
    fn seq_of_scalars() {
        let v = smap(vec![("band_breakdown", GoValue::Array(vec![GoValue::Int(112539)]))]);
        assert_eq!(s(&v), "band_breakdown:\n    - 112539\n");
    }

    #[test]
    fn scalar_quoting() {
        let cases: &[(&str, &str)] = &[
            ("true", "\"true\""),
            ("123", "\"123\""),
            ("", "\"\""),
            ("null", "\"null\""),
            ("yes", "\"yes\""),
            ("no", "\"no\""),
            ("on", "\"on\""),
            ("off", "\"off\""),
            ("~", "\"~\""),
            ("1.5", "\"1.5\""),
            ("hello", "hello"),
            ("a:b", "a:b"),
            ("a: b", "'a: b'"),
            ("a #b", "'a #b'"),
            ("@foo", "'@foo'"),
            ("-x", "-x"),
            ("!x", "'!x'"),
            ("?x", "?x"),
            ("[x]", "'[x]'"),
            ("{x}", "'{x}'"),
            ("&x", "'&x'"),
            ("*x", "'*x'"),
            ("%x", "'%x'"),
            ("a,b", "a,b"),
            ("2026-01-26T21:53:53Z", "\"2026-01-26T21:53:53Z\""),
            ("CRITICAL", "CRITICAL"),
            (".", "."),
            ("+5", "\"+5\""),
            ("0", "\"0\""),
            ("it's", "it's"),
            ("say \"hi\"", "say \"hi\""),
            (" x", "' x'"),
            ("x ", "'x '"),
        ];
        for (input, want) in cases {
            let got = s(&GoValue::Str((*input).into()));
            assert_eq!(got, format!("{want}\n"), "input={input:?}");
        }
    }

    #[test]
    fn ctrl_char_double_quoted() {
        assert_eq!(s(&GoValue::Str("a\u{01}b".into())), "\"a\\x01b\"\n");
        assert_eq!(s(&GoValue::Str("a\tb".into())), "\"a\\tb\"\n");
    }

    #[test]
    fn newline_literal_block() {
        // map value with a newline -> literal block scalar |-
        let v = smap(vec![("k", GoValue::Str("a\nb".into()))]);
        assert_eq!(s(&v), "k: |-\n    a\n    b\n");
    }

    #[test]
    fn floats_g_format() {
        let v = smap(vec![
            ("f1", GoValue::Float(0.7142857142857143)),
            ("f3", GoValue::Float(1.0)),
            ("f5", GoValue::Float(0.0)),
            ("f7", GoValue::Float(1e20)),
            ("f8", GoValue::Float(1e-7)),
            ("f10", GoValue::Float(123456789.123456789)),
        ]);
        let expect = "f1: 0.7142857142857143\nf3: 1\nf5: 0\nf7: 1e+20\nf8: 1e-07\nf10: 1.2345678912345679e+08\n";
        assert_eq!(s(&v), expect);
    }
}
