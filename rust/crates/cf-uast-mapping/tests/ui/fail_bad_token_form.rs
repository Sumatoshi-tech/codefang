//! Only self / child("…") / capture("…") are valid token forms.
use cf_uast_mapping::{uast_language, LanguageMapping};

static BAD: LanguageMapping = uast_language! {
    name: "bad",
    extensions: [".bad"],
    rules: {
        r => {
            type: Synthetic,
            token: descendant("x"),
        },
    }
};

fn main() {}
