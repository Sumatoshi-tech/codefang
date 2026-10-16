//! End-to-end coverage of the configuration file, driven through the **real
//! binary** (`specs/bugs/BUG-cli-inert-surface.md`, item 1).
//!
//! The contract (`site/guide/configuration.md`): `--config <file>` names the
//! file explicitly (it must be readable); without it `./.codefang.yaml` and
//! then `$HOME/.codefang.yaml` are discovered. Every supported key is the
//! default of one `run` flag, so the precedence is explicit flag >
//! `CODEFANG_*` environment > config file > compiled default. Unsupported
//! keys are ignored with ONE stderr warning naming them.
//!
//! Every run pins `HOME` and the working directory to empty temporary
//! directories, so a developer's own `~/.codefang.yaml` can never leak in.

mod common;

use std::path::{Path, PathBuf};
use std::process::Output;

use cf_gitlib::testutil::{commit_files, init_repo, TestRepo};

/// Path to the freshly built `codefang` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_codefang");
/// The discovered configuration file name.
const CONFIG_NAME: &str = ".codefang.yaml";
/// Base instant of the fixture history: 2021-01-01T12:00:00Z.
const T0: i64 = 1_609_502_400;
/// One day between quiet fixture commits.
const DAY: i64 = 86_400;
/// Quiet days before the busy day.
const QUIET_DAYS: i64 = 8;
/// New files written on the busy day.
const BURST_FILES: usize = 6;
/// A window shorter than the quiet history, so it changes every Z-score.
const SHORT_WINDOW: &str = "3";
/// The compiled `--anomaly-window` default.
const DEFAULT_WINDOW: &str = "20";
/// A config file that only shortens the anomaly window.
const SHORT_WINDOW_CONFIG: &str = "history:\n  anomaly:\n    window_size: 3\n";

/// One hermetic invocation: an empty `HOME`, an empty working directory, a
/// deterministic locale.
struct Sandbox {
    home: tempfile::TempDir,
    cwd: tempfile::TempDir,
}

impl Sandbox {
    fn new() -> Self {
        Self {
            home: tempfile::tempdir().expect("home tempdir"),
            cwd: tempfile::tempdir().expect("cwd tempdir"),
        }
    }

    /// Writes `body` as a config file named `name` under `dir`.
    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, body).expect("write config");
        path
    }

    /// Writes a config file in the sandbox's working directory under a
    /// non-discovered name, for `--config`.
    fn explicit(&self, body: &str) -> String {
        Self::write(self.cwd.path(), "explicit.yaml", body)
            .to_str()
            .expect("utf-8 path")
            .to_owned()
    }

    /// Runs `codefang run <args>` with `envs` in the sandbox.
    fn run(&self, args: &[&str], envs: &[(&str, &str)]) -> Output {
        let mut cmd = common::command(BIN);
        cmd.arg("run")
            .args(args)
            .current_dir(self.cwd.path())
            .env("HOME", self.home.path())
            .env("TZ", "UTC")
            .env("NO_COLOR", "1")
            .env("LANG", "C")
            .env("LC_ALL", "C");
        for (k, v) in envs {
            cmd.env(k, v);
        }
        cmd.output().expect("run the codefang binary")
    }

    /// stdout of a successful run.
    fn stdout_ok(&self, args: &[&str], envs: &[(&str, &str)]) -> String {
        let out = self.run(args, envs);
        assert!(
            out.status.success(),
            "run failed ({args:?}): {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).expect("utf-8 stdout")
    }
}

/// `n` numbered lines tagged with `tag`.
fn lines(tag: &str, n: usize) -> Vec<u8> {
    (1..=n)
        .map(|i| format!("line {i} of {tag}\n"))
        .collect::<String>()
        .into_bytes()
}

/// Eight quiet days (one file each, alternating 10 / 12 lines) and one busy
/// day writing six new files: the busy day is flagged, and its Z-scores
/// depend on the anomaly window.
fn burst_fixture() -> TestRepo {
    let test = init_repo().expect("init");
    for d in 0..QUIET_DAYS {
        let name = format!("q{d}.txt");
        let n = if d % 2 == 0 { 10 } else { 12 };
        commit_files(
            &test,
            "quiet",
            T0 + d * DAY,
            &[(name.as_str(), lines(&name, n).as_slice())],
        )
        .expect("quiet commit");
    }
    for k in 0..BURST_FILES {
        let name = format!("burst{k}.txt");
        let when = T0 + QUIET_DAYS * DAY + i64::try_from(k).expect("small") * 60;
        commit_files(
            &test,
            "burst",
            when,
            &[(name.as_str(), lines(&name, 10).as_slice())],
        )
        .expect("burst commit");
    }
    test
}

fn path(test: &TestRepo) -> &str {
    test.path().to_str().expect("utf-8 path")
}

/// The anomaly report of `test` with the extra `args`, run with the compiled
/// defaults only (`--config /dev/null` disables discovery).
fn anomaly_with_flags(test: &TestRepo, args: &[&str]) -> String {
    let sandbox = Sandbox::new();
    let mut all = vec!["-a", "history/anomaly", "--config", "/dev/null"];
    all.extend_from_slice(args);
    all.push(path(test));
    sandbox.stdout_ok(&all, &[])
}

#[test]
fn config_window_size_changes_the_anomaly_report_like_the_flag() {
    let test = burst_fixture();
    let by_flag = anomaly_with_flags(&test, &["--anomaly-window", SHORT_WINDOW]);
    let by_default = anomaly_with_flags(&test, &[]);
    assert_ne!(by_flag, by_default, "the fixture must be window-sensitive");

    let sandbox = Sandbox::new();
    let config = sandbox.explicit(SHORT_WINDOW_CONFIG);
    let by_config = sandbox.stdout_ok(
        &["-a", "history/anomaly", "--config", &config, path(&test)],
        &[],
    );
    assert_eq!(by_config, by_flag);
}

#[test]
fn an_explicit_flag_beats_the_config_file() {
    let test = burst_fixture();
    let sandbox = Sandbox::new();
    let config = sandbox.explicit(SHORT_WINDOW_CONFIG);
    let out = sandbox.stdout_ok(
        &[
            "-a",
            "history/anomaly",
            "--config",
            &config,
            "--anomaly-window",
            DEFAULT_WINDOW,
            path(&test),
        ],
        &[],
    );
    assert_eq!(out, anomaly_with_flags(&test, &[]));
}

#[test]
fn the_environment_beats_the_config_file_and_the_flag_beats_both() {
    let test = burst_fixture();
    let sandbox = Sandbox::new();
    let config = sandbox.explicit("history:\n  anomaly:\n    window_size: 20\n");
    let env = [("CODEFANG_HISTORY_ANOMALY_WINDOW_SIZE", SHORT_WINDOW)];
    let args = ["-a", "history/anomaly", "--config", &config, path(&test)];
    assert_eq!(
        sandbox.stdout_ok(&args, &env),
        anomaly_with_flags(&test, &["--anomaly-window", SHORT_WINDOW])
    );

    let mut explicit = args.to_vec();
    explicit.extend_from_slice(&["--anomaly-window", DEFAULT_WINDOW]);
    assert_eq!(
        sandbox.stdout_ok(&explicit, &env),
        anomaly_with_flags(&test, &[])
    );
}

#[test]
fn the_config_is_discovered_in_the_working_directory() {
    let test = burst_fixture();
    let sandbox = Sandbox::new();
    Sandbox::write(sandbox.cwd.path(), CONFIG_NAME, SHORT_WINDOW_CONFIG);
    // A HOME config must lose to the working-directory one.
    Sandbox::write(
        sandbox.home.path(),
        CONFIG_NAME,
        "history:\n  anomaly:\n    window_size: 20\n",
    );
    let out = sandbox.stdout_ok(&["-a", "history/anomaly", path(&test)], &[]);
    assert_eq!(
        out,
        anomaly_with_flags(&test, &["--anomaly-window", SHORT_WINDOW])
    );
}

#[test]
fn the_config_is_discovered_in_home() {
    let test = burst_fixture();
    let sandbox = Sandbox::new();
    Sandbox::write(sandbox.home.path(), CONFIG_NAME, SHORT_WINDOW_CONFIG);
    let out = sandbox.stdout_ok(&["-a", "history/anomaly", path(&test)], &[]);
    assert_eq!(
        out,
        anomaly_with_flags(&test, &["--anomaly-window", SHORT_WINDOW])
    );
}

#[test]
fn config_analyzers_are_the_default_selection() {
    let test = burst_fixture();
    let sandbox = Sandbox::new();
    let config = sandbox.explicit("analyzers:\n  - history/devs\n");
    let by_config = sandbox.stdout_ok(&["--config", &config, path(&test)], &[]);
    let by_flag = sandbox.stdout_ok(
        &["--config", "/dev/null", "-a", "history/devs", path(&test)],
        &[],
    );
    assert_eq!(by_config, by_flag);

    // `-a` on the command line replaces the configured selection.
    let explicit = sandbox.stdout_ok(
        &["--config", &config, "-a", "history/anomaly", path(&test)],
        &[],
    );
    assert_eq!(explicit, anomaly_with_flags(&test, &[]));
}

#[test]
fn a_missing_explicit_config_is_an_error() {
    let test = burst_fixture();
    let sandbox = Sandbox::new();
    let missing = sandbox.cwd.path().join("missing.yaml");
    let missing = missing.to_str().expect("utf-8 path");
    let out = sandbox.run(
        &["-a", "history/anomaly", "--config", missing, path(&test)],
        &[],
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty(), "no report on a config error");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(&format!("Error: cannot read --config {missing}: ")),
        "{err}"
    );
}

#[test]
fn an_invalid_config_value_names_the_key() {
    let test = burst_fixture();
    let sandbox = Sandbox::new();
    let config = sandbox.explicit("history:\n  anomaly:\n    window_size: 1\n");
    let out = sandbox.run(
        &["-a", "history/anomaly", "--config", &config, path(&test)],
        &[],
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(&format!(
            "Error: invalid config {config}: history.anomaly.window_size must be >= 2, got 1"
        )),
        "{err}"
    );
}

#[test]
fn unsupported_keys_warn_once_and_the_rest_applies() {
    let test = burst_fixture();
    let sandbox = Sandbox::new();
    let config = sandbox.explicit(
        "pipeline:\n  workers: 8\n  max_changes_per_commit: 10000\n\
         checkpoint:\n  enabled: false\n\
         history:\n  burndown:\n    goroutines: 4\n  anomaly:\n    window_size: 3\n",
    );
    let out = sandbox.run(
        &["-a", "history/anomaly", "--config", &config, path(&test)],
        &[],
    );
    assert!(out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    let warnings: Vec<&str> = err
        .lines()
        .filter(|l| l.starts_with("warning: ignoring unsupported config keys"))
        .collect();
    assert_eq!(
        warnings,
        vec![
            "warning: ignoring unsupported config keys: pipeline.workers, \
             checkpoint.enabled, history.burndown.goroutines"
        ],
        "{err}"
    );
    assert_eq!(
        String::from_utf8(out.stdout).expect("utf-8"),
        anomaly_with_flags(&test, &["--anomaly-window", SHORT_WINDOW])
    );
}

#[test]
fn the_repository_config_file_loads() {
    let repo_config = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.codefang.yaml");
    let repo_config = repo_config.to_str().expect("utf-8 path");
    let test = burst_fixture();
    let sandbox = Sandbox::new();
    let out = sandbox.run(
        &[
            "-a",
            "history/anomaly",
            "--config",
            repo_config,
            path(&test),
        ],
        &[],
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    assert!(!err.contains("warning:"), "{err}");
}

#[test]
fn verbose_names_the_config_file() {
    let test = burst_fixture();
    let sandbox = Sandbox::new();
    let config = sandbox.explicit(SHORT_WINDOW_CONFIG);
    let out = sandbox.run(
        &[
            "-v",
            "-a",
            "history/anomaly",
            "--config",
            &config,
            path(&test),
        ],
        &[],
    );
    assert!(out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(&format!("progress: config file: {config}")),
        "{err}"
    );

    let out = sandbox.run(&["-v", "-a", "history/anomaly", path(&test)], &[]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("progress: config file: none"), "{err}");
}

/// `pipeline.chunk_size` and `pipeline.memory_budget` default their flags:
/// the configured chunk reaches the walk's plan (`-v`), the environment
/// beats the file, and an out-of-range value fails like the flag.
#[test]
fn the_pipeline_budget_keys_default_their_flags() {
    let test = burst_fixture();
    let sandbox = Sandbox::new();
    let p = path(&test);

    let config = sandbox.explicit("pipeline:\n  chunk_size: 3\n");
    let out = sandbox.run(&["-v", "-a", "history/devs", "--config", &config, p], &[]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    assert!(
        err.contains("progress: history/devs plan: 3 commits per chunk, "),
        "{err}"
    );

    // The plan line names the walk's share of the budget: the environment's
    // 1 GiB beats the file's 512 MiB exactly like the flag would.
    let config = sandbox.explicit("pipeline:\n  memory_budget: 512MiB\n");
    let share = |args: &[&str], envs: &[(&str, &str)]| {
        let out = sandbox.run(args, envs);
        let err = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(out.status.success(), "{err}");
        err.lines()
            .find_map(|l| {
                l.strip_suffix(" MiB)")?
                    .rsplit_once("(budget ")?
                    .1
                    .parse::<u64>()
                    .ok()
            })
            .unwrap_or_else(|| panic!("no plan line in {err}"))
    };
    let from_file = share(&["-v", "-a", "history/devs", "--config", &config, p], &[]);
    let from_env = share(
        &["-v", "-a", "history/devs", "--config", &config, p],
        &[("CODEFANG_PIPELINE_MEMORY_BUDGET", "1GiB")],
    );
    let from_flag = share(
        &["-v", "-a", "history/devs", "--memory-budget", "1GiB", p],
        &[],
    );
    assert_eq!(from_env, from_flag);
    assert!(from_env > from_file, "{from_env} <= {from_file}");

    let config = sandbox.explicit("pipeline:\n  memory_budget: 1MB\n");
    let out = sandbox.run(&["-a", "history/devs", "--config", &config, p], &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("Error: invalid --memory-budget: \"1MB\" is below the minimum 256 MiB"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let config = sandbox.explicit("pipeline:\n  chunk_size: 0\n");
    let out = sandbox.run(&["-a", "history/devs", "--config", &config, p], &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains(&format!(
            "Error: invalid config {config}: pipeline.chunk_size must be >= 1, got 0"
        )),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
