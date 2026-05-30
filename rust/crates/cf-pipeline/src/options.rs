//! Configuration-option description types used to build CLI flags
//! (`options.go`).
//!
//! `cf-commands` iterates each analyzer's configuration options to register one
//! clap flag per option, so the option's name, default, help text, and rendered
//! type/default strings must match the Go originals byte-for-byte to keep the
//! CLI golden (DESIGN §4.1) clean.

/// The possible types of a [`ConfigurationOption`]'s value.
///
/// The discriminant order mirrors Go's `iota` declaration order so that any
/// numeric persistence (e.g. config dumps) round-trips identically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConfigurationOptionType {
    /// Boolean value type.
    Bool,
    /// Integer value type.
    Int,
    /// String value type.
    String,
    /// Floating-point value type.
    Float,
    /// Array-of-strings value type.
    Strings,
    /// Filesystem-path value type.
    Path,
}

impl ConfigurationOptionType {
    /// Returns the integer discriminant matching Go's `iota` ordering
    /// (`Bool=0`, `Int=1`, `String=2`, `Float=3`, `Strings=4`, `Path=5`).
    #[must_use]
    pub fn discriminant(self) -> i64 {
        match self {
            ConfigurationOptionType::Bool => 0,
            ConfigurationOptionType::Int => 1,
            ConfigurationOptionType::String => 2,
            ConfigurationOptionType::Float => 3,
            ConfigurationOptionType::Strings => 4,
            ConfigurationOptionType::Path => 5,
        }
    }
}

impl std::fmt::Display for ConfigurationOptionType {
    /// Reproduces Go's `ConfigurationOptionType.String()`: an empty string for
    /// the boolean type, `"int"` for integers, `"string"` for strings and
    /// string-slices, `"float"` for floats, and `"path"` for paths. Used in the
    /// CLI to show the argument's type.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ConfigurationOptionType::Bool => "",
            ConfigurationOptionType::Int => "int",
            ConfigurationOptionType::String => "string",
            ConfigurationOptionType::Float => "float",
            ConfigurationOptionType::Strings => "string",
            ConfigurationOptionType::Path => "path",
        };
        f.write_str(s)
    }
}

/// The default value carried by a [`ConfigurationOption`].
///
/// Go stores the default as `any`; the variants here cover every value
/// `FormatDefault` and the CLI flag registration can encounter (Bool / Int /
/// String / Float / Strings / Path). [`DefaultValue::format`] reproduces the
/// `fmt.Sprint` / `fmt.Sprintf("%q", ...)` rendering that Go uses.
#[derive(Debug, Clone, PartialEq)]
pub enum DefaultValue {
    /// Boolean default (`fmt.Sprint` → `true` / `false`).
    Bool(bool),
    /// Integer default (`fmt.Sprint` → decimal).
    Int(i64),
    /// String default (`fmt.Sprintf("%q", ...)` → Go-quoted).
    String(String),
    /// Float default (`fmt.Sprint` → Go's `%v` float rendering).
    Float(f64),
    /// String-slice default (joined with `,` then `%q`-quoted).
    Strings(Vec<String>),
    /// Path default (rendered like a string via `fmt.Sprint`).
    Path(String),
}

/// Allows for the unified, retrospective way to set up pipeline items.
///
/// Field set mirrors Go's `ConfigurationOption` struct.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigurationOption {
    /// The initial value of the configuration option.
    pub default: DefaultValue,
    /// Identifies the configuration option in facts.
    pub name: String,
    /// Help text about the configuration option.
    pub description: String,
    /// The CLI token, with `--` prepended.
    pub flag: String,
    /// The kind of the configuration option's value.
    pub option_type: ConfigurationOptionType,
}

impl ConfigurationOption {
    /// Converts the default value to a string for CLI display.
    ///
    /// Reproduces Go's `ConfigurationOption.FormatDefault`:
    ///
    /// - `StringsConfigurationOption` → `fmt.Sprintf("%q", strings.Join(slice, ","))`
    ///   (the joined, comma-separated values, Go-double-quoted). If the stored
    ///   default is not actually a string slice, Go falls back to
    ///   `fmt.Sprint(default)`; that fallback cannot occur here because the type
    ///   tag and the value variant are coupled.
    /// - `StringConfigurationOption` → `fmt.Sprintf("%q", default)` (Go-quoted).
    /// - everything else → `fmt.Sprint(default)`.
    #[must_use]
    pub fn format_default(&self) -> String {
        if self.option_type == ConfigurationOptionType::Strings {
            let joined = match &self.default {
                DefaultValue::Strings(items) => items.join(","),
                // Fallback path: `fmt.Sprint` of whatever default is present.
                other => return go_sprint(other),
            };
            return go_quote(&joined);
        }

        if self.option_type != ConfigurationOptionType::String {
            return go_sprint(&self.default);
        }

        // String option: %q-quote the default. For a non-string default Go
        // would call `fmt.Sprintf("%q", v)` on a non-string, but our typed
        // variant guarantees a string here in practice; handle defensively.
        match &self.default {
            DefaultValue::String(s) | DefaultValue::Path(s) => go_quote(s),
            other => go_sprint(other),
        }
    }
}

/// Reproduces Go's `fmt.Sprint(v)` for the default-value variants we carry.
fn go_sprint(v: &DefaultValue) -> String {
    match v {
        DefaultValue::Bool(b) => {
            if *b {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        DefaultValue::Int(i) => i.to_string(),
        DefaultValue::String(s) | DefaultValue::Path(s) => s.clone(),
        DefaultValue::Float(fl) => go_float_v(*fl),
        // `fmt.Sprint` on a []string prints `[a b c]` (space-separated, bracketed).
        DefaultValue::Strings(items) => {
            let mut out = String::from("[");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                out.push_str(item);
            }
            out.push(']');
            out
        }
    }
}

/// Reproduces Go's `fmt.Sprintf("%q", s)` — a Go double-quoted string literal.
///
/// Go's `%q` quotes with `strconv.Quote`: wrap in double quotes, escape `"` and
/// `\`, render `\a \b \f \n \r \t \v` shortcuts, escape other non-printable /
/// control bytes as `\xNN` / `\uNNNN` / `\UNNNNNNNN`. For the CLI-default
/// strings this package handles (flag/identifier-like values), the common cases
/// are plain ASCII; the escapes below cover the standard control set so the
/// output matches `strconv.Quote` for typical defaults.
fn go_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0007}' => out.push_str("\\a"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{000B}' => out.push_str("\\v"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Reproduces Go's default float rendering for `fmt.Sprint`/`%v`, which uses
/// `strconv.FormatFloat(f, 'g', -1, 64)` shortest form. For the integer-valued
/// and simple defaults used in CLI options this matches Go; arbitrary-precision
/// `'g'` parity with Go is the concern of `cf-gojson` (out of scope for this
/// crate, which never emits machine bytes).
fn go_float_v(f: f64) -> String {
    if f == f.trunc() && f.is_finite() && f.abs() < 1e21 {
        // Integer-valued: Go's %v prints without a decimal point (e.g. `0`, `1`).
        format!("{}", f as i64)
    } else {
        let mut s = format!("{f}");
        // Rust prints `inf`/`-inf`/`NaN`; Go prints `+Inf`/`-Inf`/`NaN`.
        if s == "inf" {
            s = "+Inf".to_string();
        } else if s == "-inf" {
            s = "-Inf".to_string();
        } else if s == "NaN" {
            s = "NaN".to_string();
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_string_matches_go() {
        assert_eq!(ConfigurationOptionType::Bool.to_string(), "");
        assert_eq!(ConfigurationOptionType::Int.to_string(), "int");
        assert_eq!(ConfigurationOptionType::String.to_string(), "string");
        assert_eq!(ConfigurationOptionType::Float.to_string(), "float");
        assert_eq!(ConfigurationOptionType::Strings.to_string(), "string");
        assert_eq!(ConfigurationOptionType::Path.to_string(), "path");
    }

    #[test]
    fn discriminant_matches_iota() {
        assert_eq!(ConfigurationOptionType::Bool.discriminant(), 0);
        assert_eq!(ConfigurationOptionType::Int.discriminant(), 1);
        assert_eq!(ConfigurationOptionType::String.discriminant(), 2);
        assert_eq!(ConfigurationOptionType::Float.discriminant(), 3);
        assert_eq!(ConfigurationOptionType::Strings.discriminant(), 4);
        assert_eq!(ConfigurationOptionType::Path.discriminant(), 5);
    }

    fn opt(t: ConfigurationOptionType, d: DefaultValue) -> ConfigurationOption {
        ConfigurationOption {
            default: d,
            name: "n".into(),
            description: "d".into(),
            flag: "f".into(),
            option_type: t,
        }
    }

    #[test]
    fn format_default_bool() {
        assert_eq!(
            opt(ConfigurationOptionType::Bool, DefaultValue::Bool(true)).format_default(),
            "true"
        );
        assert_eq!(
            opt(ConfigurationOptionType::Bool, DefaultValue::Bool(false)).format_default(),
            "false"
        );
    }

    #[test]
    fn format_default_int() {
        assert_eq!(
            opt(ConfigurationOptionType::Int, DefaultValue::Int(0)).format_default(),
            "0"
        );
        assert_eq!(
            opt(ConfigurationOptionType::Int, DefaultValue::Int(-7)).format_default(),
            "-7"
        );
    }

    #[test]
    fn format_default_string_is_quoted() {
        // Go: fmt.Sprintf("%q", "json") => "\"json\""
        assert_eq!(
            opt(
                ConfigurationOptionType::String,
                DefaultValue::String("json".into())
            )
            .format_default(),
            "\"json\""
        );
        // Empty string default => "\"\"".
        assert_eq!(
            opt(
                ConfigurationOptionType::String,
                DefaultValue::String(String::new())
            )
            .format_default(),
            "\"\""
        );
    }

    #[test]
    fn format_default_string_escapes() {
        assert_eq!(
            opt(
                ConfigurationOptionType::String,
                DefaultValue::String("a\tb\"c".into())
            )
            .format_default(),
            "\"a\\tb\\\"c\""
        );
    }

    #[test]
    fn format_default_strings_joined_and_quoted() {
        // Go: fmt.Sprintf("%q", strings.Join([]string{"a","b"}, ",")) => "\"a,b\""
        assert_eq!(
            opt(
                ConfigurationOptionType::Strings,
                DefaultValue::Strings(vec!["a".into(), "b".into()])
            )
            .format_default(),
            "\"a,b\""
        );
        // Empty slice joins to "" then quotes to "\"\"".
        assert_eq!(
            opt(
                ConfigurationOptionType::Strings,
                DefaultValue::Strings(vec![])
            )
            .format_default(),
            "\"\""
        );
    }

    #[test]
    fn format_default_float() {
        // Integer-valued float prints without a point, like Go's %v.
        assert_eq!(
            opt(ConfigurationOptionType::Float, DefaultValue::Float(0.0)).format_default(),
            "0"
        );
        assert_eq!(
            opt(ConfigurationOptionType::Float, DefaultValue::Float(1.5)).format_default(),
            "1.5"
        );
    }

    #[test]
    fn format_default_path() {
        // Path is not StringConfigurationOption, so fmt.Sprint (unquoted).
        assert_eq!(
            opt(
                ConfigurationOptionType::Path,
                DefaultValue::Path("/tmp/x".into())
            )
            .format_default(),
            "/tmp/x"
        );
    }
}
