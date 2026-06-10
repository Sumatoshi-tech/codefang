//! CLI entry point: `uastmap2rs [--escape-hatch <rule_name>]... <file.uastmap>`
//! prints the transpiled Rust module to stdout. See the library docs for the
//! emission contract.

#![forbid(unsafe_code)]

use std::process::ExitCode;

use uastmap2rs::Options;

fn main() -> ExitCode {
    let mut opts = Options::default();
    let mut input: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--escape-hatch" => match args.next() {
                Some(name) => {
                    opts.escape_hatch.insert(name);
                }
                None => return usage("--escape-hatch requires a rule name"),
            },
            "--help" | "-h" => {
                eprintln!("usage: uastmap2rs [--escape-hatch <rule_name>]... <file.uastmap>");
                return ExitCode::SUCCESS;
            }
            _ if input.is_none() => input = Some(arg),
            _ => return usage("exactly one input file expected"),
        }
    }
    let Some(path) = input else {
        return usage("missing input file");
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("uastmap2rs: {path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let source_name = std::path::Path::new(&path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.clone());
    match uastmap2rs::transpile(&content, &source_name, &opts) {
        Ok(module) => {
            print!("{module}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("uastmap2rs: {e}");
            ExitCode::FAILURE
        }
    }
}

fn usage(msg: &str) -> ExitCode {
    eprintln!("uastmap2rs: {msg}");
    eprintln!("usage: uastmap2rs [--escape-hatch <rule_name>]... <file.uastmap>");
    ExitCode::FAILURE
}
