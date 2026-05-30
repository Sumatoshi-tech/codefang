//! Per-chunk memory telemetry logging.
//!
//! Port of Go `internal/streaming/memlog.go`. Go uses `log/slog`; this crate
//! avoids a hard logging dependency, so [`log_chunk_memory`] writes the
//! structured fields to any [`std::io::Write`] sink in slog's text format
//! (`key=value` pairs). Production builds route this through `tracing` /
//! `cf-observability`; see the crate todos.
//!
//! The derived field values (the integer divisions by [`cf_units`]) reproduce
//! Go's arithmetic exactly.

use std::io::{self, Write};

use cf_units::{KIB, MIB};

/// Memory measurements for a single chunk.
///
/// Port of Go `streaming.ChunkMemoryLog`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ChunkMemoryLog {
    /// Zero-based chunk index (logged as `chunk = index + 1`).
    pub chunk_index: i32,
    /// Heap-in-use bytes before processing the chunk.
    pub heap_before: i64,
    /// Heap-in-use bytes after processing the chunk.
    pub heap_after: i64,
    /// System memory bytes after processing the chunk.
    pub sys_after: i64,
    /// Resident-set-size bytes after processing the chunk.
    pub rss_after: i64,
    /// Percentage of the memory budget used.
    pub budget_used_pct: f64,
    /// Observed per-commit state growth in bytes.
    pub growth_per_commit: i64,
    /// Smoothed EMA growth rate in bytes.
    pub ema_growth_rate: f64,
    /// Whether this chunk triggered a re-plan.
    pub replanned: bool,
}

/// The structured-log message emitted by [`log_chunk_memory`].
pub const CHUNK_MEMORY_MSG: &str = "streaming: chunk memory";

/// Emits a structured log entry with per-chunk memory telemetry to `out`.
///
/// The fields, their order, their keys, and their (integer-divided) values
/// reproduce Go's `slog.InfoContext(ctx, "streaming: chunk memory", ...)` call
/// exactly:
///
/// - `chunk` = `chunk_index + 1`
/// - `heap_before_mib` = `heap_before / MiB`
/// - `heap_after_mib`  = `heap_after / MiB`
/// - `sys_mib`         = `sys_after / MiB`
/// - `rss_mib`         = `rss_after / MiB`
/// - `budget_used_pct` = `budget_used_pct`
/// - `growth_per_commit_kib` = `growth_per_commit / KiB`
/// - `ema_growth_kib`  = `(ema_growth_rate as i64) / KiB`
/// - `replanned`       = `replanned`
///
/// # Errors
///
/// Returns any [`io::Error`] from writing to `out`.
pub fn log_chunk_memory<W: Write>(out: &mut W, entry: &ChunkMemoryLog) -> io::Result<()> {
    writeln!(
        out,
        "level=INFO msg=\"{msg}\" \
         chunk={chunk} \
         heap_before_mib={heap_before_mib} \
         heap_after_mib={heap_after_mib} \
         sys_mib={sys_mib} \
         rss_mib={rss_mib} \
         budget_used_pct={budget_used_pct} \
         growth_per_commit_kib={growth_per_commit_kib} \
         ema_growth_kib={ema_growth_kib} \
         replanned={replanned}",
        msg = CHUNK_MEMORY_MSG,
        chunk = entry.chunk_index + 1,
        heap_before_mib = entry.heap_before / MIB,
        heap_after_mib = entry.heap_after / MIB,
        sys_mib = entry.sys_after / MIB,
        rss_mib = entry.rss_after / MIB,
        budget_used_pct = entry.budget_used_pct,
        growth_per_commit_kib = entry.growth_per_commit / KIB,
        ema_growth_kib = (entry.ema_growth_rate as i64) / KIB,
        replanned = entry.replanned,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mirrors Go TestLogChunkMemory_EmitsStructuredFields.
    #[test]
    fn emits_structured_fields() {
        let mut buf = Vec::new();
        log_chunk_memory(
            &mut buf,
            &ChunkMemoryLog {
                chunk_index: 2,
                heap_before: 500 * KIB * 1024,
                heap_after: 900 * KIB * 1024,
                budget_used_pct: 43.5,
                growth_per_commit: 478 * KIB,
                ema_growth_rate: 502.0 * KIB as f64,
                replanned: false,
                ..ChunkMemoryLog::default()
            },
        )
        .unwrap();

        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("streaming: chunk memory"), "{output}");
        assert!(output.contains("chunk=3"), "{output}");
        assert!(output.contains("replanned=false"), "{output}");
    }

    // Mirrors Go TestLogChunkMemory_ReplanTrue.
    #[test]
    fn replan_true() {
        let mut buf = Vec::new();
        log_chunk_memory(
            &mut buf,
            &ChunkMemoryLog {
                chunk_index: 0,
                heap_before: 0,
                heap_after: 100 * KIB * 1024,
                budget_used_pct: 10.0,
                growth_per_commit: 100 * KIB,
                ema_growth_rate: 100.0 * KIB as f64,
                replanned: true,
                ..ChunkMemoryLog::default()
            },
        )
        .unwrap();

        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("replanned=true"), "{output}");
    }
}
