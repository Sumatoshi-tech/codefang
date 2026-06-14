//! Every canonical key/form compiles.
use cf_uast_mapping::{uast_language, LanguageMapping};

static OK: LanguageMapping = uast_language! {
    name: "ok",
    extensions: [".ok"],
    files: ["Okfile"],
    rules: {
        minimal => {
            type: Synthetic,
        },
        full ("(full name: (identifier) @name)") => {
            extends: minimal,
            type: Function,
            token: capture("name"),
            roles: [Function, Declaration],
            children: ["block"],
            props: { "k": "v" },
            when: ["f == \"v\""],
        },
    }
};

fn main() {
    assert_eq!(OK.rules.len(), 2);
}
