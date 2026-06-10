//! `type:` is required on every rule.
use cf_uast_mapping::{uast_language, LanguageMapping};

static BAD: LanguageMapping = uast_language! {
    name: "bad",
    extensions: [".bad"],
    rules: {
        r => {
            token: self,
        },
    }
};

fn main() {}
