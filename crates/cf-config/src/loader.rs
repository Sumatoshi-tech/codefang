//! Locating, reading and layering the configuration.
//!
//! Layers, lowest first: the compiled defaults (every field `None`), the
//! config file, the `CODEFANG_*` environment. The caller applies the result
//! as the default of each flag, so an explicit flag wins over all of them.

use crate::types::{Config, ConfigError, KEYS};
use serde_yaml::{Mapping, Value as Yaml};
use std::path::{Path, PathBuf};

/// The configuration file name searched in the working directory and in
/// `$HOME`.
pub const CONFIG_FILE_NAME: &str = ".codefang.yaml";

/// Prefix of the environment variable of a key: `CODEFANG_` + the key
/// upper-cased with `.` replaced by `_`.
pub const ENV_PREFIX: &str = "CODEFANG_";

/// Why the configuration could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// The file could not be read.
    #[error("cannot read config {}: {cause}", .path.display())]
    Read {
        /// The file.
        path: PathBuf,
        /// The I/O error.
        cause: std::io::Error,
    },
    /// The file is not a YAML mapping of the documented types.
    #[error("invalid config {}: {cause}", .path.display())]
    Parse {
        /// The file.
        path: PathBuf,
        /// The parser's message (it names the key and the line).
        cause: String,
    },
    /// A `CODEFANG_*` variable does not hold a value of its key's type.
    #[error("invalid environment variable {name}: {cause}")]
    Env {
        /// The variable name.
        name: String,
        /// The parser's message.
        cause: String,
    },
    /// A value breaks its key's rule; `origin` is the file path or the
    /// environment variable that set it.
    #[error("invalid config {origin}: {error}")]
    Invalid {
        /// The file path or variable name.
        origin: String,
        /// The violated rule.
        error: ConfigError,
    },
}

/// A loaded configuration.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Loaded {
    /// The file the configuration came from (`None`: no file).
    pub path: Option<PathBuf>,
    /// The layered configuration.
    pub config: Config,
    /// Keys the file sets that nothing reads, in document order.
    pub unsupported_keys: Vec<String>,
}

/// The default search directories: the working directory, then `$HOME`.
#[must_use]
pub fn default_search_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from(".")];
    if let Some(home) = std::env::var_os("HOME").filter(|h| !h.is_empty()) {
        dirs.push(PathBuf::from(home));
    }
    dirs
}

/// Loads the configuration.
///
/// `explicit` (from `--config`) must be readable; without it the first
/// [`CONFIG_FILE_NAME`] found in `search_dirs` is read, and none at all is
/// not an error. `env` resolves an environment variable name; the variable
/// of a key overrides the file.
///
/// # Errors
///
/// [`LoadError`] naming the file, variable or key at fault.
pub fn load<F>(
    explicit: Option<&Path>,
    search_dirs: &[PathBuf],
    env: F,
) -> Result<Loaded, LoadError>
where
    F: Fn(&str) -> Option<String>,
{
    let (path, contents) = match explicit {
        Some(p) => (Some(p.to_path_buf()), Some(read(p)?)),
        None => match discover(search_dirs)? {
            Some((p, c)) => (Some(p), Some(c)),
            None => (None, None),
        },
    };
    let file_tree = match (&path, contents) {
        (Some(p), Some(c)) => parse_file(p, &c)?,
        _ => Yaml::Null,
    };
    let mut unsupported_keys = Vec::new();
    collect_unsupported(&file_tree, "", &mut unsupported_keys);

    let mut merged = file_tree;
    for key in KEYS {
        let name = env_name(key);
        if let Some(raw) = env(&name) {
            let leaf = env_value(key, &raw);
            let only = nest(key, leaf.clone());
            let cfg: Config = serde_yaml::from_value(only).map_err(|e| LoadError::Env {
                name: name.clone(),
                cause: e.to_string(),
            })?;
            cfg.validate().map_err(|error| LoadError::Invalid {
                origin: name.clone(),
                error,
            })?;
            overlay(&mut merged, key, leaf);
        }
    }
    let config: Config = serde_yaml::from_value(merged).map_err(|e| LoadError::Parse {
        path: path.clone().unwrap_or_default(),
        cause: e.to_string(),
    })?;
    Ok(Loaded {
        path,
        config,
        unsupported_keys,
    })
}

/// The environment variable of `key`.
#[must_use]
pub fn env_name(key: &str) -> String {
    format!("{ENV_PREFIX}{}", key.to_uppercase().replace('.', "_"))
}

/// Reads `path` whole.
fn read(path: &Path) -> Result<String, LoadError> {
    std::fs::read_to_string(path).map_err(|cause| LoadError::Read {
        path: path.to_path_buf(),
        cause,
    })
}

/// The first [`CONFIG_FILE_NAME`] in `dirs` and its contents. A missing file
/// moves on to the next directory; any other read failure is an error.
fn discover(dirs: &[PathBuf]) -> Result<Option<(PathBuf, String)>, LoadError> {
    for dir in dirs {
        let candidate = dir.join(CONFIG_FILE_NAME);
        match std::fs::read_to_string(&candidate) {
            Ok(contents) => return Ok(Some((candidate, contents))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(cause) => {
                return Err(LoadError::Read {
                    path: candidate,
                    cause,
                })
            }
        }
    }
    Ok(None)
}

/// Parses the file into a YAML tree, checks the documented types and the
/// value rules. A file holding only comments is the empty configuration.
fn parse_file(path: &Path, contents: &str) -> Result<Yaml, LoadError> {
    let parse_error = |cause: String| LoadError::Parse {
        path: path.to_path_buf(),
        cause,
    };
    let tree: Yaml = serde_yaml::from_str(contents).map_err(|e| parse_error(e.to_string()))?;
    if !matches!(tree, Yaml::Null | Yaml::Mapping(_)) {
        return Err(parse_error("the document must be a mapping of keys".into()));
    }
    if tree.is_null() {
        return Ok(tree);
    }
    // Typed parse of the TEXT (not the tree) so the message names the key
    // path and the line: `history.anomaly.window_size: invalid type: …`.
    let cfg: Config = serde_yaml::from_str(contents).map_err(|e| parse_error(e.to_string()))?;
    cfg.validate().map_err(|error| LoadError::Invalid {
        origin: path.display().to_string(),
        error,
    })?;
    Ok(tree)
}

/// Appends to `out` every leaf path under `node` (prefixed with `prefix`)
/// that is not a supported key, descending into mappings.
fn collect_unsupported(node: &Yaml, prefix: &str, out: &mut Vec<String>) {
    let Yaml::Mapping(map) = node else {
        // `pipeline:` with every child commented out is an empty section.
        let empty_section = node.is_null() && is_section(prefix);
        if !prefix.is_empty() && !empty_section {
            out.push(prefix.to_owned());
        }
        return;
    };
    if map.is_empty() && !prefix.is_empty() && !is_section(prefix) {
        out.push(prefix.to_owned());
    }
    for (k, v) in map {
        let name = match k {
            Yaml::String(s) => s.clone(),
            other => serde_yaml::to_string(other)
                .map(|s| s.trim().to_owned())
                .unwrap_or_default(),
        };
        let path = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}.{name}")
        };
        if KEYS.contains(&path.as_str()) {
            continue;
        }
        collect_unsupported(v, &path, out);
    }
}

/// True when `prefix` is a section holding supported keys (`history`,
/// `history.anomaly`, …).
fn is_section(prefix: &str) -> bool {
    KEYS.iter()
        .any(|k| k.strip_prefix(prefix).is_some_and(|r| r.starts_with('.')))
}

/// The YAML value of an environment variable: a comma / whitespace
/// separated list for `analyzers`, the raw text for the string keys, a YAML
/// scalar otherwise (`3`, `true`, `2.5`).
fn env_value(key: &str, raw: &str) -> Yaml {
    use crate::types::keys;
    if key == keys::SHOTNESS_DSL_STRUCT
        || key == keys::SHOTNESS_DSL_NAME
        || key == keys::MEMORY_BUDGET
    {
        return Yaml::String(raw.to_owned());
    }
    if key == keys::ANALYZERS {
        let items = raw
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .map(|s| Yaml::String(s.to_owned()))
            .collect();
        return Yaml::Sequence(items);
    }
    serde_yaml::from_str(raw).unwrap_or_else(|_| Yaml::String(raw.to_owned()))
}

/// `leaf` nested under the dotted `key` (`a.b` → `{a: {b: leaf}}`).
fn nest(key: &str, leaf: Yaml) -> Yaml {
    key.rsplit('.').fold(leaf, |inner, part| {
        let mut map = Mapping::new();
        map.insert(Yaml::String(part.to_owned()), inner);
        Yaml::Mapping(map)
    })
}

/// Sets the dotted `key` of `tree` to `leaf`, creating (or replacing
/// non-mapping) intermediate sections.
fn overlay(tree: &mut Yaml, key: &str, leaf: Yaml) {
    let mut node = tree;
    let parts: Vec<&str> = key.split('.').collect();
    for (i, part) in parts.iter().enumerate() {
        if !matches!(node, Yaml::Mapping(_)) {
            *node = Yaml::Mapping(Mapping::new());
        }
        let Yaml::Mapping(map) = node else {
            unreachable!("just made a mapping")
        };
        let k = Yaml::String((*part).to_owned());
        if i + 1 == parts.len() {
            map.insert(k, leaf);
            return;
        }
        node = map.entry(k).or_insert(Yaml::Null);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::keys;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn env_names_follow_the_key() {
        assert_eq!(
            env_name(keys::ANOMALY_WINDOW_SIZE),
            "CODEFANG_HISTORY_ANOMALY_WINDOW_SIZE"
        );
        assert_eq!(env_name(keys::ANALYZERS), "CODEFANG_ANALYZERS");
    }

    /// The budget variable holds a size, kept as text whatever it looks
    /// like.
    #[test]
    fn the_memory_budget_variable_is_text() {
        for raw in ["512MB", "1073741824"] {
            let dir = tempfile::tempdir().unwrap();
            let env =
                |name: &str| (name == "CODEFANG_PIPELINE_MEMORY_BUDGET").then(|| raw.to_owned());
            let loaded = load(None, &[dir.path().to_path_buf()], env).unwrap();
            assert_eq!(loaded.config.pipeline.memory_budget.as_deref(), Some(raw));
        }
    }

    #[test]
    fn no_file_and_no_env_is_the_empty_configuration() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = load(None, &[dir.path().to_path_buf()], no_env).unwrap();
        assert_eq!(loaded, Loaded::default());
    }

    #[test]
    fn discovery_takes_the_first_directory_that_has_the_file() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        write(
            second.path(),
            CONFIG_FILE_NAME,
            "history:\n  tick_size: 2\n",
        );
        let dirs = [first.path().to_path_buf(), second.path().to_path_buf()];
        let loaded = load(None, &dirs, no_env).unwrap();
        assert_eq!(loaded.path, Some(second.path().join(CONFIG_FILE_NAME)));
        assert_eq!(loaded.config.history.tick_size, Some(2));

        write(first.path(), CONFIG_FILE_NAME, "history:\n  tick_size: 1\n");
        let loaded = load(None, &dirs, no_env).unwrap();
        assert_eq!(loaded.config.history.tick_size, Some(1));
    }

    #[test]
    fn an_explicit_path_skips_discovery_and_must_exist() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), CONFIG_FILE_NAME, "history:\n  tick_size: 1\n");
        let other = write(dir.path(), "other.yaml", "history:\n  tick_size: 6\n");
        let dirs = [dir.path().to_path_buf()];
        let loaded = load(Some(&other), &dirs, no_env).unwrap();
        assert_eq!(loaded.config.history.tick_size, Some(6));

        let missing = dir.path().join("missing.yaml");
        let err = load(Some(&missing), &dirs, no_env).unwrap_err();
        assert!(matches!(err, LoadError::Read { .. }), "{err}");
        assert!(err.to_string().starts_with("cannot read config "), "{err}");
    }

    #[test]
    fn a_comment_only_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(dir.path(), "c.yaml", "# nothing\n# analyzers: []\n");
        let loaded = load(Some(&p), &[], no_env).unwrap();
        assert_eq!(loaded.config, Config::default());
        assert!(loaded.unsupported_keys.is_empty());
    }

    #[test]
    fn unsupported_keys_are_listed_in_document_order() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(
            dir.path(),
            "c.yaml",
            "pipeline:\n  workers: 4\n  max_changes_per_commit: 5\n\
             checkpoint:\n  enabled: true\n  dir: x\n\
             history:\n  burndown:\n    granularity: 7\n    debug: true\n  clones: {}\n\
             bogus: 1\n",
        );
        let loaded = load(Some(&p), &[], no_env).unwrap();
        assert_eq!(
            loaded.unsupported_keys,
            vec![
                "pipeline.workers",
                "checkpoint.enabled",
                "checkpoint.dir",
                "history.burndown.debug",
                "history.clones",
                "bogus",
            ]
        );
        assert_eq!(loaded.config.pipeline.max_changes_per_commit, Some(5));
        assert_eq!(loaded.config.history.burndown.granularity, Some(7));
    }

    #[test]
    fn a_section_with_every_child_commented_out_is_not_unsupported() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(
            dir.path(),
            "c.yaml",
            "pipeline:\n#  workers: 0\nhistory:\n  burndown:\ncheckpoint:\n",
        );
        let loaded = load(Some(&p), &[], no_env).unwrap();
        assert_eq!(loaded.unsupported_keys, vec!["checkpoint"]);
    }

    #[test]
    fn a_wrong_type_is_a_parse_error_naming_the_key() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(
            dir.path(),
            "c.yaml",
            "history:\n  anomaly:\n    window_size: many\n",
        );
        let err = load(Some(&p), &[], no_env).unwrap_err().to_string();
        assert!(err.starts_with("invalid config "), "{err}");
        assert!(err.contains("history.anomaly.window_size"), "{err}");
    }

    #[test]
    fn a_non_mapping_document_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(dir.path(), "c.yaml", "- a\n- b\n");
        let err = load(Some(&p), &[], no_env).unwrap_err().to_string();
        assert!(err.contains("must be a mapping"), "{err}");
    }

    #[test]
    fn a_rule_violation_names_the_file_and_the_key() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(
            dir.path(),
            "c.yaml",
            "history:\n  burndown:\n    sampling: 0\n",
        );
        let err = load(Some(&p), &[], no_env).unwrap_err().to_string();
        assert_eq!(
            err,
            format!(
                "invalid config {}: history.burndown.sampling must be > 0, got 0",
                p.display()
            )
        );
    }

    #[test]
    fn the_environment_overrides_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(
            dir.path(),
            "c.yaml",
            "history:\n  anomaly:\n    window_size: 20\n    threshold: 3\n",
        );
        let env = |name: &str| match name {
            "CODEFANG_HISTORY_ANOMALY_WINDOW_SIZE" => Some("4".to_owned()),
            "CODEFANG_ANALYZERS" => Some("history/devs, history/anomaly".to_owned()),
            "CODEFANG_HISTORY_DEVS_ANONYMIZE" => Some("true".to_owned()),
            _ => None,
        };
        let loaded = load(Some(&p), &[], env).unwrap();
        let cfg = &loaded.config;
        assert_eq!(cfg.history.anomaly.window_size, Some(4));
        assert_eq!(cfg.history.anomaly.threshold, Some(3.0));
        assert_eq!(cfg.history.devs.anonymize, Some(true));
        assert_eq!(
            cfg.analyzers,
            Some(vec![
                "history/devs".to_owned(),
                "history/anomaly".to_owned()
            ])
        );
    }

    #[test]
    fn a_bad_environment_value_names_the_variable() {
        let env = |name: &str| (name == "CODEFANG_HISTORY_TICK_SIZE").then(|| "hourly".to_owned());
        let err = load(None, &[], env).unwrap_err().to_string();
        assert!(
            err.starts_with("invalid environment variable CODEFANG_HISTORY_TICK_SIZE: "),
            "{err}"
        );

        let env = |name: &str| (name == "CODEFANG_HISTORY_TICK_SIZE").then(|| "0".to_owned());
        let err = load(None, &[], env).unwrap_err().to_string();
        assert_eq!(
            err,
            "invalid config CODEFANG_HISTORY_TICK_SIZE: history.tick_size must be > 0, got 0"
        );
    }

    #[test]
    fn overlay_replaces_scalars_on_the_path() {
        let mut tree: Yaml = serde_yaml::from_str("history: 3\n").unwrap();
        overlay(&mut tree, keys::TICK_SIZE, Yaml::Number(5.into()));
        let cfg: Config = serde_yaml::from_value(tree).unwrap();
        assert_eq!(cfg.history.tick_size, Some(5));
    }
}
