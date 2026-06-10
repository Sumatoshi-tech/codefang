//! An unknown rule key must not parse.
use cf_uast_mapping::{uast_language, LanguageMapping};

static BAD: LanguageMapping = uast_language! {
    name: "bad",
    extensions: [".bad"],
    rules: {
        r => {
            type: Synthetic,
            tokens: self,
        },
    }
};

fn main() {}
