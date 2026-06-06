//! `cf-plotpage` — INTENTIONAL SCAFFOLD (documented deferral).
//!
//! Go origin: `internal/analyzers/common/plotpage` (1629 LOC) — renders the
//! multi-page HTML for `run --format plot|html`. That path writes to an output
//! DIRECTORY (`-o`) and emits EMPTY stdout, so it produces NO byte-gated golden
//! capture (MANIFEST `plotHtmlNote`: plot/html are nonBinding by nature). This
//! crate is therefore the sole remaining bare scaffold and is NOT on any binding
//! path; it is depended on only by `cf-commands` (link-through). Implementation
//! is deferred until the human-rendered plot/html views are ported (out of
//! binding scope). See ARCHITECTURE.md §8.2 and ROADMAP.md Step 13.
#![allow(dead_code)]

/// Crate name, used by smoke tests to confirm the module links.
pub const CRATE_NAME: &str = "cf-plotpage";
