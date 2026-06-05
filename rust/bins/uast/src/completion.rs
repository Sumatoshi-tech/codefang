//! `uast completion [shell]` — shell completion scripts (port of
//! `cmd/uast/completion.go`).
//!
//! Go builds a *separate* root command (Short "Unified AST - Parse, analyze, and
//! transform code across 100+ languages") registering only parse, query,
//! analyze, diff, explore, completion, version, then calls cobra's per-shell
//! generator. The script bytes are clap-vs-cobra cosmetic (Layer-D informational,
//! DESIGN §6), so we generate best-effort via `clap_complete` against a command
//! that mirrors the same subset; the shell set and the `unsupported shell` error
//! wording are reproduced exactly.

use std::io;

use clap::{Arg, ArgMatches, Command};
use clap_complete::{generate, Shell};

/// Builds the `completion` subcommand (completion.go:14-30).
pub fn command() -> Command {
    Command::new("completion")
        .about("Generate shell completion scripts")
        .arg(Arg::new("shell").required(true).index(1))
}

/// Runs `completion` (completion.go `runCompletion`).
pub fn run(m: &ArgMatches) -> Result<(), String> {
    let shell = m.get_one::<String>("shell").map(String::as_str).unwrap_or("");
    run_for_shell(shell)
}

/// Generates the completion script for `shell`, or errors `unsupported shell:
/// <shell>` for an unknown shell (completion.go default arm).
fn run_for_shell(shell: &str) -> Result<(), String> {
    let sh = match shell {
        "bash" => Shell::Bash,
        "zsh" => Shell::Zsh,
        "fish" => Shell::Fish,
        "powershell" => Shell::PowerShell,
        other => return Err(format!("unsupported shell: {other}")),
    };

    let mut root = completion_root();
    generate(sh, &mut root, "uast", &mut io::stdout());
    Ok(())
}

/// The dedicated completion root (completion.go `runCompletion` registers this
/// subset with a different Short string).
fn completion_root() -> Command {
    Command::new("uast")
        .about("Unified AST - Parse, analyze, and transform code across 100+ languages")
        .subcommand(crate::parse::command())
        .subcommand(crate::query::command())
        .subcommand(crate::analyze::command())
        .subcommand(crate::diff::command())
        .subcommand(crate::explore::command())
        .subcommand(command())
        .subcommand(Command::new("version").about("Show version information"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_shell_errors_with_go_wording() {
        let err = run_for_shell("tcsh").unwrap_err();
        assert_eq!(err, "unsupported shell: tcsh");
    }

    #[test]
    fn known_shells_succeed() {
        for sh in ["bash", "zsh", "fish", "powershell"] {
            assert!(run_for_shell(sh).is_ok(), "shell {sh} should generate");
        }
    }
}
