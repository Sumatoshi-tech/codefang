//! Values spilled to disk: finished results a run no longer needs in memory
//! until a later consumer reads them back.
//!
//! A spill is an unlinked temporary file in the system temp directory
//! (`TMPDIR`): it has no name, so the space is reclaimed when the process
//! exits however it exits. The encoding is the workspace's internal binary
//! codec (`cf_persist::GobCodec`, bincode), which keeps every float's bits.

use std::fs::File;
use std::io::{BufReader, BufWriter, Seek, SeekFrom, Write};
use std::marker::PhantomData;
use std::sync::{Mutex, PoisonError};

use cf_persist::{Codec, GobCodec};
use serde::de::DeserializeOwned;
use serde::Serialize;

/// Why a value could not be spilled or read back.
#[derive(Debug, thiserror::Error)]
#[error("{op}: {cause}")]
pub(crate) struct SpillError {
    /// What was being done.
    op: &'static str,
    /// The underlying failure.
    cause: String,
}

impl SpillError {
    fn new(op: &'static str, cause: impl std::fmt::Display) -> Self {
        Self {
            op,
            cause: cause.to_string(),
        }
    }
}

/// A value of type `T` held on disk.
pub(crate) struct Spilled<T> {
    /// The unlinked file; reads seek it, so one reader at a time.
    file: Mutex<File>,
    /// Encoded size.
    bytes: u64,
    value: PhantomData<fn() -> T>,
}

impl<T: Serialize + DeserializeOwned + Default> Spilled<T> {
    /// Encodes `value` into a new unlinked temporary file.
    ///
    /// # Errors
    ///
    /// A [`SpillError`] when the file cannot be created or written (for
    /// example a full disk).
    pub(crate) fn write(value: &T) -> Result<Self, SpillError> {
        let mut file =
            tempfile::tempfile().map_err(|e| SpillError::new("create a spill file", e))?;
        let mut writer = BufWriter::new(&mut file);
        GobCodec::new()
            .encode(&mut writer, value)
            .map_err(|e| SpillError::new("write a spill file", e))?;
        writer
            .flush()
            .map_err(|e| SpillError::new("write a spill file", e))?;
        drop(writer);
        let bytes = file
            .stream_position()
            .map_err(|e| SpillError::new("size a spill file", e))?;
        Ok(Self {
            file: Mutex::new(file),
            bytes,
            value: PhantomData,
        })
    }

    /// Decodes the whole value again.
    ///
    /// # Errors
    ///
    /// A [`SpillError`] when the file cannot be read or decoded.
    pub(crate) fn read(&self) -> Result<T, SpillError> {
        let mut file = self.file.lock().unwrap_or_else(PoisonError::into_inner);
        file.seek(SeekFrom::Start(0))
            .map_err(|e| SpillError::new("rewind a spill file", e))?;
        let mut value = T::default();
        GobCodec::new()
            .decode(BufReader::new(&mut *file), &mut value)
            .map_err(|e| SpillError::new("read a spill file", e))?;
        Ok(value)
    }

    /// The encoded size, in bytes.
    pub(crate) const fn bytes(&self) -> u64 {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    struct Sample {
        name: String,
        values: Vec<f64>,
        nested: BTreeMap<String, Vec<i64>>,
    }

    /// Floats whose bits must survive the round trip.
    const EDGE_FLOATS: [f64; 6] = [0.0, -0.0, 1e-300, 5e-324, f64::MAX, 0.1];

    fn sample() -> Sample {
        let mut nested = BTreeMap::new();
        nested.insert("b".to_owned(), vec![-3, 7]);
        nested.insert("a".to_owned(), Vec::new());
        Sample {
            name: "kubernetes".to_owned(),
            values: EDGE_FLOATS.to_vec(),
            nested,
        }
    }

    /// A spilled value reads back unchanged, every float to the bit.
    #[test]
    fn a_spilled_value_reads_back_unchanged() {
        let spilled = Spilled::write(&sample()).expect("spill");
        let back = spilled.read().expect("read");
        assert_eq!(back, sample());
        let bits = |v: &[f64]| v.iter().map(|f| f.to_bits()).collect::<Vec<_>>();
        assert_eq!(bits(&back.values), bits(&EDGE_FLOATS));
    }

    /// Every read returns the whole value again.
    #[test]
    fn a_spilled_value_reads_back_any_number_of_times() {
        let spilled = Spilled::write(&sample()).expect("spill");
        for _ in 0..3 {
            assert_eq!(spilled.read().expect("read"), sample());
        }
    }

    /// Readers on several threads each get the whole value.
    #[test]
    fn concurrent_reads_each_get_the_whole_value() {
        let spilled = Spilled::write(&sample()).expect("spill");
        std::thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| assert_eq!(spilled.read().expect("read"), sample()));
            }
        });
    }

    /// The spill reports how many bytes it wrote.
    #[test]
    fn the_spill_reports_its_size() {
        let spilled = Spilled::write(&sample()).expect("spill");
        assert!(spilled.bytes() > 0);
        let empty = Spilled::write(&Vec::<u8>::new()).expect("spill");
        assert!(spilled.bytes() > empty.bytes());
    }
}
