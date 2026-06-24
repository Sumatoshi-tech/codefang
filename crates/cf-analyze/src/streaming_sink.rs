//! NDJSON streaming sink — one JSON line per `TC`.
//!
//! [`NdjsonLine`] is a wrapper struct: its fields serialize in declaration order (`hash`, `tick`,
//! `author_id`, `timestamp`, `analyzer`, `data`). The timestamp is rendered with
//! the contract RFC3339 emitter (UTC → `Z`) and only when the commit time is
//! non-zero (zero timestamps are skipped; report contract).

use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use cf_gojson::{Encoder, GoMap, GoValue, MapOrigin};

pub use crate::tc::{Tc, Tick};

use crate::metadata::format_rfc3339_utc;

/// The JSON structure for one NDJSON output line.
///
/// Wrapper struct — field order is
/// `hash`, `tick`, `author_id`, `timestamp`, `analyzer`, `data`.
#[derive(Debug, Clone)]
pub struct NdjsonLine {
    /// Commit hex hash (`hash`).
    pub hash: String,
    /// Tick index (`tick`).
    pub tick: i32,
    /// Author numeric id (`author_id`).
    pub author_id: i32,
    /// RFC3339 timestamp, empty for a zero commit time (`timestamp`).
    pub timestamp: String,
    /// Analyzer flag (`analyzer`).
    pub analyzer: String,
    /// Analyzer-specific payload (`data`).
    pub data: GoValue,
}

impl NdjsonLine {
    /// Builds the wrapper [`GoValue`] in declaration order for serialization.
    #[must_use]
    pub fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new(MapOrigin::Struct);
        m.insert("hash", GoValue::Str(self.hash.clone()));
        m.insert("tick", GoValue::Int(i64::from(self.tick)));
        m.insert("author_id", GoValue::Int(i64::from(self.author_id)));
        m.insert("timestamp", GoValue::Str(self.timestamp.clone()));
        m.insert("analyzer", GoValue::Str(self.analyzer.clone()));
        m.insert("data", self.data.clone());
        GoValue::Map(m)
    }
}

/// Writes one NDJSON line per `TC` to a writer.
///
/// Thread-safe: concurrent
/// [`write_tc`](StreamingSink::write_tc) calls are serialized via a mutex over
/// the underlying writer via a mutex. The compact encoder
/// (HTML escaping on, trailing `\n` per line) reproduces `json.NewEncoder`.
pub struct StreamingSink<W: Write> {
    inner: Mutex<W>,
    encoder: Encoder,
}

impl<W: Write> StreamingSink<W> {
    /// Creates a sink writing to `w`.
    pub fn new(w: W) -> Self {
        Self {
            inner: Mutex::new(w),
            encoder: Encoder::compact().with_trailing_newline(true),
        }
    }

    /// Writes one NDJSON line for `tc`; skips TCs with no data.
    ///
    /// Returns early (no output) when `tc.data` is `None`; the timestamp is
    /// RFC3339 only when the commit time is present.
    ///
    /// # Errors
    /// Returns a [`SinkError`] if writing to the underlying writer fails.
    pub fn write_tc(&self, tc: &Tc, analyzer_flag: &str) -> Result<(), SinkError> {
        let Some(data) = tc.data.clone() else {
            return Ok(());
        };

        let timestamp = match tc.timestamp {
            Some(t) => system_time_to_rfc3339(t),
            None => String::new(),
        };

        let line = NdjsonLine {
            hash: tc.commit_hash.as_str().to_string(),
            tick: tc.tick,
            author_id: tc.author_id,
            timestamp,
            analyzer: analyzer_flag.to_string(),
            data,
        };

        let bytes = self.encoder.encode(&line.to_go_value());

        let mut w = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        w.write_all(&bytes)
            .map_err(|e| SinkError(format!("ndjson encode: {e}")))
    }
}

/// Formats a [`SystemTime`] as contract RFC3339 in UTC.
///
/// Truncates to whole seconds (the streaming sink emits no fractional part) and
/// renders via [`format_rfc3339_utc`], matching `tc.Timestamp.Format(time.RFC3339)`.
fn system_time_to_rfc3339(t: SystemTime) -> String {
    let secs = t
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    format_rfc3339_utc(secs)
}

/// Error writing an NDJSON line.
#[derive(Debug)]
pub struct SinkError(pub String);

impl std::fmt::Display for SinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SinkError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tc::CommitHash;
    use std::sync::Arc;
    use std::time::Duration;

    fn ts(year_secs: i64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(year_secs as u64)
    }

    fn go_map(pairs: &[(&str, GoValue)]) -> GoValue {
        let mut m = GoMap::new(MapOrigin::Map);
        for (k, v) in pairs {
            m.insert(*k, v.clone());
        }
        GoValue::Map(m)
    }

    // TestStreamingSink_WriteTC_SingleLine (streaming_sink_test.go:20).
    // 2024-01-15T10:30:00Z == 1705314600 unix seconds.
    #[test]
    fn write_tc_single_line() {
        let mut buf = Vec::new();
        {
            let sink = StreamingSink::new(&mut buf);
            let tc = Tc {
                commit_hash: CommitHash::new("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                tick: 0,
                author_id: 1,
                timestamp: Some(ts(1_705_314_600)),
                data: Some(go_map(&[("score", GoValue::Int(42))])),
            };
            sink.write_tc(&tc, "quality").expect("write");
        }
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("\"hash\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\""));
        assert!(s.contains("\"tick\":0"));
        assert!(s.contains("\"author_id\":1"));
        assert!(s.contains("\"timestamp\":\"2024-01-15T10:30:00Z\""));
        assert!(s.contains("\"analyzer\":\"quality\""));
        assert!(s.contains("\"data\""));
        assert!(s.ends_with('\n'));
    }

    // TestStreamingSink_WriteTC_NilData (streaming_sink_test.go:51).
    #[test]
    fn write_tc_nil_data() {
        let mut buf = Vec::new();
        {
            let sink = StreamingSink::new(&mut buf);
            let tc = Tc {
                commit_hash: CommitHash::new("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                data: None,
                ..Default::default()
            };
            sink.write_tc(&tc, "quality").expect("write");
        }
        assert!(buf.is_empty(), "nil Data should produce no output");
    }

    // TestStreamingSink_WriteTC_MultipleLines (streaming_sink_test.go:68).
    #[test]
    fn write_tc_multiple_lines() {
        let mut buf = Vec::new();
        {
            let sink = StreamingSink::new(&mut buf);
            for i in 0..3 {
                let tc = Tc {
                    commit_hash: CommitHash::new("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                    tick: i,
                    timestamp: Some(ts(1_700_000_000 + i64::from(i) * 86_400)),
                    data: Some(go_map(&[("val", GoValue::Int(i64::from(i)))])),
                    ..Default::default()
                };
                sink.write_tc(&tc, "quality").expect("write");
            }
        }
        let s = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = s.trim_end().split('\n').collect();
        assert_eq!(lines.len(), 3);
    }

    // TestStreamingSink_WriteTC_ConcurrentWrites (streaming_sink_test.go:99).
    #[test]
    fn write_tc_concurrent_writes() {
        let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
        // Adapter so the sink can own a clonable writer handle.
        struct SharedWriter(Arc<Mutex<Vec<u8>>>);
        impl Write for SharedWriter {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let sink = Arc::new(StreamingSink::new(SharedWriter(buf.clone())));

        const GOROUTINES: i32 = 10;
        let mut handles = Vec::new();
        for id in 0..GOROUTINES {
            let sink = sink.clone();
            handles.push(std::thread::spawn(move || {
                let tc = Tc {
                    commit_hash: CommitHash::new("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                    tick: id,
                    timestamp: Some(ts(1_700_000_000)),
                    data: Some(go_map(&[("id", GoValue::Int(i64::from(id)))])),
                    ..Default::default()
                };
                sink.write_tc(&tc, "quality").expect("write");
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let bytes = buf.lock().unwrap();
        let s = String::from_utf8(bytes.clone()).unwrap();
        let lines: Vec<&str> = s.trim_end().split('\n').collect();
        assert_eq!(lines.len(), GOROUTINES as usize);
    }

    // TestStreamingSink_WriteTC_WriterError (streaming_sink_test.go:151).
    #[test]
    fn write_tc_writer_error() {
        struct ErrWriter;
        impl Write for ErrWriter {
            fn write(&mut self, _b: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "broken pipe",
                ))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let sink = StreamingSink::new(ErrWriter);
        let tc = Tc {
            commit_hash: CommitHash::new("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            data: Some(go_map(&[("val", GoValue::Int(1))])),
            ..Default::default()
        };
        assert!(sink.write_tc(&tc, "quality").is_err());
    }

    // TestStreamingSink_WriteTC_ZeroTimestamp (streaming_sink_test.go:165).
    #[test]
    fn write_tc_zero_timestamp() {
        let mut buf = Vec::new();
        {
            let sink = StreamingSink::new(&mut buf);
            let tc = Tc {
                commit_hash: CommitHash::new("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                data: Some(go_map(&[("val", GoValue::Int(1))])),
                ..Default::default()
            };
            sink.write_tc(&tc, "quality").expect("write");
        }
        let s = String::from_utf8(buf).unwrap();
        assert!(
            s.contains("\"timestamp\":\"\""),
            "zero timestamp should produce empty string, got: {s}"
        );
    }
}
