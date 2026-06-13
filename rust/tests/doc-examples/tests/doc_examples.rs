//! Markdown-example test harness — makes the prose docs RUNNABLE.
//!
//! Scans `README.md` and `docs/**/*.md` (relative to the repo root) for fenced
//! ```` ```console ````, ```` ```bash ````, and ```` ```sh ```` blocks, extracts
//! every line that invokes `codefang` or `uast`, rewrites placeholder paths to
//! the hercules fixture, runs each command against the freshly built Rust binary,
//! and asserts exit code 0. Where the doc block shows expected stdout right after
//! the command, the first non-empty output line is checked for a loose match.
//!
//! # What counts as a command
//!
//! Inside a shell fence, a line is a command if it starts with `$ ` (prompt
//! style) or, in a `bash`/`sh` fence, is a bare `codefang …` / `uast …` line.
//! Lines that are not `codefang`/`uast` invocations are ignored (they are output
//! samples or unrelated shell). Only the subcommands the harness knows how to run
//! safely are executed: `version`, `run`, `parse`. Anything else (e.g. `docker`,
//! `git clone`, `cargo`) is skipped so the docs can still show install steps.
//!
//! # Placeholder rewriting
//!
//! - `codefang` / `uast` (argv[0]) → the built binary path.
//! - A path-shaped argument (`/path/to/repo`, `.`, `repo`) → the fixture repo.
//! - `uast parse <file.ext>` → a temp source file with that extension.
//!
//! # Speed and side-effects
//!
//! History `run` commands get `--checkpoint=false --resume=false --no-cache
//! --workers 1` appended when absent (no behavioural change to the documented
//! flags — they only disable on-disk caches and pin determinism) and a
//! `--limit`/`--head` cap is enforced so a single doc command stays fast.
//!
//! # Fixture
//!
//! The fixture repo is `$CODEFANG_DOC_FIXTURE` or, by default,
//! `/home/dmitriy/sources/hercules`. If it is absent the repo-walking commands
//! are skipped (not failed) so the harness stays portable; `version` and `parse`
//! always run.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A single extracted command plus any expected-output hint and its source.
struct DocCommand {
    source: String,     // "README.md:42"
    raw: String,        // the command text as written in the doc
    tokens: Vec<String>,// shell-split tokens
    expect: Vec<String>,// expected stdout lines shown right after the command (may be empty)
}

/// Repo root = two levels up from this crate's manifest dir
/// (rust/tests/doc-examples -> rust/tests -> rust -> <repo root>).
fn repo_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // tests/
    p.pop(); // rust/
    p.pop(); // <repo root>
    p
}

/// Locate a built binary in the target directory (test exe is in
/// target/<profile>/deps/).
fn binary_path(name: &str) -> Option<PathBuf> {
    let test_exe = std::env::current_exe().ok()?;
    let deps_dir = test_exe.parent()?; // target/<profile>/deps
    let profile_dir = deps_dir.parent()?; // target/<profile>
    let exe = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    let mut candidates = vec![profile_dir.join(&exe), deps_dir.join(&exe)];
    if let Some(target_dir) = profile_dir.parent() {
        candidates.push(target_dir.join("release").join(&exe));
        candidates.push(target_dir.join("debug").join(&exe));
    }
    candidates.into_iter().find(|c| c.exists())
}

/// Build `codefang`/`uast` with cargo if they are not already in the target
/// dir. The dev-dep on the bin crates is `ignored` by cargo (no lib target), so
/// `cargo test -p doc-examples` does not always build them first; this guarantees
/// the harness has binaries to run against.
fn ensure_built(name: &str) -> Option<PathBuf> {
    if let Some(p) = binary_path(name) {
        return Some(p);
    }
    let profile = if cfg!(debug_assertions) { "dev" } else { "release" };
    let status = Command::new(env!("CARGO"))
        .args(["build", "-p", name, "--profile", profile])
        .current_dir(repo_root().join("rust"))
        .status();
    if !matches!(status, Ok(s) if s.success()) {
        return None;
    }
    binary_path(name)
}

fn fixture_repo() -> Option<PathBuf> {
    let p = std::env::var("CODEFANG_DOC_FIXTURE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/home/dmitriy/sources/hercules"));
    if p.join(".git").exists() {
        Some(p)
    } else {
        None
    }
}

/// Collect README.md + docs/**/*.md. Optionally also the MkDocs sources under
/// site/**/*.md when `CODEFANG_DOC_SCAN_SITE=1` is set — the site docs use many
/// shell-continuation/placeholder forms that are not single-line runnable, so
/// they are opt-in rather than gated by default.
fn doc_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let readme = root.join("README.md");
    if readme.exists() {
        out.push(readme);
    }
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().and_then(|s| s.to_str()) == Some("md") {
                out.push(p);
            }
        }
    }
    walk(&root.join("docs"), &mut out);
    if std::env::var("CODEFANG_DOC_SCAN_SITE").as_deref() == Ok("1") {
        walk(&root.join("site"), &mut out);
    }
    out.sort();
    out
}

/// Naive POSIX-ish tokenizer: splits on whitespace, honoring single/double
/// quotes. Sufficient for the simple invocations the docs contain.
fn shell_split(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for c in s.chars() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                } else {
                    cur.push(c);
                }
            }
            None => match c {
                '\'' | '"' => {
                    quote = Some(c);
                    started = true;
                }
                c if c.is_whitespace() => {
                    if started {
                        out.push(std::mem::take(&mut cur));
                        started = false;
                    }
                }
                c => {
                    cur.push(c);
                    started = true;
                }
            },
        }
    }
    if started {
        out.push(cur);
    }
    out
}

/// Extract codefang/uast commands from one markdown file.
fn extract(path: &Path, rel: &str) -> Vec<DocCommand> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    let mut cmds = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        let fence_lang = trimmed
            .strip_prefix("```")
            .map(|rest| rest.trim().to_string());
        let is_shell_fence = matches!(
            fence_lang.as_deref(),
            Some("console") | Some("bash") | Some("sh") | Some("shell")
        );
        if !is_shell_fence {
            i += 1;
            continue;
        }
        // Inside a shell fence: collect until closing ```.
        let fence_start = i;
        let mut block: Vec<(usize, &str)> = Vec::new();
        i += 1;
        while i < lines.len() && !lines[i].trim_start().starts_with("```") {
            block.push((i, lines[i]));
            i += 1;
        }
        i += 1; // skip closing fence
        let _ = fence_start;
        parse_block(&block, rel, &mut cmds);
    }
    cmds
}

fn parse_block(block: &[(usize, &str)], rel: &str, cmds: &mut Vec<DocCommand>) {
    let mut j = 0;
    while j < block.len() {
        let (lineno, raw_line) = block[j];
        let line = raw_line.trim_end();
        let body = line.trim_start();
        // Prompt-style or bare invocation.
        let cmd_text = if let Some(rest) = body.strip_prefix("$ ") {
            Some(rest.to_string())
        } else if body.starts_with("codefang ")
            || body == "codefang"
            || body.starts_with("uast ")
            || body == "uast"
        {
            Some(body.to_string())
        } else {
            None
        };
        if let Some(cmd_text) = cmd_text {
            let tokens = shell_split(&cmd_text);
            if matches!(tokens.first().map(String::as_str), Some("codefang") | Some("uast")) {
                // Gather expected output: subsequent non-command, non-empty lines
                // until the next prompt/command or blank gap.
                let mut expect = Vec::new();
                let mut k = j + 1;
                while k < block.len() {
                    let next = block[k].1.trim_end();
                    let nb = next.trim_start();
                    if nb.is_empty() || nb.starts_with("$ ")
                        || nb.starts_with("codefang ")
                        || nb.starts_with("uast ")
                    {
                        break;
                    }
                    expect.push(next.to_string());
                    k += 1;
                }
                cmds.push(DocCommand {
                    source: format!("{rel}:{}", lineno + 1),
                    raw: cmd_text,
                    tokens,
                    expect,
                });
            }
        }
        j += 1;
    }
}

/// Decide if/how to run this command. Returns the rewritten argv (without
/// argv[0], which is replaced by the binary path) and a "skip reason" if it
/// should be skipped.
enum Plan {
    Run { bin: &'static str, args: Vec<String>, tmp_file: Option<PathBuf> },
    Skip(String),
}

fn plan(cmd: &DocCommand, fixture: Option<&Path>) -> Plan {
    // Shell features the harness does not emulate: pipes, redirection, env
    // assignments, and command chaining. Skip rather than mis-run them.
    if cmd.raw.contains('|')
        || cmd.raw.contains('>')
        || cmd.raw.contains("&&")
        || cmd.raw.contains(';')
        || cmd.raw.contains('`')
        || cmd.raw.contains("$(")
        || cmd.raw.contains('\\') // line continuation
        || cmd.raw.contains('$') // env var reference
    {
        return Plan::Skip("shell pipe/redirect/chain/continuation not emulated".into());
    }
    // Placeholder / synopsis syntax (`[path]`, `{}`, `~/…`, `<repo>`) cannot be
    // run literally.
    if cmd.tokens.iter().any(|t| {
        t.starts_with('[')
            || t.starts_with('{')
            || t.starts_with('~')
            || (t.starts_with('<') && t.ends_with('>') && !is_path_placeholder(t))
    }) {
        return Plan::Skip("synopsis/placeholder token not runnable".into());
    }
    // Inline `# comment` trailing the command (kept by the tokenizer) means this
    // is an annotated sample, not a clean invocation.
    if cmd.tokens.iter().any(|t| t == "#") {
        return Plan::Skip("inline comment in command".into());
    }
    // Glob-bearing path args (e.g. `**/*.go`, `*.go`) are shell-expanded by a
    // real shell; we do not expand them.
    if cmd.tokens.iter().any(|t| t.contains('*') && !t.starts_with("-a") && !t.contains('/')) {
        // Allow analyzer globs like `static/*`; only skip bare file globs.
        if cmd.tokens.iter().any(|t| {
            t.contains('*') && !t.contains("static/") && !t.contains("history/") && *t != "*"
        }) {
            return Plan::Skip("file glob not expanded".into());
        }
    }

    let bin = match cmd.tokens[0].as_str() {
        "uast" => "uast",
        _ => "codefang",
    };
    let sub = cmd.tokens.get(1).map(String::as_str).unwrap_or("");
    let rest = &cmd.tokens[1..];

    // Plot output requires an -o output dir the docs may not supply; skip.
    if bin == "codefang" && sub == "run" {
        let has_plot = rest.iter().any(|t| t == "plot");
        let has_out = rest.iter().any(|t| t == "-o" || t == "--output");
        if has_plot && !has_out {
            return Plan::Skip("--format plot needs -o output dir".into());
        }
    }

    match (bin, sub) {
        ("codefang", "version") | ("uast", "version") => {
            Plan::Run { bin, args: rest.to_vec(), tmp_file: None }
        }
        ("codefang", "run") => plan_run(rest, fixture),
        ("uast", "parse") => plan_parse(&rest[1..]),
        _ => Plan::Skip(format!("unsupported subcommand `{bin} {sub}`")),
    }
}

/// `codefang run …`: rewrite placeholder repo paths to the fixture, append
/// safety/speed flags when absent.
fn plan_run(rest: &[String], fixture: Option<&Path>) -> Plan {
    let Some(fixture) = fixture else {
        return Plan::Skip("fixture repo unavailable".into());
    };
    let is_list = rest.iter().any(|t| t == "--list-analyzers");

    let mut args: Vec<String> = Vec::new();
    let mut replaced_path = false;
    for t in rest {
        if is_path_placeholder(t) {
            args.push(fixture.display().to_string());
            replaced_path = true;
        } else {
            args.push(t.clone());
        }
    }
    if is_list {
        // No repo needed.
        return Plan::Run { bin: "codefang", args, tmp_file: None };
    }
    if !replaced_path {
        // No explicit repo arg — point at the fixture.
        args.push(fixture.display().to_string());
    }
    // Determinism + no on-disk cache side-effects.
    ensure_flag(&mut args, "--checkpoint=false");
    ensure_flag(&mut args, "--resume=false");
    ensure_flag(&mut args, "--no-cache");
    if !has_flag(&args, "--workers") {
        args.push("--workers".into());
        args.push("1".into());
    }
    // Speed cap: keep one doc command fast.
    let is_static = args.iter().any(|a| a.contains("static/"));
    if is_static {
        ensure_flag(&mut args, "--head");
        if !has_flag(&args, "--static-workers") {
            args.push("--static-workers".into());
            args.push("1".into());
        }
    } else if !has_flag(&args, "--limit") && !has_flag(&args, "--head") {
        args.push("--limit".into());
        args.push("30".into());
    }
    Plan::Run { bin: "codefang", args, tmp_file: None }
}

/// `uast parse <file>`: materialize a temp source file matching the extension.
/// `args_after_parse` are the tokens after the `parse` subcommand.
fn plan_parse(args_after_parse: &[String]) -> Plan {
    let rest = args_after_parse;
    // stdin (`-`), whole-tree (`--all`), or a forced language (`-l`/--language)
    // are not safely reproducible with a synthetic temp file; skip them.
    if rest.iter().any(|t| {
        t == "-" || t == "--all" || t == "-l" || t == "--language" || t == "--lang"
    }) {
        return Plan::Skip("uast parse stdin/--all/forced-language not emulated".into());
    }
    // Output to a file (`-o`) is fine to skip — it writes nowhere useful here.
    if rest.iter().any(|t| t == "-o" || t == "--output") {
        return Plan::Skip("uast parse -o writes a file; skipped".into());
    }
    // Find the file-shaped argument (first non-flag token).
    let file_arg = rest.iter().find(|t| !t.starts_with('-'));
    let Some(file_arg) = file_arg else {
        return Plan::Skip("no file argument to `uast parse`".into());
    };
    let ext = Path::new(file_arg)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("rs");
    let (content, ext) = sample_source(ext);
    let dir = std::env::temp_dir();
    let tmp = dir.join(format!("codefang_doc_example_{}.{ext}", std::process::id()));
    if fs::write(&tmp, content).is_err() {
        return Plan::Skip("could not write temp source".into());
    }
    let mut args: Vec<String> = vec!["parse".to_string()];
    for t in rest {
        if t.as_str() == file_arg.as_str() {
            args.push(tmp.display().to_string());
        } else {
            args.push(t.clone());
        }
    }
    Plan::Run { bin: "uast", args, tmp_file: Some(tmp) }
}

fn sample_source(ext: &str) -> (&'static str, &'static str) {
    match ext {
        "go" => ("package main\n\nfunc main() {}\n", "go"),
        "py" => ("def main():\n    return 0\n", "py"),
        "js" => ("function main() { return 0; }\n", "js"),
        _ => ("fn main() {}\n", "rs"),
    }
}

fn is_path_placeholder(t: &str) -> bool {
    matches!(
        t,
        "/path/to/repo" | "/repo" | "." | "repo" | "<repo>" | "./" | "/path/to/repository"
    )
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag || a.starts_with(&format!("{flag}=")))
}

fn ensure_flag(args: &mut Vec<String>, flag: &str) {
    // flag is like "--no-cache" or "--checkpoint=false".
    let name = flag.split('=').next().unwrap_or(flag);
    if !has_flag(args, name) {
        args.push(flag.to_string());
    }
}

/// Loose output match: the first non-empty expected line should appear (as a
/// substring, case-insensitively) somewhere in stdout. Output samples in docs
/// are illustrative, so we only check structural prefixes like the binary name.
fn output_matches(expect: &[String], stdout: &str) -> bool {
    let Some(first) = expect.iter().map(|s| s.trim()).find(|s| !s.is_empty()) else {
        return true;
    };
    // Only enforce a match for `version`-style lines that name the binary.
    if first.starts_with("codefang ") || first.starts_with("uast ") {
        let prefix: String = first.split_whitespace().take(1).collect();
        return stdout.to_lowercase().contains(&prefix.to_lowercase());
    }
    true
}

#[test]
fn markdown_examples_run() {
    let root = repo_root();
    let fixture = fixture_repo();
    let codefang = ensure_built("codefang").expect("codefang binary should build");
    let uast = ensure_built("uast").expect("uast binary should build");

    let mut files = doc_files(&root);
    assert!(!files.is_empty(), "no markdown docs found under {}", root.display());
    files.sort();

    let mut all: Vec<DocCommand> = Vec::new();
    for f in &files {
        let rel = f.strip_prefix(&root).unwrap_or(f).display().to_string();
        all.extend(extract(f, &rel));
    }
    assert!(
        !all.is_empty(),
        "no codefang/uast commands extracted from docs — extraction likely broke"
    );

    let mut ran = 0usize;
    let mut skipped: BTreeMap<String, usize> = BTreeMap::new();
    let mut failures: Vec<String> = Vec::new();

    for cmd in &all {
        match plan(cmd, fixture.as_deref()) {
            Plan::Skip(reason) => {
                *skipped.entry(reason).or_default() += 1;
            }
            Plan::Run { bin, args, tmp_file } => {
                let exe = if bin == "uast" { &uast } else { &codefang };
                let output = Command::new(exe)
                    .args(&args)
                    .env("TZ", "UTC")
                    .env("NO_COLOR", "1")
                    .env("LANG", "C")
                    .env("LC_ALL", "C")
                    .output();
                if let Some(tmp) = tmp_file {
                    let _ = fs::remove_file(tmp);
                }
                ran += 1;
                match output {
                    Ok(out) if out.status.success() => {
                        let stdout = String::from_utf8_lossy(&out.stdout);
                        if !output_matches(&cmd.expect, &stdout) {
                            failures.push(format!(
                                "[{}] output mismatch for `{}`\n  expected to contain: {:?}\n  got first 120 bytes: {:?}",
                                cmd.source,
                                cmd.raw,
                                cmd.expect.first(),
                                &stdout.chars().take(120).collect::<String>(),
                            ));
                        }
                    }
                    Ok(out) => {
                        failures.push(format!(
                            "[{}] non-zero exit ({:?}) for `{}` -> {} {}\n  stderr: {}",
                            cmd.source,
                            out.status.code(),
                            cmd.raw,
                            exe.display(),
                            args.join(" "),
                            String::from_utf8_lossy(&out.stderr)
                                .lines()
                                .take(5)
                                .collect::<Vec<_>>()
                                .join(" | "),
                        ));
                    }
                    Err(e) => {
                        failures.push(format!("[{}] spawn failed for `{}`: {e}", cmd.source, cmd.raw));
                    }
                }
            }
        }
    }

    eprintln!("doc-examples: extracted {} command(s), ran {ran}", all.len());
    for (reason, n) in &skipped {
        eprintln!("doc-examples: skipped {n} ({reason})");
    }

    assert!(
        ran > 0,
        "no doc commands were actually executed (fixture missing? set CODEFANG_DOC_FIXTURE)"
    );
    assert!(failures.is_empty(), "doc example(s) failed:\n{}", failures.join("\n"));
}
