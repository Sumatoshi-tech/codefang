//! `IdentityDetector`: maps a commit author to a canonical person id.
//!
//! Person ids and the reversed people dictionary flow into report output
//! (developer names, `author_id` fields, people matrices). Two matching modes:
//! * **loose** (default): an author is identified by *either* their lowercased
//!   email or lowercased name; the two are unified into one id. A hit on one
//!   token REGISTERS the other, so `alice <a@x>` followed by `Alice Smith
//!   <a@x>` followed by `Alice Smith <a2@y>` is one person — every alias seen
//!   together with a known token joins that person;
//! * **exact**: the whole `"name <email>"` signature (lowercased) is the key.
//!
//! The people dictionary can be loaded from a file or built incrementally as
//! commits are consumed. When the dictionary is finalized (loaded ahead of
//! time) an unknown author yields [`AUTHOR_MISSING`].

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

pub use cf_identity::{AUTHOR_MISSING, AUTHOR_MISSING_NAME};

/// An author signature — the commit metadata the detector reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    /// Author display name.
    pub name: String,
    /// Author email.
    pub email: String,
    /// Authoring time as a UTC unix timestamp in seconds.
    pub when_unix: i64,
}

/// The [`AUTHOR_MISSING`] sentinel as the `i64` the detector hands out.
#[must_use]
pub const fn author_missing_id() -> i64 {
    AUTHOR_MISSING as i64
}

/// `IdentityDetector` provider.
#[derive(Debug, Clone, Default)]
pub struct IdentityDetector {
    /// Identity-string -> person id. Keys are lowercased.
    pub people_dict: HashMap<String, i64>,
    /// One display entry per person id.
    pub reversed_people_dict: Vec<String>,
    /// Person id of the last consumed commit.
    pub author_id: i64,
    /// Disable separate name/email matching (match whole signatures).
    pub exact_signatures: bool,

    // Incremental-build state.
    incremental_emails: HashMap<i64, Vec<String>>,
    incremental_names: HashMap<i64, Vec<String>>,
    incremental_size: i64,
    dict_finalized: bool,
}

impl IdentityDetector {
    /// Construct an empty detector in incremental (non-finalized) mode.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct from a preset dictionary (treated as finalized).
    #[must_use]
    pub fn from_dict(people_dict: HashMap<String, i64>, reversed_people_dict: Vec<String>) -> Self {
        Self {
            people_dict,
            reversed_people_dict,
            dict_finalized: true,
            ..Default::default()
        }
    }

    /// Resolve the author id for a commit.
    ///
    /// Updates [`author_id`](IdentityDetector::author_id) and returns it.
    pub fn consume_signature(&mut self, signature: &Signature) -> i64 {
        let (author_id, exists) = if self.exact_signatures {
            self.lookup_exact_signature(signature)
        } else {
            self.lookup_loose_signature(signature)
        };
        let author_id = if !exists && self.dict_finalized {
            author_missing_id()
        } else {
            author_id
        };
        self.author_id = author_id;
        author_id
    }

    /// Find or register an author by exact signature.
    fn lookup_exact_signature(&mut self, signature: &Signature) -> (i64, bool) {
        let sig = format!("{} <{}>", signature.name, signature.email).to_lowercase();
        if let Some(&id) = self.people_dict.get(&sig) {
            return (id, true);
        }
        if !self.dict_finalized {
            let id = self.incremental_size;
            self.people_dict.insert(sig, id);
            self.incremental_size += 1;
            return (id, false);
        }
        (0, false)
    }

    /// Find or register an author by loose signature. A hit on either token
    /// registers the other token under the same id (incremental mode), so a
    /// later commit carrying only the newly seen token still resolves to this
    /// person.
    fn lookup_loose_signature(&mut self, signature: &Signature) -> (i64, bool) {
        let email = signature.email.to_lowercase();
        let name = signature.name.to_lowercase();

        let hit = self
            .people_dict
            .get(&email)
            .or_else(|| self.people_dict.get(&name))
            .copied();
        if let Some(id) = hit {
            if !self.dict_finalized {
                self.register_loose_identity(&email, &name);
            }
            return (id, true);
        }
        if !self.dict_finalized {
            self.register_loose_identity(&email, &name);
            let id = self.people_dict.get(&email).copied().unwrap_or(0);
            return (id, false);
        }
        (0, false)
    }

    /// Register an email/name pair, unifying them under one id.
    fn register_loose_identity(&mut self, email: &str, name: &str) {
        if let Some(&id) = self.people_dict.get(email) {
            if !self.people_dict.contains_key(name) {
                self.people_dict.insert(name.to_string(), id);
                self.incremental_names
                    .entry(id)
                    .or_default()
                    .push(name.to_string());
            }
            return;
        }
        if let Some(&id) = self.people_dict.get(name) {
            self.people_dict.insert(email.to_string(), id);
            self.incremental_emails
                .entry(id)
                .or_default()
                .push(email.to_string());
            return;
        }
        let id = self.incremental_size;
        self.people_dict.insert(email.to_string(), id);
        self.people_dict.insert(name.to_string(), id);
        self.incremental_emails
            .entry(id)
            .or_default()
            .push(email.to_string());
        self.incremental_names
            .entry(id)
            .or_default()
            .push(name.to_string());
        self.incremental_size += 1;
    }

    /// Load a people dictionary from a file.
    ///
    /// Each non-empty line lists `|`-separated identity tokens for one person;
    /// every token (lowercased) maps to that person's index, and the first
    /// token is the reverse-dict entry. A trailing [`AUTHOR_MISSING_NAME`]
    /// entry is appended so the sentinel id has a display name.
    ///
    /// # Errors
    ///
    /// Returns the I/O error when the file cannot be opened or read.
    pub fn load_people_dict<P: AsRef<Path>>(&mut self, path: P) -> std::io::Result<()> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut dict: HashMap<String, i64> = HashMap::new();
        let mut reverse: Vec<String> = Vec::new();
        for (size, line) in reader.lines().enumerate() {
            let line = line?;
            let size = size as i64;
            // Lines are processed verbatim: only the trailing newline is
            // stripped, other whitespace is NOT trimmed, and blank lines are
            // processed too. Split on '|' directly.
            let ids: Vec<&str> = line.split('|').collect();
            for id in &ids {
                dict.insert(id.to_lowercase(), size);
            }
            reverse.push(ids[0].to_string());
        }
        reverse.push(AUTHOR_MISSING_NAME.to_string());
        self.people_dict = dict;
        self.reversed_people_dict = reverse;
        self.dict_finalized = true;
        Ok(())
    }

    /// Build [`reversed_people_dict`](IdentityDetector::reversed_people_dict)
    /// from the incrementally-collected names/emails.
    ///
    /// No-op when the dictionary is already finalized (e.g. loaded from a file
    /// or preset). For loose matching the reverse entry is
    /// `sorted(names).join("|") + "|" + sorted(emails).join("|")`; for exact
    /// matching it is the signature key itself.
    pub fn finalize_dict(&mut self) {
        if self.dict_finalized {
            return;
        }
        let size = self.incremental_size;
        let mut reverse = vec![String::new(); size.max(0) as usize];
        if self.exact_signatures {
            for (key, &val) in &self.people_dict {
                if val >= 0 && (val as usize) < reverse.len() {
                    reverse[val as usize].clone_from(key);
                }
            }
        } else {
            for id in 0..size {
                let mut names = self.incremental_names.get(&id).cloned().unwrap_or_default();
                let mut emails = self
                    .incremental_emails
                    .get(&id)
                    .cloned()
                    .unwrap_or_default();
                names.sort();
                emails.sort();
                reverse[id as usize] = format!("{}|{}", names.join("|"), emails.join("|"));
            }
        }
        self.reversed_people_dict = reverse;
        self.dict_finalized = true;
    }

    /// Author id of the last consumed commit.
    #[must_use]
    pub const fn get_author_id(&self) -> i64 {
        self.author_id
    }
}

/// Prefix of every anonymized developer label.
const ANONYMOUS_PREFIX: &str = "Developer-";

/// Replaces every reversed-dictionary entry with a stable pseudonym derived
/// from its index: `Developer-A`, `Developer-B`, …, `Developer-Z`,
/// `Developer-AA`, … (the `--anonymize` output policy). The
/// [`AUTHOR_MISSING_NAME`] sentinel is kept as-is so an unmatched author
/// stays recognisable.
#[must_use]
pub fn anonymize_names(names: &[String]) -> Vec<String> {
    names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            if name == AUTHOR_MISSING_NAME {
                name.clone()
            } else {
                format!("{ANONYMOUS_PREFIX}{}", column_label(i))
            }
        })
        .collect()
}

/// Spreadsheet-style column label for a 0-based index (`0 → A`, `25 → Z`,
/// `26 → AA`).
fn column_label(index: usize) -> String {
    const ALPHABET: usize = 26;
    let mut n = index + 1;
    let mut out = Vec::new();
    while n > 0 {
        n -= 1;
        out.push(b'A' + (n % ALPHABET) as u8);
        n /= ALPHABET;
    }
    out.reverse();
    String::from_utf8(out).expect("ASCII letters")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn sig(name: &str, email: &str) -> Signature {
        Signature {
            name: name.to_string(),
            email: email.to_string(),
            when_unix: 0,
        }
    }

    #[test]
    fn loose_unifies_name_and_email() {
        let mut d = IdentityDetector::new();
        let id1 = d.consume_signature(&sig("John Doe", "john@example.com"));
        // Same email, different display name -> same id.
        let id2 = d.consume_signature(&sig("J. Doe", "john@example.com"));
        assert_eq!(id1, id2);
        // Different person.
        let id3 = d.consume_signature(&sig("Jane", "jane@example.com"));
        assert_ne!(id1, id3);
    }

    /// A hit on one token must register the OTHER token: after `alice
    /// <alice@x>` and `Alice Smith <alice@x>` (email hit), a commit by `Alice
    /// Smith <alice@y>` (name hit only) is still the same person.
    #[test]
    fn loose_hit_registers_the_other_token() {
        let mut d = IdentityDetector::new();
        let a = d.consume_signature(&sig("alice", "alice@example.com"));
        let b = d.consume_signature(&sig("Alice Smith", "alice@example.com"));
        let c = d.consume_signature(&sig("Alice Smith", "alice@other.org"));
        assert_eq!((a, b, c), (0, 0, 0), "all three aliases are one person");
        d.finalize_dict();
        assert_eq!(
            d.reversed_people_dict,
            vec!["alice|alice smith|alice@example.com|alice@other.org".to_string()],
            "the reverse entry lists every alias seen"
        );
    }

    #[test]
    fn exact_distinguishes_full_signature() {
        let mut d = IdentityDetector::new();
        d.exact_signatures = true;
        let id1 = d.consume_signature(&sig("John Doe", "john@example.com"));
        let id2 = d.consume_signature(&sig("J. Doe", "john@example.com"));
        // Exact mode treats the differing display name as a new identity.
        assert_ne!(id1, id2);
        d.finalize_dict();
        assert_eq!(
            d.reversed_people_dict,
            vec![
                "john doe <john@example.com>".to_string(),
                "j. doe <john@example.com>".to_string()
            ]
        );
    }

    #[test]
    fn finalized_unknown_author_is_missing() {
        let mut dict = HashMap::new();
        dict.insert("john@example.com".to_string(), 0);
        let mut d = IdentityDetector::from_dict(dict, vec!["John".into()]);
        assert_eq!(d.consume_signature(&sig("John", "john@example.com")), 0);
        assert_eq!(
            d.consume_signature(&sig("Nobody", "nobody@example.com")),
            author_missing_id()
        );
        assert_eq!(author_missing_id(), i64::from(cf_identity::AUTHOR_MISSING));
    }

    #[test]
    fn load_people_dict_appends_unknown_sentinel() {
        let content = "John Doe|john@example.com\nJane Smith|jane@example.com\n";
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(content.as_bytes()).unwrap();
        let mut d = IdentityDetector::new();
        d.load_people_dict(tmp.path()).unwrap();
        // Two persons * two tokens each = 4 dict entries.
        assert_eq!(d.people_dict.len(), 4);
        // reversed = [first-token-of-line-0, first-token-of-line-1, "<unmatched>"].
        assert_eq!(d.reversed_people_dict.len(), 3);
        assert_eq!(d.reversed_people_dict[2], AUTHOR_MISSING_NAME);
        // Tokens lowercased on load; a loaded dict never grows.
        assert_eq!(d.consume_signature(&sig("john doe", "john@example.com")), 0);
        assert_eq!(d.consume_signature(&sig("Jane Smith", "x@y")), 1);
        assert_eq!(
            d.consume_signature(&sig("Stranger", "s@z")),
            author_missing_id()
        );
        assert_eq!(d.people_dict.len(), 4);
    }

    #[test]
    fn load_people_dict_missing_file_is_an_error() {
        let mut d = IdentityDetector::new();
        let err = d
            .load_people_dict("/nonexistent/people.txt")
            .expect_err("missing file must error");
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn anonymize_names_numbers_developers_and_keeps_the_sentinel() {
        let names: Vec<String> = (0..27)
            .map(|i| format!("dev{i}|dev{i}@x"))
            .chain(std::iter::once(AUTHOR_MISSING_NAME.to_string()))
            .collect();
        let anon = anonymize_names(&names);
        assert_eq!(anon[0], "Developer-A");
        assert_eq!(anon[25], "Developer-Z");
        assert_eq!(anon[26], "Developer-AA");
        assert_eq!(anon[27], AUTHOR_MISSING_NAME);
    }
}
