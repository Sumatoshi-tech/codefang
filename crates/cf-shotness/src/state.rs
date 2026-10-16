//! The live entity state of a shotness walk — the ONE source of truth every
//! report is built from — and the per-commit [`EntityEffect`] records it is
//! built from.
//!
//! Entities (functions, methods, classes — whatever the structural DSL
//! selects) are keyed by [`NodeSummary::key`] (`Type_Name_File`). A walk
//! records, per commit, what the commit did to each entity
//! ([`CommitEffects`]): a *touch* (its hunks overlapped the entity's span, or
//! created it), a *move* (a file rename changed the entity's key) and an
//! *end* (the declaration, or its file, was removed). [`ShotnessState::apply`]
//! replays one commit's effects: moves first (keys rewritten, counts and
//! couplings preserved), then touches (one count per entity, the touched
//! entities coupled pairwise), then ends (the entity and every coupling that
//! referenced it are dropped). Replaying every commit in walk order over an
//! empty state yields [`ShotnessState::report`]: exactly the entities alive
//! at the end of the window.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::types::{NodeSummary, ReportData};

/// Minimum touched entities in one commit to form a coupling pair.
pub const MIN_COUPLING_NODES: usize = 2;
/// Cap on touched entities per commit for the pairwise coupling update: above
/// it (a mass refactor) the O(n²) update is skipped as noise.
pub const MAX_COUPLING_NODES: usize = 500;
/// Divisor in `C(n,2) = n*(n-1)/2`.
const COMBINATORIAL_PAIR_DIVISOR: i64 = 2;

/// One entity of the window: its identity, self-change count, co-change
/// counters and whether it has ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeShotness {
    /// Entity identity.
    pub summary: NodeSummary,
    /// Number of commits that touched the entity.
    pub count: i64,
    /// Co-change counters keyed by the *other* entity's key (symmetric with
    /// the partner's map; a `BTreeMap` keeps report building deterministic).
    pub couples: BTreeMap<String, i64>,
    /// The declaration (or its file) was removed and not declared again: the
    /// entity stays in the report with its history, marked ended.
    pub ended: bool,
}

/// What one commit did to one entity. Keyed in [`CommitEffects`] by the
/// entity's key AFTER any move in that commit.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EntityEffect {
    /// The entity's identity after the commit.
    pub summary: NodeSummary,
    /// The commit's hunks overlapped the entity's span (or created it): one
    /// change is counted and the entity is coupled with the commit's other
    /// touched entities.
    pub touched: bool,
    /// The declaration (or its file) was removed: the entity ends after this
    /// commit's touches are counted.
    pub ended: bool,
    /// The entity's key before a file rename in this commit.
    pub moved_from: Option<String>,
}

impl EntityEffect {
    /// A touch of `summary`.
    #[must_use]
    pub fn touched(summary: NodeSummary) -> Self {
        Self {
            summary,
            touched: true,
            ended: false,
            moved_from: None,
        }
    }
}

/// One commit's effects, keyed by entity key — the per-commit record the walk
/// returns and every format reads.
pub type CommitEffects = BTreeMap<String, EntityEffect>;

/// The entities a commit touched (key → identity), in key order.
pub fn touched_entities(effects: &CommitEffects) -> impl Iterator<Item = (&String, &NodeSummary)> {
    effects
        .iter()
        .filter(|(_, e)| e.touched)
        .map(|(k, e)| (k, &e.summary))
}

/// The number of coupling pairs `n` touched entities form: `C(n,2)`, zero
/// below [`MIN_COUPLING_NODES`].
///
/// ```
/// use cf_shotness::state::coupling_pair_count;
///
/// assert_eq!(coupling_pair_count(1), 0);
/// assert_eq!(coupling_pair_count(2), 1);
/// assert_eq!(coupling_pair_count(4), 6);
/// ```
#[must_use]
#[allow(clippy::cast_possible_wrap)] // touched-entity counts are small
pub fn coupling_pair_count(n: usize) -> i64 {
    if n < MIN_COUPLING_NODES {
        return 0;
    }
    let n = n as i64;
    n * (n - 1) / COMBINATORIAL_PAIR_DIVISOR
}

/// The entity set of a walk. By default an entity that ends is dropped with
/// its couplings; a state built with [`ShotnessState::keeping_ended`] keeps
/// it, marked ended (`--shotness-keep-ended`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShotnessState {
    /// Entity key → entity.
    nodes: BTreeMap<String, NodeShotness>,
    /// File path → the keys of the LIVE entities declared in it.
    files: BTreeMap<String, BTreeSet<String>>,
    /// Ended entities stay, marked, instead of being dropped.
    keep_ended: bool,
}

impl ShotnessState {
    /// An empty state that drops ended entities.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty state that keeps ended entities with their counts and
    /// couplings, marked ended.
    #[must_use]
    pub fn keeping_ended() -> Self {
        Self {
            keep_ended: true,
            ..Self::default()
        }
    }

    /// Number of entities, live and ended.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether no entity was seen.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The entity under `key`, live or ended.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&NodeShotness> {
        self.nodes.get(key)
    }

    /// The live entities declared in `file`, in key order.
    pub fn entities_in(&self, file: &str) -> impl Iterator<Item = &NodeShotness> {
        self.files
            .get(file)
            .into_iter()
            .flatten()
            .filter_map(|key| self.nodes.get(key))
    }

    /// Applies one commit's effects — moves, then touches (coupled pairwise),
    /// then ends — and returns the number of coupling pairs the commit formed.
    pub fn apply(&mut self, effects: &CommitEffects) -> i64 {
        for effect in effects.values() {
            if let Some(from) = &effect.moved_from {
                self.move_entity(from, &effect.summary);
            }
        }
        let mut touched: Vec<&str> = Vec::new();
        for (key, effect) in effects {
            if effect.touched {
                self.touch(&effect.summary);
                touched.push(key);
            }
        }
        let pairs = self.record_couplings(&touched);
        for (key, effect) in effects {
            if effect.ended {
                if self.keep_ended {
                    self.end_entity(key);
                } else {
                    self.drop_entity(key);
                }
            }
        }
        pairs
    }

    /// Counts one change on `summary`: a new entity is born with count 1, a
    /// known one is incremented, an ended one is declared again and keeps
    /// its history.
    fn touch(&mut self, summary: &NodeSummary) {
        let key = summary.key();
        let node = self
            .nodes
            .entry(key.clone())
            .or_insert_with(|| NodeShotness {
                summary: summary.clone(),
                count: 0,
                couples: BTreeMap::new(),
                ended: false,
            });
        node.count += 1;
        node.ended = false;
        self.files
            .entry(summary.file.clone())
            .or_default()
            .insert(key);
    }

    /// Couples every pair of touched entities (both directions) and returns
    /// the pair count. Above [`MAX_COUPLING_NODES`] the counters are left
    /// untouched (the pair count is still returned).
    fn record_couplings(&mut self, touched: &[&str]) -> i64 {
        let n = touched.len();
        let pairs = coupling_pair_count(n);
        if !(MIN_COUPLING_NODES..=MAX_COUPLING_NODES).contains(&n) {
            return pairs;
        }
        for (i, first) in touched.iter().enumerate() {
            for second in &touched[i + 1..] {
                self.bump_couple(first, second);
                self.bump_couple(second, first);
            }
        }
        pairs
    }

    /// Increments `from`'s counter towards `to`.
    fn bump_couple(&mut self, from: &str, to: &str) {
        if let Some(node) = self.nodes.get_mut(from) {
            *node.couples.entry(to.to_string()).or_insert(0) += 1;
        }
    }

    /// Ends the entity under `key` in a state that keeps ended entities: it
    /// leaves its file's live set and is marked ended, keeping its count and
    /// couplings. Unknown keys are a no-op.
    fn end_entity(&mut self, key: &str) {
        let Some(node) = self.nodes.get_mut(key) else {
            return;
        };
        node.ended = true;
        if let Some(keys) = self.files.get_mut(&node.summary.file) {
            keys.remove(key);
        }
    }

    /// Drops the entity under `key` entirely: it and the partners' counters
    /// that referenced it. Unknown keys are a no-op.
    fn drop_entity(&mut self, key: &str) {
        let Some(node) = self.nodes.remove(key) else {
            return;
        };
        for partner in node.couples.keys() {
            if let Some(p) = self.nodes.get_mut(partner) {
                p.couples.remove(key);
            }
        }
        if let Some(keys) = self.files.get_mut(&node.summary.file) {
            keys.remove(key);
        }
    }

    /// Moves the entity under `from` to the identity `to`: the key is
    /// rewritten, count and couplings preserved, the partners' counters
    /// re-pointed. A previous occupant of the destination key (live or
    /// ended) is replaced and dropped.
    fn move_entity(&mut self, from: &str, to: &NodeSummary) {
        let to_key = to.key();
        if from == to_key {
            return;
        }
        let Some(mut node) = self.nodes.remove(from) else {
            return;
        };
        self.drop_entity(&to_key);
        // The dropped occupant may have been a partner; an entity never couples
        // with its own key.
        node.couples.remove(&to_key);
        if let Some(keys) = self.files.get_mut(&node.summary.file) {
            keys.remove(from);
        }
        for partner in node.couples.keys() {
            if let Some(p) = self.nodes.get_mut(partner) {
                if let Some(count) = p.couples.remove(from) {
                    p.couples.insert(to_key.clone(), count);
                }
            }
        }
        node.summary = to.clone();
        self.files
            .entry(to.file.clone())
            .or_default()
            .insert(to_key.clone());
        self.nodes.insert(to_key, node);
    }

    /// Builds the index-keyed `Nodes`/`Counters` report over every entity,
    /// ended ones marked:
    /// entities ordered by key, `counters[i][i]` the self-change count and
    /// `counters[i][j]` the co-change count with entity `j`.
    #[must_use]
    pub fn report(&self) -> ReportData {
        let index: HashMap<&str, usize> = self
            .nodes
            .keys()
            .enumerate()
            .map(|(i, k)| (k.as_str(), i))
            .collect();
        let mut nodes = Vec::with_capacity(self.nodes.len());
        let mut counters = Vec::with_capacity(self.nodes.len());
        let mut ended = Vec::with_capacity(self.nodes.len());
        for (i, node) in self.nodes.values().enumerate() {
            nodes.push(node.summary.clone());
            ended.push(node.ended);
            let mut counter: HashMap<usize, i64> = HashMap::new();
            counter.insert(i, node.count);
            for (partner, count) in &node.couples {
                if let Some(&j) = index.get(partner.as_str()) {
                    counter.insert(j, *count);
                }
            }
            counters.push(counter);
        }
        ReportData {
            nodes,
            counters,
            ended,
            ended_tracked: self.keep_ended,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(name: &str, file: &str) -> NodeSummary {
        NodeSummary::new("Function", name, file)
    }

    /// A commit touching every `entities`.
    fn touches(entities: &[NodeSummary]) -> CommitEffects {
        entities
            .iter()
            .map(|e| (e.key(), EntityEffect::touched(e.clone())))
            .collect()
    }

    /// A commit moving every entity of `from` to `to`.
    fn file_move(state: &ShotnessState, from: &str, to: &str) -> CommitEffects {
        state
            .entities_in(from)
            .map(|n| {
                let moved = NodeSummary::new(&n.summary.type_, &n.summary.name, to);
                (
                    moved.key(),
                    EntityEffect {
                        summary: moved,
                        touched: false,
                        ended: false,
                        moved_from: Some(n.summary.key()),
                    },
                )
            })
            .collect()
    }

    /// A commit ending `entities`.
    fn ends(entities: &[NodeSummary]) -> CommitEffects {
        entities
            .iter()
            .map(|e| {
                (
                    e.key(),
                    EntityEffect {
                        summary: e.clone(),
                        touched: false,
                        ended: true,
                        moved_from: None,
                    },
                )
            })
            .collect()
    }

    fn count(state: &ShotnessState, key: &str) -> i64 {
        state.get(key).map_or(0, |n| n.count)
    }

    #[test]
    fn a_touch_counts_once_per_commit_and_accumulates() {
        let mut state = ShotnessState::new();
        let f = entity("f", "a.go");
        state.apply(&touches(std::slice::from_ref(&f)));
        assert_eq!(count(&state, &f.key()), 1);
        state.apply(&touches(std::slice::from_ref(&f)));
        assert_eq!(count(&state, &f.key()), 2);
        assert_eq!(state.len(), 1);
    }

    #[test]
    fn couplings_are_symmetric_and_additive() {
        let mut state = ShotnessState::new();
        let (f, g) = (entity("f", "a.go"), entity("g", "a.go"));
        assert_eq!(state.apply(&touches(&[f.clone(), g.clone()])), 1);
        state.apply(&touches(&[f.clone(), g.clone()]));
        assert_eq!(state.apply(&touches(std::slice::from_ref(&f))), 0);
        assert_eq!(state.get(&f.key()).unwrap().couples[&g.key()], 2);
        assert_eq!(state.get(&g.key()).unwrap().couples[&f.key()], 2);
        assert_eq!(count(&state, &f.key()), 3);
    }

    #[test]
    fn pair_count_follows_c_n_2_and_the_cap_skips_the_counters() {
        let mut state = ShotnessState::new();
        let three: Vec<NodeSummary> = ["a", "b", "c"].iter().map(|n| entity(n, "x")).collect();
        assert_eq!(state.apply(&touches(&three)), 3);

        let many: Vec<NodeSummary> = (0..=MAX_COUPLING_NODES)
            .map(|i| entity(&format!("f{i}"), "x"))
            .collect();
        let mut big = ShotnessState::new();
        let n = i64::try_from(MAX_COUPLING_NODES + 1).expect("small cap");
        assert_eq!(big.apply(&touches(&many)), n * (n - 1) / 2);
        assert!(big
            .get(&entity("f0", "x").key())
            .unwrap()
            .couples
            .is_empty());
    }

    #[test]
    fn a_file_move_keeps_counts_and_re_points_couplings() {
        let mut state = ShotnessState::new();
        let (f, g, h) = (
            entity("f", "m.go"),
            entity("g", "m.go"),
            entity("h", "o.go"),
        );
        state.apply(&touches(&[f.clone(), g.clone(), h.clone()]));
        state.apply(&touches(std::slice::from_ref(&f)));
        let moved = file_move(&state, "m.go", "n.go");
        assert_eq!(moved.len(), 2);
        state.apply(&moved);

        let (f2, g2) = (entity("f", "n.go"), entity("g", "n.go"));
        assert_eq!(state.len(), 3, "entities move, they do not duplicate");
        assert!(state.get(&f.key()).is_none());
        assert_eq!(count(&state, &f2.key()), 2);
        assert_eq!(count(&state, &g2.key()), 1);
        assert_eq!(state.get(&f2.key()).unwrap().couples[&g2.key()], 1);
        assert_eq!(state.get(&h.key()).unwrap().couples[&f2.key()], 1);
        assert!(!state.get(&h.key()).unwrap().couples.contains_key(&f.key()));
        assert_eq!(state.entities_in("m.go").count(), 0);

        // The moved entities keep accumulating under the new path, and move again.
        state.apply(&touches(std::slice::from_ref(&f2)));
        assert_eq!(count(&state, &f2.key()), 3);
        let moved = file_move(&state, "n.go", "p.go");
        state.apply(&moved);
        assert_eq!(count(&state, &entity("f", "p.go").key()), 3);
        assert_eq!(state.len(), 3);
    }

    #[test]
    fn a_move_in_the_same_commit_as_a_touch_counts_under_the_new_key() {
        let mut state = ShotnessState::new();
        let f = entity("f", "m.go");
        state.apply(&touches(std::slice::from_ref(&f)));
        let f2 = entity("f", "n.go");
        let mut effects = file_move(&state, "m.go", "n.go");
        effects.get_mut(&f2.key()).unwrap().touched = true;
        state.apply(&effects);
        assert_eq!(count(&state, &f2.key()), 2);
        assert!(state.get(&f.key()).is_none());
    }

    #[test]
    fn a_move_onto_an_occupied_key_replaces_the_occupant() {
        let mut state = ShotnessState::new();
        let (old, incoming) = (entity("f", "b.go"), entity("f", "a.go"));
        state.apply(&touches(std::slice::from_ref(&old)));
        state.apply(&touches(&[old.clone(), incoming.clone()]));
        state.apply(&touches(std::slice::from_ref(&incoming)));
        state.apply(&file_move(&state, "a.go", "b.go"));
        assert_eq!(state.len(), 1);
        assert_eq!(count(&state, &old.key()), 2, "the mover's count survives");
        assert!(state.get(&old.key()).unwrap().couples.is_empty());
    }

    #[test]
    fn an_end_drops_the_entity_and_its_couplings() {
        let mut state = ShotnessState::new();
        let (f, g, h) = (
            entity("f", "m.go"),
            entity("g", "m.go"),
            entity("h", "o.go"),
        );
        state.apply(&touches(&[f.clone(), g.clone(), h.clone()]));
        state.apply(&ends(std::slice::from_ref(&g)));
        assert_eq!(state.len(), 2);
        assert!(!state.get(&f.key()).unwrap().couples.contains_key(&g.key()));
        assert_eq!(state.get(&f.key()).unwrap().couples[&h.key()], 1);
        assert_eq!(state.entities_in("m.go").count(), 1);
        // Ending twice, or an unknown key, is a no-op.
        state.apply(&ends(&[g.clone(), entity("nope", "x")]));
        assert_eq!(state.len(), 2);
    }

    #[test]
    fn a_touch_and_an_end_in_one_commit_count_then_drop() {
        let mut state = ShotnessState::new();
        let (f, f2) = (entity("f", "m.go"), entity("f2", "m.go"));
        state.apply(&touches(std::slice::from_ref(&f)));
        let mut effects = touches(&[f.clone(), f2.clone()]);
        effects.get_mut(&f.key()).unwrap().ended = true;
        assert_eq!(
            state.apply(&effects),
            1,
            "the pair is formed before the end"
        );
        assert_eq!(state.len(), 1);
        assert_eq!(count(&state, &f2.key()), 1);
        assert!(state.get(&f2.key()).unwrap().couples.is_empty());
    }

    #[test]
    fn a_dropped_entity_declared_again_starts_over() {
        let mut state = ShotnessState::new();
        let g = entity("g", "m.go");
        state.apply(&touches(std::slice::from_ref(&g)));
        state.apply(&ends(std::slice::from_ref(&g)));
        state.apply(&touches(std::slice::from_ref(&g)));
        assert_eq!(count(&state, &g.key()), 1);
        assert!(!state.report().ended_tracked);
    }

    #[test]
    fn an_end_keeps_the_entity_and_its_couplings_marked_ended() {
        let mut state = ShotnessState::keeping_ended();
        let (f, g, h) = (
            entity("f", "m.go"),
            entity("g", "m.go"),
            entity("h", "o.go"),
        );
        state.apply(&touches(&[f.clone(), g.clone(), h.clone()]));
        state.apply(&ends(std::slice::from_ref(&g)));
        assert_eq!(state.len(), 3, "an ended entity stays in the report");
        assert!(state.get(&g.key()).unwrap().ended);
        assert_eq!(state.get(&f.key()).unwrap().couples[&g.key()], 1);
        assert_eq!(state.get(&g.key()).unwrap().couples[&f.key()], 1);
        assert_eq!(
            state.entities_in("m.go").count(),
            1,
            "only live entities belong to their file"
        );
        // Ending twice, or an unknown key, is a no-op.
        state.apply(&ends(&[g.clone(), entity("nope", "x")]));
        assert_eq!(state.len(), 3);
        assert_eq!(count(&state, &g.key()), 1);
    }

    #[test]
    fn a_touch_and_an_end_in_one_commit_count_then_end() {
        let mut state = ShotnessState::keeping_ended();
        let (f, f2) = (entity("f", "m.go"), entity("f2", "m.go"));
        state.apply(&touches(std::slice::from_ref(&f)));
        let mut effects = touches(&[f.clone(), f2.clone()]);
        effects.get_mut(&f.key()).unwrap().ended = true;
        assert_eq!(
            state.apply(&effects),
            1,
            "the pair is formed before the end"
        );
        assert_eq!(count(&state, &f.key()), 2);
        assert!(state.get(&f.key()).unwrap().ended);
        assert!(!state.get(&f2.key()).unwrap().ended);
        assert_eq!(state.get(&f2.key()).unwrap().couples[&f.key()], 1);
    }

    #[test]
    fn a_touch_revives_an_ended_entity() {
        let mut state = ShotnessState::keeping_ended();
        let g = entity("g", "m.go");
        state.apply(&touches(std::slice::from_ref(&g)));
        state.apply(&ends(std::slice::from_ref(&g)));
        state.apply(&touches(std::slice::from_ref(&g)));
        let node = state.get(&g.key()).unwrap();
        assert!(!node.ended, "declared again");
        assert_eq!(node.count, 2, "its history continues");
        assert_eq!(state.entities_in("m.go").count(), 1);
    }

    #[test]
    fn report_marks_ended_entities() {
        let mut state = ShotnessState::keeping_ended();
        let (f, g) = (entity("f", "m.go"), entity("g", "m.go"));
        state.apply(&touches(&[f.clone(), g.clone()]));
        state.apply(&ends(std::slice::from_ref(&g)));
        let report = state.report();
        assert_eq!(report.nodes, vec![f, g]);
        assert_eq!(report.ended, vec![false, true]);
        assert!(report.ended_tracked);
        assert_eq!(
            report.counters[1][&0], 1,
            "the ended entity keeps its couplings"
        );
    }

    #[test]
    fn report_orders_by_key_with_self_and_pair_counters() {
        let mut state = ShotnessState::new();
        let (foo, bar) = (entity("foo", "a.go"), entity("bar", "a.go"));
        state.apply(&touches(&[foo.clone(), bar.clone()]));
        state.apply(&touches(std::slice::from_ref(&foo)));
        let report = state.report();
        // Function_bar_a.go < Function_foo_a.go
        assert_eq!(report.nodes, vec![bar, foo]);
        assert_eq!(report.counters[0][&0], 1);
        assert_eq!(report.counters[1][&1], 2);
        assert_eq!(report.counters[0][&1], 1);
        assert_eq!(report.counters[1][&0], 1);
    }

    #[test]
    fn touched_entities_lists_only_touches() {
        let f = entity("f", "m.go");
        let mut effects = touches(std::slice::from_ref(&f));
        effects.extend(ends(&[entity("g", "m.go")]));
        let touched: Vec<&String> = touched_entities(&effects).map(|(k, _)| k).collect();
        assert_eq!(touched, vec![&f.key()]);
    }

    #[test]
    fn empty_state_reports_nothing() {
        let state = ShotnessState::new();
        assert!(state.is_empty());
        assert_eq!(state.report(), ReportData::default());
    }
}
