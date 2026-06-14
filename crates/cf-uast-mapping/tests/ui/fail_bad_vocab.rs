//! A typo'd UastType variant is a compile error (the closed-vocabulary win).
use cf_uast_mapping::{uast_language, LanguageMapping};

static BAD: LanguageMapping = uast_language! {
    name: "bad",
    extensions: [".bad"],
    rules: {
        r => {
            type: Assigment,
        },
    }
};

fn main() {}
