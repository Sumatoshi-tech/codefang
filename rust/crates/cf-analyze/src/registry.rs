//! Analyzer-descriptor registry with deterministic ordering and glob expansion.
//!
//! Port of `internal/analyzers/analyze/registry.go`. The [`Registry`] stores
//! [`Descriptor`]s in registration order plus a name index, splits IDs by mode,
//! and expands glob patterns (`*`, `history/*`, …) against registered IDs.

use std::collections::BTreeMap;

use cf_alg_mapx::unique;

use crate::descriptor::{AnalyzerMode, Descriptor};
use crate::error::AnalyzeError;
use crate::glob::path_match;

/// Stores analyzer metadata with deterministic ordering. Port of Go `Registry`.
#[derive(Debug, Clone, Default)]
pub struct Registry {
    ordered: Vec<Descriptor>,
    index: BTreeMap<String, Descriptor>,
}

impl Registry {
    /// Creates a registry from descriptor groups, validating modes and
    /// uniqueness. Port of Go `NewRegistry`.
    ///
    /// Each group's descriptors must declare (or default to) the group's mode;
    /// `static` and `raw` analyzers both register under [`AnalyzerMode::Static`]
    /// (matching the Go source, which passes `ModeStatic` for both), and
    /// `history` analyzers under [`AnalyzerMode::History`]. Descriptors with an
    /// empty mode inherit the group mode.
    pub fn new(
        static_descs: &[Descriptor],
        raw_descs: &[Descriptor],
        history_descs: &[Descriptor],
    ) -> Result<Registry, AnalyzeError> {
        let cap = static_descs.len() + raw_descs.len() + history_descs.len();
        let mut ordered: Vec<Descriptor> = Vec::with_capacity(cap);
        let mut index: BTreeMap<String, Descriptor> = BTreeMap::new();

        append_descriptors(AnalyzerMode::Static, static_descs, &mut index, &mut ordered)?;
        append_descriptors(AnalyzerMode::Static, raw_descs, &mut index, &mut ordered)?;
        append_descriptors(AnalyzerMode::History, history_descs, &mut index, &mut ordered)?;

        Ok(Registry { ordered, index })
    }

    /// Returns all descriptors in stable (registration) order. Port of Go `All`.
    pub fn all(&self) -> Vec<Descriptor> {
        self.ordered.clone()
    }

    /// Returns IDs for the given mode in stable order. Port of Go `IDsByMode`.
    pub fn ids_by_mode(&self, mode: AnalyzerMode) -> Vec<String> {
        self.ordered
            .iter()
            .filter(|d| d.mode == mode)
            .map(|d| d.id.clone())
            .collect()
    }

    /// Returns metadata for the given ID. Port of Go `Descriptor`.
    pub fn descriptor(&self, id: &str) -> Option<Descriptor> {
        self.index.get(id).cloned()
    }

    /// Divides IDs by mode, preserving input order. Port of Go `Split`.
    ///
    /// Returns `(static_ids, history_ids)`. Unknown IDs yield
    /// [`AnalyzeError::UnknownAnalyzerId`].
    pub fn split(&self, ids: &[String]) -> Result<(Vec<String>, Vec<String>), AnalyzeError> {
        let mut static_ids = Vec::with_capacity(ids.len());
        let mut history_ids = Vec::with_capacity(ids.len());
        for id in ids {
            let d = self
                .descriptor(id)
                .ok_or_else(|| AnalyzeError::UnknownAnalyzerId(id.clone()))?;
            if d.mode == AnalyzerMode::Static {
                static_ids.push(id.clone());
            } else {
                history_ids.push(id.clone());
            }
        }
        Ok((static_ids, history_ids))
    }

    /// Expands glob patterns against registered IDs, de-duplicating
    /// first-occurrence-wins. Port of Go `ExpandPatterns`.
    pub fn expand_patterns(&self, patterns: &[String]) -> Result<Vec<String>, AnalyzeError> {
        let mut selected: Vec<String> = Vec::with_capacity(self.ordered.len());
        for raw in patterns {
            let ids = self.resolve_pattern(raw.trim())?;
            selected.extend(ids);
        }
        Ok(unique(&selected))
    }

    /// Returns IDs for the patterns, or all IDs when none are specified. Port of
    /// Go `SelectedIDs`.
    pub fn selected_ids(&self, patterns: &[String]) -> Result<Vec<String>, AnalyzeError> {
        if patterns.is_empty() {
            return Ok(self.all_ids());
        }
        self.expand_patterns(patterns)
    }

    fn resolve_pattern(&self, pattern: &str) -> Result<Vec<String>, AnalyzeError> {
        if pattern.is_empty() {
            return Err(AnalyzeError::UnknownAnalyzerId(pattern.to_string()));
        }
        if !has_glob_meta(pattern) {
            if !self.index.contains_key(pattern) {
                return Err(AnalyzeError::UnknownAnalyzerId(pattern.to_string()));
            }
            return Ok(vec![pattern.to_string()]);
        }
        if pattern == "*" {
            return Ok(self.all_ids());
        }
        let matched = self.match_glob(pattern)?;
        if matched.is_empty() {
            return Err(AnalyzeError::UnknownAnalyzerId(pattern.to_string()));
        }
        Ok(matched)
    }

    fn match_glob(&self, pattern: &str) -> Result<Vec<String>, AnalyzeError> {
        let mut matched = Vec::with_capacity(self.ordered.len());
        for d in &self.ordered {
            let is_match = path_match(pattern, &d.id).map_err(|e| AnalyzeError::InvalidAnalyzerGlob {
                pattern: pattern.to_string(),
                source: e.to_string(),
            })?;
            if is_match {
                matched.push(d.id.clone());
            }
        }
        Ok(matched)
    }

    fn all_ids(&self) -> Vec<String> {
        self.ordered.iter().map(|d| d.id.clone()).collect()
    }
}

fn append_descriptors(
    mode: AnalyzerMode,
    descs: &[Descriptor],
    index: &mut BTreeMap<String, Descriptor>,
    ordered: &mut Vec<Descriptor>,
) -> Result<(), AnalyzeError> {
    for descriptor in descs {
        // Go defaults an empty mode to the group mode. Our AnalyzerMode enum
        // cannot be empty, so callers pass the correct mode; we still validate
        // it matches the group, reproducing Go's mode-mismatch error.
        if descriptor.mode != mode {
            return Err(AnalyzeError::InvalidAnalyzerMode {
                id: descriptor.id.clone(),
                expected: mode.to_string(),
                got: descriptor.mode.to_string(),
            });
        }
        if index.contains_key(&descriptor.id) {
            return Err(AnalyzeError::DuplicateAnalyzerId(descriptor.id.clone()));
        }
        index.insert(descriptor.id.clone(), descriptor.clone());
        ordered.push(descriptor.clone());
    }
    Ok(())
}

/// Maps history analyzer IDs to their pipeline keys. Port of Go
/// `HistoryKeysByID`. `leaves` maps a pipeline key to a descriptor; the result
/// preserves `ids` order. Unknown IDs yield [`AnalyzeError::UnknownAnalyzerId`].
pub fn history_keys_by_id(
    leaves: &BTreeMap<String, Descriptor>,
    ids: &[String],
) -> Result<Vec<String>, AnalyzeError> {
    let mut id_to_key: BTreeMap<String, String> = BTreeMap::new();
    for (key, descriptor) in leaves {
        id_to_key.insert(descriptor.id.clone(), key.clone());
    }
    let mut keys = Vec::with_capacity(ids.len());
    for id in ids {
        let key = id_to_key
            .get(id)
            .ok_or_else(|| AnalyzeError::UnknownAnalyzerId(id.clone()))?;
        keys.push(key.clone());
    }
    Ok(keys)
}

fn has_glob_meta(pattern: &str) -> bool {
    pattern.contains(['*', '?', '['])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(id: &str, mode: AnalyzerMode) -> Descriptor {
        Descriptor {
            id: id.to_string(),
            description: String::new(),
            mode,
        }
    }

    fn registry() -> Registry {
        Registry::new(
            &[d("static/a", AnalyzerMode::Static), d("static/b", AnalyzerMode::Static)],
            &[],
            &[d("history/b", AnalyzerMode::History), d("history/c", AnalyzerMode::History)],
        )
        .unwrap()
    }

    #[test]
    fn duplicate_id_rejected() {
        let err = Registry::new(
            &[d("static/a", AnalyzerMode::Static), d("static/a", AnalyzerMode::Static)],
            &[],
            &[],
        )
        .unwrap_err();
        assert!(matches!(err, AnalyzeError::DuplicateAnalyzerId(_)));
    }

    #[test]
    fn mode_mismatch_rejected() {
        let err = Registry::new(&[d("history/x", AnalyzerMode::History)], &[], &[]).unwrap_err();
        assert!(matches!(err, AnalyzeError::InvalidAnalyzerMode { .. }));
    }

    // Port of TestRegistrySplit.
    #[test]
    fn split_by_mode() {
        let reg = registry();
        let (s, h) = reg
            .split(&["static/a".into(), "history/b".into()])
            .unwrap();
        assert_eq!(s, vec!["static/a"]);
        assert_eq!(h, vec!["history/b"]);
    }

    #[test]
    fn split_unknown_id_errors() {
        let reg = registry();
        assert!(reg.split(&["nope".into()]).is_err());
    }

    #[test]
    fn ids_by_mode() {
        let reg = registry();
        assert_eq!(reg.ids_by_mode(AnalyzerMode::Static), vec!["static/a", "static/b"]);
        assert_eq!(reg.ids_by_mode(AnalyzerMode::History), vec!["history/b", "history/c"]);
    }

    #[test]
    fn expand_star_returns_all() {
        let reg = registry();
        let got = reg.expand_patterns(&["*".into()]).unwrap();
        assert_eq!(got, vec!["static/a", "static/b", "history/b", "history/c"]);
    }

    #[test]
    fn expand_glob_segment() {
        let reg = registry();
        let got = reg.expand_patterns(&["history/*".into()]).unwrap();
        assert_eq!(got, vec!["history/b", "history/c"]);
    }

    #[test]
    fn expand_dedups_first_wins() {
        let reg = registry();
        let got = reg
            .expand_patterns(&["history/b".into(), "history/*".into()])
            .unwrap();
        assert_eq!(got, vec!["history/b", "history/c"]);
    }

    #[test]
    fn expand_unknown_literal_errors() {
        let reg = registry();
        assert!(reg.expand_patterns(&["history/zzz".into()]).is_err());
    }

    #[test]
    fn expand_nonmatching_glob_errors() {
        let reg = registry();
        assert!(reg.expand_patterns(&["zzz/*".into()]).is_err());
    }

    #[test]
    fn selected_ids_empty_returns_all() {
        let reg = registry();
        assert_eq!(reg.selected_ids(&[]).unwrap().len(), 4);
    }

    #[test]
    fn history_keys_by_id_maps_keys() {
        let mut leaves = BTreeMap::new();
        leaves.insert("key_b".to_string(), d("history/b", AnalyzerMode::History));
        leaves.insert("key_c".to_string(), d("history/c", AnalyzerMode::History));
        let keys = history_keys_by_id(&leaves, &["history/c".into(), "history/b".into()]).unwrap();
        assert_eq!(keys, vec!["key_c", "key_b"]);
    }
}
