//! Deferred typed-slice → `[]map[string]any` conversion at the serialization
//! boundary.
//!
//! Per-file analyzers
//! place a [`TypedCollection`] in the report instead of a materialized map slice;
//! conversion is deferred to the serialization boundary. In Rust the "typed
//! slice + reflective converter" is expressed with a boxed converter closure that
//! yields [`cf_gojson::GoMap`]s (the same value model the encoder consumes).

use cf_gojson::GoMap;

/// Report key stamping the originating source file.
pub const SOURCE_FILE_KEY: &str = "_source_file";
/// Report key stamping the detected language.
pub const LANGUAGE_KEY: &str = "_language";
/// Report key stamping the parent directory.
pub const DIRECTORY_KEY: &str = "_directory";

/// Converts deferred typed items into report maps.
///
/// A deferred item-to-maps converter. It receives the stamped source
/// file and produces the flattened maps; when `source_file` is non-empty the
/// converter should include it as [`SOURCE_FILE_KEY`] in each output map.
pub type ItemConverter = Box<dyn Fn(&str) -> Vec<GoMap> + Send + Sync>;

/// Wraps a typed struct slice for deferred map conversion.
///
/// The reference stores the concrete
/// typed slice in `Items any`; in Rust the (already type-erased) items are
/// captured inside the [`ItemConverter`] closure, so this struct carries only
/// the stamped metadata plus the converter.
pub struct TypedCollection {
    /// Stamped source file (`SourceFile`).
    pub source_file: String,
    /// Stamped detected language (`Language`).
    pub language: String,
    /// Stamped parent directory of the source file (`Directory`).
    pub directory: String,
    /// Deferred converter; `None` means no converter is set.
    pub to_maps: Option<ItemConverter>,
}

impl TypedCollection {
    /// Converts the typed items to report maps using the stored converter.
    ///
    /// Returns an empty vector when no converter is set.
    #[must_use]
    pub fn map_slice(&self) -> Vec<GoMap> {
        match &self.to_maps {
            Some(convert) => convert(&self.source_file),
            None => Vec::new(),
        }
    }
}

impl std::fmt::Debug for TypedCollection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypedCollection")
            .field("source_file", &self.source_file)
            .field("language", &self.language)
            .field("directory", &self.directory)
            .field("to_maps", &self.to_maps.as_ref().map(|_| "<fn>"))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_gojson::{GoValue, MapOrigin};

    #[test]
    fn keys_match_reference() {
        assert_eq!(SOURCE_FILE_KEY, "_source_file");
        assert_eq!(LANGUAGE_KEY, "_language");
        assert_eq!(DIRECTORY_KEY, "_directory");
    }

    #[test]
    fn map_slice_nil_converter_yields_empty() {
        let tc = TypedCollection {
            source_file: "a.rs".into(),
            language: "rust".into(),
            directory: ".".into(),
            to_maps: None,
        };
        assert!(tc.map_slice().is_empty());
    }

    #[test]
    fn map_slice_invokes_converter_with_source_file() {
        let tc = TypedCollection {
            source_file: "a.rs".into(),
            language: "rust".into(),
            directory: ".".into(),
            to_maps: Some(Box::new(|src| {
                let mut m = GoMap::new(MapOrigin::Map);
                m.insert(SOURCE_FILE_KEY, GoValue::Str(src.to_string()));
                vec![m]
            })),
        };
        let maps = tc.map_slice();
        assert_eq!(maps.len(), 1);
    }
}
