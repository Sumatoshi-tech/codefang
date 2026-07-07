//! The [`uast_language!`](crate::uast_language) declarative macro — the
//! Rust-native syntax for one
//! language's mapping table (specs/uastmap-rust-macros).
//!
//! The macro is an EXPRESSION expanding to a [`crate::LanguageMapping`] literal,
//! so a language module writes:
//!
//! ```
//! use cf_uast_mapping::{uast_language, LanguageMapping};
//!
//! pub static T: LanguageMapping = uast_language! {
//!     name: "t",
//!     extensions: [".t"],
//!     rules: {
//!         assignment_statement => {
//!             type: Assignment,
//!             token: self,
//!             roles: [Assignment],
//!             children: ["expression_list"],
//!         },
//!         qualified_type ("(qualified_type package: (package_identifier) @pkg)") => {
//!             type: Synthetic,
//!             token: capture("pkg"),
//!         },
//!     }
//! };
//! assert_eq!(T.rules.len(), 2);
//! ```
//!
//! # Syntax
//!
//! Keys follow a CANONICAL ORDER (what the transpiler emits); every key except
//! `type` is optional:
//!
//! ```text
//! name: "<language>",
//! extensions: ["<.ext>", ...],
//! files: ["<FileName>", ...],            // optional
//! rules: {
//!     <rule_name> [("<s-expr pattern>")] => {
//!         extends: <base_rule>,           // optional ('# Extends' inheritance)
//!         type: <UastType variant>,       // required; typo = compile error
//!         token: self | child("<type>") | capture("<name>"),   // optional
//!         roles: [<Role variant>, ...],   // optional
//!         children: ["<rule>", ...],      // optional
//!         props: { "<key>": "<value>", ... },                  // optional
//!         when: ["<condition expr>", ...],                     // optional
//!     },
//!     ...
//! }
//! ```
//!
//! When the pattern is omitted it defaults to `"(<rule_name>)"`, mirroring the
//! dominant DSL form `name <- (name) => uast(...)`. Rule names may be any
//! tree-sitter node identifier, including Rust keywords (`loop`, `type`, …) —
//! the macro only ever `stringify!`s them. The expansion is pure repetition
//! (no tt-munching recursion), so corpus-scale invocations (~700 rules) stay
//! far from `recursion_limit`.

/// Expands to a [`crate::LanguageMapping`] expression. See the [module
/// docs](self) for the full syntax.
#[macro_export]
macro_rules! uast_language {
    (
        name: $lang:literal,
        extensions: [$($ext:literal),* $(,)?],
        $(files: [$($file:literal),* $(,)?],)?
        rules: {
            $(
                $rname:ident $(($rpattern:literal))? => {
                    $(extends: $extends:ident,)?
                    type: $rtype:ident,
                    $(token: $tokkind:ident $(($tokarg:literal))?,)?
                    $(roles: [$($role:ident),* $(,)?],)?
                    $(children: [$($child:literal),* $(,)?],)?
                    $(props: { $($pk:literal: $pv:literal),* $(,)? },)?
                    $(when: [$($cond:literal),* $(,)?],)?
                }
            ),* $(,)?
        } $(,)?
    ) => {
        $crate::LanguageMapping {
            name: $lang,
            extensions: &[$($ext),*],
            files: &[$($($file),*)?],
            rules: &[
                $(
                    $crate::MappingRule {
                        name: stringify!($rname),
                        pattern: $crate::__uast_pattern!($rname $(, $rpattern)?),
                        extends: $crate::__uast_extends!($($extends)?),
                        uast_type: $crate::UastType::$rtype,
                        token: $crate::__uast_token!($($tokkind $(($tokarg))?)?),
                        roles: &[$($($crate::Role::$role),*)?],
                        children: &[$($($child),*)?],
                        props: &[$($(($pk, $pv)),*)?],
                        conditions: &[$($($cond),*)?],
                    }
                ),*
            ],
        }
    };
}

/// `__uast_pattern!(name)` → `"(name)"`; `__uast_pattern!(name, "(…)")` → the
/// explicit pattern. Internal to [`uast_language!`].
#[doc(hidden)]
#[macro_export]
macro_rules! __uast_pattern {
    ($name:ident) => {
        concat!("(", stringify!($name), ")")
    };
    ($name:ident, $pattern:literal) => {
        $pattern
    };
}

/// `__uast_extends!()` → `""`; `__uast_extends!(base)` → `"base"`. Internal to
/// [`uast_language!`].
#[doc(hidden)]
#[macro_export]
macro_rules! __uast_extends {
    () => {
        ""
    };
    ($base:ident) => {
        stringify!($base)
    };
}

/// Maps the `token:` forms to [`crate::TokenSource`]; absent → `None`. Internal
/// to [`uast_language!`].
#[doc(hidden)]
#[macro_export]
macro_rules! __uast_token {
    () => {
        $crate::TokenSource::None
    };
    (self) => {
        $crate::TokenSource::SelfText
    };
    (child($t:literal)) => {
        $crate::TokenSource::Child($t)
    };
    (capture($c:literal)) => {
        $crate::TokenSource::Capture($c)
    };
}

#[cfg(test)]
mod tests {
    use crate::{LanguageMapping, MappingRule, Role, TokenSource, UastType};

    /// The macro form of `static_model::tests::TWO_RULES` — the representative
    /// invocation the roadmap requires to equal the hand-written static.
    static TWO_RULES_MACRO: LanguageMapping = uast_language! {
        name: "t",
        extensions: [".t"],
        rules: {
            assignment_statement => {
                type: Assignment,
                token: self,
                roles: [Assignment],
                children: ["expression_list"],
            },
            qualified_type ("(qualified_type package: (package_identifier) @pkg)") => {
                type: Synthetic,
                token: capture("pkg"),
                props: { "custom_prop": "v" },
            },
        }
    };

    /// Kitchen sink: every macro key at once, including the constructs the
    /// corpus never uses (`extends`, `when`, `files`, keyword rule names).
    static KITCHEN_SINK: LanguageMapping = uast_language! {
        name: "sink",
        extensions: [".a", ".b"],
        files: ["Sinkfile"],
        rules: {
            base_rule => {
                type: Synthetic,
            },
            loop ("(loop body: (_) @body)") => {
                extends: base_rule,
                type: Loop,
                token: child("identifier"),
                roles: [Loop, Body],
                children: ["_expression", "_statement"],
                props: { "kind": "while", "style": "c" },
                when: ["field == \"v\"", "other != \"bad\""],
            },
        }
    };

    #[test]
    fn macro_matches_hand_written_static() {
        let expected = LanguageMapping {
            name: "t",
            extensions: &[".t"],
            files: &[],
            rules: &[
                MappingRule {
                    name: "assignment_statement",
                    pattern: "(assignment_statement)",
                    extends: "",
                    uast_type: UastType::Assignment,
                    token: TokenSource::SelfText,
                    roles: &[Role::Assignment],
                    children: &["expression_list"],
                    props: &[],
                    conditions: &[],
                },
                MappingRule {
                    name: "qualified_type",
                    pattern: "(qualified_type package: (package_identifier) @pkg)",
                    extends: "",
                    uast_type: UastType::Synthetic,
                    token: TokenSource::Capture("pkg"),
                    roles: &[],
                    children: &[],
                    props: &[("custom_prop", "v")],
                    conditions: &[],
                },
            ],
        };
        assert_eq!(TWO_RULES_MACRO, expected);
        // And the converted Vec<Rule> matches too (the to_rules path).
        assert_eq!(TWO_RULES_MACRO.to_rules(), expected.to_rules());
    }

    #[test]
    fn kitchen_sink_covers_every_construct() {
        assert_eq!(KITCHEN_SINK.files, &["Sinkfile"]);
        let r = &KITCHEN_SINK.rules[1];
        assert_eq!(r.name, "loop"); // keyword rule names stringify fine.
        assert_eq!(r.pattern, "(loop body: (_) @body)");
        assert_eq!(r.extends, "base_rule");
        assert_eq!(r.uast_type, UastType::Loop);
        assert_eq!(r.token, TokenSource::Child("identifier"));
        assert_eq!(r.roles, &[Role::Loop, Role::Body]);
        assert_eq!(r.children, &["_expression", "_statement"]);
        assert_eq!(r.props, &[("kind", "while"), ("style", "c")]);
        assert_eq!(r.conditions, &["field == \"v\"", "other != \"bad\""]);

        // Defaulted keys on the minimal rule.
        let b = &KITCHEN_SINK.rules[0];
        assert_eq!(b.pattern, "(base_rule)");
        assert_eq!(b.token, TokenSource::None);
        assert!(b.roles.is_empty() && b.props.is_empty() && b.conditions.is_empty());
    }
}
