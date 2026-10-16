//! Shared helper for the CLI-level end-to-end tests: a process command that is
//! isolated from the developer's environment.
//!
//! `codefang run` reads `./.codefang.yaml`, then `$HOME/.codefang.yaml`, and a
//! `CODEFANG_<KEY>` environment variable per configuration key, as the default
//! layer of its flags. A test that inherited a developer's config file or
//! variables would assert against defaults it did not choose, so every e2e
//! suite builds its commands through [`command`].

use std::process::Command;

/// The test clock variable: not a configuration key, kept when inherited.
const TEST_CLOCK_VAR: &str = "CODEFANG_NOW";

/// Builds a command for `program` (the `codefang` binary, or a wrapper such as
/// `taskset` that runs it) with `HOME` pointed at the test scratch directory
/// (which holds no `.codefang.yaml`) and every inherited `CODEFANG_*`
/// configuration variable removed.
pub fn command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.env("HOME", env!("CARGO_TARGET_TMPDIR"));
    for (key, _) in std::env::vars_os() {
        if let Some(name) = key.to_str() {
            if name.starts_with("CODEFANG_") && name != TEST_CLOCK_VAR {
                cmd.env_remove(name);
            }
        }
    }
    cmd
}
