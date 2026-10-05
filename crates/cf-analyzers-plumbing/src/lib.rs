//! cf-analyzers-plumbing: the per-commit facts shared by every history
//! analyzer walk.
//!
//! | Provider | Output |
//! |----------|--------|
//! | [`ticks::HistoryTicks`] | the commit's tick (`--tick-size` buckets) |
//! | [`identity_detector::IdentityDetector`] | the commit's author id |
//!
//! Both are pure functions of the commit stream and the run flags — no wall
//! clock, no host CPU count — so a history report is reproducible on any
//! machine. Tick values and author ids flow into report keys and are part of
//! the output contract.
//!
//! # Example
//!
//! ```
//! use cf_analyzers_plumbing::{HistoryTicks, IdentityDetector, Signature};
//!
//! // Ticks: one-hour buckets relative to the first commit.
//! let mut ticks = HistoryTicks::new(1);
//! assert_eq!(ticks.tick_for(1_577_836_800), 0);
//! assert_eq!(ticks.tick_for(1_577_836_800 + 2 * 3600), 2);
//!
//! // Identity: loose matching unifies a person's aliases under one id.
//! let mut ids = IdentityDetector::new();
//! let sig = |name: &str, email: &str| Signature {
//!     name: name.into(),
//!     email: email.into(),
//!     when_unix: 0,
//! };
//! let a = ids.consume_signature(&sig("alice", "alice@example.com"));
//! let b = ids.consume_signature(&sig("Alice Smith", "alice@example.com"));
//! assert_eq!(a, b);
//! ```

#![forbid(unsafe_code)]

pub mod identity_detector;
pub mod ticks;

pub use identity_detector::{
    anonymize_names, author_missing_id, IdentityDetector, Signature, AUTHOR_MISSING,
    AUTHOR_MISSING_NAME,
};
pub use ticks::{HistoryTicks, DEFAULT_TICK_SIZE_HOURS};
