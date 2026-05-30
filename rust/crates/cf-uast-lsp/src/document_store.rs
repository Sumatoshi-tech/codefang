//! Thread-safe in-memory store of open document contents, keyed by URI.
//!
//! Port of the Go `DocumentStore` (`pkg/uast/lsp/server.go`). The Go type guards
//! a `map[string]string` with a `sync.RWMutex`. Here we use a [`std::sync::RwLock`]
//! around a [`HashMap`], preserving the same `Set` / `Get` / `Delete` semantics
//! (last write wins, `Get` returns presence as a boolean).

use std::collections::HashMap;
use std::sync::RwLock;

/// A thread-safe store for document contents keyed by URI.
///
/// Mirrors Go `lsp.DocumentStore`: a URI → content map behind a read/write lock.
/// Multiple readers may hold the lock concurrently; writers are exclusive. All
/// methods take `&self` so the store can be shared (e.g. inside an `Arc`) across
/// the async LSP handler tasks.
#[derive(Debug, Default)]
pub struct DocumentStore {
    /// URI -> content.
    documents: RwLock<HashMap<String, String>>,
}

impl DocumentStore {
    /// Creates a new, empty [`DocumentStore`].
    ///
    /// Equivalent to Go `NewDocumentStore()`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            documents: RwLock::new(HashMap::new()),
        }
    }

    /// Stores `content` for the given `uri`, overwriting any previous value.
    ///
    /// Equivalent to Go `(*DocumentStore).Set`.
    pub fn set(&self, uri: impl Into<String>, content: impl Into<String>) {
        // A poisoned lock can only happen if another thread panicked while
        // holding it; we recover the guard so a single failed handler does not
        // wedge the whole server (Go's mutex has no poisoning concept).
        let mut map = self.documents.write().unwrap_or_else(|e| e.into_inner());
        map.insert(uri.into(), content.into());
    }

    /// Retrieves the content stored for `uri`, if any.
    ///
    /// Returns the content together with a presence flag, mirroring Go's
    /// `content, ok := ds.documents[uri]` two-value return.
    #[must_use]
    pub fn get(&self, uri: &str) -> Option<String> {
        let map = self.documents.read().unwrap_or_else(|e| e.into_inner());
        map.get(uri).cloned()
    }

    /// Returns `true` if a document is stored for `uri`.
    #[must_use]
    pub fn contains(&self, uri: &str) -> bool {
        let map = self.documents.read().unwrap_or_else(|e| e.into_inner());
        map.contains_key(uri)
    }

    /// Removes the document stored for `uri`. A no-op if it is absent.
    ///
    /// Equivalent to Go `(*DocumentStore).Delete`.
    pub fn delete(&self, uri: &str) {
        let mut map = self.documents.write().unwrap_or_else(|e| e.into_inner());
        map.remove(uri);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_DOCUMENT_URI: &str = "file:///test.uastmap";

    /// Ported from Go `TestNewDocumentStore`.
    #[test]
    fn test_new_document_store() {
        let store = DocumentStore::new();
        // A freshly created store has no documents (Go asserts the inner map is
        // non-nil; in Rust the map is always present, so we assert it is empty).
        assert!(store.get(TEST_DOCUMENT_URI).is_none());
    }

    /// Ported from Go `TestDocumentStore_SetAndGet`.
    #[test]
    fn test_document_store_set_and_get() {
        let store = DocumentStore::new();
        let uri = TEST_DOCUMENT_URI;
        let content = "test content";

        store.set(uri, content);

        let got = store.get(uri);
        assert!(got.is_some(), "Expected document to exist for URI {uri}");
        assert_eq!(got.unwrap(), content);
    }

    /// Ported from Go `TestDocumentStore_Get_NotFound`.
    #[test]
    fn test_document_store_get_not_found() {
        let store = DocumentStore::new();
        assert!(
            store.get("file:///nonexistent.uastmap").is_none(),
            "Expected document to not exist"
        );
    }

    /// Ported from Go `TestDocumentStore_Delete`.
    #[test]
    fn test_document_store_delete() {
        let store = DocumentStore::new();
        let uri = TEST_DOCUMENT_URI;

        store.set(uri, "test content");
        store.delete(uri);

        assert!(store.get(uri).is_none(), "Expected document to be deleted");
    }

    /// Ported from Go `TestDocumentStore_Update`.
    #[test]
    fn test_document_store_update() {
        let store = DocumentStore::new();
        let uri = TEST_DOCUMENT_URI;

        store.set(uri, "initial content");
        store.set(uri, "updated content");

        let got = store.get(uri);
        assert!(got.is_some(), "Expected document to exist for URI {uri}");
        assert_eq!(got.unwrap(), "updated content");
    }

    /// Ported from Go `TestDocumentStore_ConcurrentAccess`.
    ///
    /// Spawns concurrent writers and readers against two URIs and verifies the
    /// final state, exercising the [`RwLock`] under contention (the Rust analog
    /// of Go's goroutine fan-out).
    #[test]
    fn test_document_store_concurrent_access() {
        use std::sync::Arc;
        use std::thread;

        let store = Arc::new(DocumentStore::new());

        let mut handles = Vec::new();

        // Concurrent writes to test1.
        {
            let s = Arc::clone(&store);
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    s.set("file:///test1.uastmap", "content1");
                }
            }));
        }
        // Concurrent writes to test2.
        {
            let s = Arc::clone(&store);
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    s.set("file:///test2.uastmap", "content2");
                }
            }));
        }
        // Concurrent reads of test1.
        {
            let s = Arc::clone(&store);
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    let _ = s.get("file:///test1.uastmap");
                }
            }));
        }
        // Concurrent reads of test2.
        {
            let s = Arc::clone(&store);
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    let _ = s.get("file:///test2.uastmap");
                }
            }));
        }

        for h in handles {
            h.join().expect("worker thread panicked");
        }

        assert_eq!(
            store.get("file:///test1.uastmap").as_deref(),
            Some("content1"),
            "Expected test1.uastmap to have content1"
        );
        assert_eq!(
            store.get("file:///test2.uastmap").as_deref(),
            Some("content2"),
            "Expected test2.uastmap to have content2"
        );
    }
}
