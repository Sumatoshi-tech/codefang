//! Composable building blocks for concurrent pipeline construction, plus the
//! configuration-option types for analysis pipeline items:
//!
//! - [`RunPc`] — a producer-consumer skeleton.
//! - [`Phase`] / [`run_phases`] — a chain-of-responsibility over a threaded
//!   state value.
//! - [`Batcher`] family ([`ThresholdBatcher`], [`PassthroughBatcher`]) —
//!   batching strategies.
//! - [`DispatchFunc`] — a dispatch strategy alias.
//! - [`Fetcher`] / [`FetcherFunc`] — the cache-decorator base interface.
//! - [`WorkerPool`] — bounded-concurrency item processing.
//! - [`signal_on_drain`] — fan a channel through while signalling exhaustion.
//! - [`SharedResponse`] — evaluate a computation exactly once.
//! - [`ConfigurationOption`] / [`ConfigurationOptionType`] — the unified option
//!   description used to build CLI flags.
//!
//! # Concurrency model
//!
//! Cancellation is modeled by [`Ctx`], a cheap clonable handle: [`Ctx::err`]
//! reports the cancellation error and [`Ctx::is_cancelled`] is the boolean
//! guard. Workers are OS threads communicating over [`crossbeam_channel`]
//! channels (bounded/unbounded, blocking send/recv, close-on-drop).
//!
//! This crate emits no machine-format report bytes, so — unlike the report
//! crates — it does not route through `cf-gojson` / `cf-goyaml`.

mod batcher;
mod context;
mod dispatch;
mod drain;
mod fetcher;
mod options;
mod phase;
mod runpc;
mod shared_response;
mod workerpool;

pub use batcher::{Batcher, PassthroughBatcher, ThresholdBatcher};
pub use context::{ContextError, Ctx};
pub use dispatch::DispatchFunc;
pub use drain::signal_on_drain;
pub use fetcher::{Fetcher, FetcherFunc};
pub use options::{ConfigurationOption, ConfigurationOptionType, DefaultValue};
pub use phase::{run_phases, Phase, PhaseFunc};
pub use runpc::RunPc;
pub use shared_response::SharedResponse;
pub use workerpool::{WorkerPool, WorkerPoolError};
