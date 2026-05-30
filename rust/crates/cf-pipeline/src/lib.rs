//! Rust port of the Go package `pkg/pipeline`.
//!
//! Provides configuration-option types for analysis pipeline items and
//! composable building blocks for concurrent pipeline construction:
//!
//! - [`RunPc`] — a producer-consumer goroutine skeleton (`runpc.go`).
//! - [`Phase`] / [`run_phases`] — a chain-of-responsibility over a threaded
//!   state value (`phase.go`).
//! - [`Batcher`] family ([`ThresholdBatcher`], [`PassthroughBatcher`]) —
//!   batching strategies (`batcher.go`).
//! - [`DispatchFunc`] — a dispatch strategy alias (`dispatch.go`).
//! - [`Fetcher`] / [`FetcherFunc`] — the cache-decorator base interface
//!   (`fetcher.go`).
//! - [`WorkerPool`] — bounded-concurrency item processing (`workerpool.go`).
//! - [`signal_on_drain`] — fan a channel through while signalling exhaustion
//!   (`drain.go`).
//! - [`SharedResponse`] — evaluate a computation exactly once (`shared_response.go`).
//! - [`ConfigurationOption`] / [`ConfigurationOptionType`] — the unified option
//!   description used to build CLI flags (`options.go`).
//!
//! # Mapping from Go to Rust
//!
//! Go's `context.Context` is reproduced by [`Ctx`], a cheap clonable
//! cancellation handle: [`Ctx::err`] mirrors `ctx.Err()` and
//! [`Ctx::is_cancelled`] mirrors `ctx.Err() != nil`. Go goroutines map to OS
//! threads and Go channels map to [`crossbeam_channel`] channels, which
//! reproduce Go's buffered/unbuffered, blocking-send/recv, and close-on-drop
//! semantics. Go generics map to Rust generics.
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
