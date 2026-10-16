//! The cross-file [`Aggregator`]: collects every file's candidates, qualifies
//! their names with the source file (`path::name`), builds one global LSH
//! index, runs the engine, and produces the [`CloneReport`].

use cf_alg_lsh::Index;

use crate::engine::{find_clone_pairs, IndexedFunc};
use crate::report::{CloneReport, FileSummary};
use crate::visitor::FileScan;
use crate::{
    clone_message, compute_clone_ratio, DEFAULT_MAX_CLONE_PAIRS, MSG_NO_FUNCTIONS, NUM_BANDS,
    NUM_ROWS, SIMILARITY_THRESHOLD,
};

/// Separator between a source file path and a function name.
pub const FILE_NAME_SEPARATOR: &str = "::";

/// Collects per-file candidates and performs global cross-file clone
/// detection.
#[derive(Debug, Clone)]
pub struct Aggregator {
    funcs: Vec<IndexedFunc>,
    files: Vec<FileSummary>,
    total_functions: i64,
    /// Caps the stored clone pairs in the report detail (`0` = unlimited).
    /// The pair count, distribution and ratio stay exact.
    pub max_clone_pairs: usize,
    /// Minimum exact Jaccard for a structurally different pair (Type-3).
    pub similarity_threshold: f64,
}

impl Default for Aggregator {
    fn default() -> Self {
        Self::new()
    }
}

impl Aggregator {
    /// Creates an aggregator with the default cap and threshold.
    #[must_use]
    pub fn new() -> Self {
        Self {
            funcs: Vec::new(),
            files: Vec::new(),
            total_functions: 0,
            max_clone_pairs: DEFAULT_MAX_CLONE_PAIRS,
            similarity_threshold: SIMILARITY_THRESHOLD,
        }
    }

    /// Folds one file's scan into the aggregate. `path` (relative to the
    /// analyzed root) qualifies every candidate name as `path::name`.
    pub fn add_file(&mut self, path: &str, scan: FileScan) {
        let file = self.files.len();
        let base = self.funcs.len();
        self.total_functions += scan.function_count as i64;
        self.files.push(FileSummary {
            path: path.to_string(),
            total_functions: scan.function_count as i64,
            ..FileSummary::default()
        });
        for entry in scan.entries {
            let mut func = entry.into_indexed(file, base);
            func.name = qualify_func_name(&func.name, path);
            self.funcs.push(func);
        }
    }

    /// Number of candidates collected so far.
    #[must_use]
    pub fn candidate_count(&self) -> usize {
        self.funcs.len()
    }

    /// Builds the global LSH index, finds every clone pair, and emits the
    /// report.
    ///
    /// # Panics
    ///
    /// Never in practice: [`NUM_BANDS`] × [`NUM_ROWS`] equals the signature
    /// length every candidate is built with (asserted by the crate's tests),
    /// so the index construction and inserts cannot fail.
    #[must_use]
    pub fn finish(&self) -> CloneReport {
        let mut files = self.files.clone();
        if self.total_functions == 0 {
            return CloneReport {
                message: MSG_NO_FUNCTIONS.to_string(),
                files,
                ..CloneReport::default()
            };
        }

        let mut index = Index::new(NUM_BANDS, NUM_ROWS).expect("positive LSH parameters");
        for (i, f) in self.funcs.iter().enumerate() {
            index
                .insert(i, &f.sig)
                .expect("signature length matches the LSH parameters");
        }
        let result = find_clone_pairs(
            &self.funcs,
            &index,
            self.max_clone_pairs,
            self.similarity_threshold,
        );

        for (i, f) in self.funcs.iter().enumerate() {
            if result.cloned[i] {
                files[f.file].cloned_functions += 1;
            }
        }
        for (pair, &(fa, fb)) in result.pair_files.iter().enumerate() {
            files[fa].pairs.push(pair);
            if fb != fa {
                files[fb].pairs.push(pair);
            }
        }

        let cloned = result.cloned.iter().filter(|c| **c).count();
        let clone_ratio = compute_clone_ratio(cloned, self.total_functions as usize);
        CloneReport {
            total_functions: self.total_functions,
            total_clone_pairs: result.total_count as i64,
            clone_ratio,
            distribution: result.distribution,
            pairs: result.pairs,
            message: clone_message(clone_ratio).to_string(),
            files,
        }
    }
}

/// Returns `path::name` if `path` is non-empty, else `name`.
#[must_use]
fn qualify_func_name(name: &str, path: &str) -> String {
    if path.is_empty() {
        name.to_string()
    } else {
        format!("{path}{FILE_NAME_SEPARATOR}{name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::CloneType;
    use crate::visitor::Visitor;
    use cf_uast::Parser;

    const PROCESS_ITEMS: &str = r#"package main

import "fmt"

func ProcessItems(items []int) int {
	total := 0
	for i, v := range items {
		if v%2 == 0 {
			total += v * i
		} else {
			total -= v
		}
	}
	fmt.Println("total", total)
	return total
}
"#;

    /// Every identifier and literal of [`PROCESS_ITEMS`] renamed.
    const SUM_VALUES: &str = r#"package main

import "fmt"

func SumValues(values []int) int {
	acc := 0
	for idx, val := range values {
		if val%2 == 0 {
			acc += val * idx
		} else {
			acc -= val
		}
	}
	fmt.Println("acc", acc)
	return acc
}
"#;

    /// [`PROCESS_ITEMS`] with one statement and one call argument added.
    const PROCESS_ITEMS_PLUS: &str = r#"package main

import "fmt"

func ProcessItemsPlus(items []int) int {
	total := 0
	count := len(items)
	for i, v := range items {
		if v%2 == 0 {
			total += v * i
		} else {
			total -= v
		}
	}
	fmt.Println("total", total, count)
	return total
}
"#;

    /// [`PROCESS_ITEMS`] with a comment added and reformatted.
    const PROCESS_ITEMS_COMMENTED: &str = r#"package main

import "fmt"

// ProcessItems folds the items.
func ProcessItems(items []int) int {
	total := 0 // running sum
	for i, v := range items {
		if v%2 == 0 {
			total += v * i
		} else {
			total -= v
		}
	}
	fmt.Println("total", total)
	return total
}
"#;

    const DESCRIBE: &str = r#"package main

import "strings"

type Server struct {
	name string
	tags map[string][]string
}

func (s *Server) Describe(prefix string) string {
	var b strings.Builder
	b.WriteString(prefix)
	b.WriteString(": ")
	b.WriteString(s.name)
	for k, vs := range s.tags {
		b.WriteString(" ")
		b.WriteString(k)
		b.WriteString("=")
		b.WriteString(strings.Join(vs, ","))
	}
	return strings.ToUpper(b.String())
}
"#;

    const TINY_ADD: &str = "package main\n\nfunc Add(a, b int) int {\n\treturn a + b\n}\n";

    /// Exact 5-shingle Jaccard of `PROCESS_ITEMS` vs `PROCESS_ITEMS_PLUS`.
    /// Exact 5-shingle Jaccard of `PROCESS_ITEMS` vs `PROCESS_ITEMS_PLUS`:
    /// the shared `for`/`if`/`else` body contributes the common shingles,
    /// the extra `count` statement and call argument the differing ones.
    /// (41/64 before the `else_clause` wrapper was collapsed in the Go
    /// mapping; one shared node fewer removes one common and one union
    /// shingle.)
    const T3_EXPECTED: f64 = 40.0 / 63.0;

    fn report(files: &[(&str, &str)]) -> CloneReport {
        let parser = Parser::new();
        let mut agg = Aggregator::new();
        for (path, src) in files {
            let root = parser.parse(path, src.as_bytes()).expect("parses");
            agg.add_file(path, Visitor::new().scan(&root));
        }
        agg.finish()
    }

    fn only_pair(r: &CloneReport) -> &crate::ClonePair {
        assert_eq!(r.pairs.len(), 1, "{:?}", r.pairs);
        assert_eq!(r.total_clone_pairs, 1);
        &r.pairs[0]
    }

    #[test]
    fn qualify_with_and_without_path() {
        assert_eq!(qualify_func_name("Foo", ""), "Foo");
        assert_eq!(qualify_func_name("Foo", "a.go"), "a.go::Foo");
    }

    #[test]
    fn no_functions_returns_the_empty_report() {
        let r = report(&[("a.go", "package main\n")]);
        assert_eq!(r.message, MSG_NO_FUNCTIONS);
        assert_eq!(r.total_functions, 0);
        assert_eq!(r.files.len(), 1);
    }

    #[test]
    fn identical_pair_is_type1_with_similarity_one() {
        let r = report(&[("a.go", PROCESS_ITEMS), ("b.go", PROCESS_ITEMS)]);
        let p = only_pair(&r);
        assert_eq!(p.func_a, "a.go::ProcessItems");
        assert_eq!(p.func_b, "b.go::ProcessItems");
        assert_eq!(p.clone_type, CloneType::Type1);
        assert!((p.similarity - 1.0).abs() < 1e-12);
        assert!((r.clone_ratio - 1.0).abs() < 1e-12);
        assert_eq!(r.message, crate::MSG_HIGH_CLONES);
        assert_eq!(r.distribution.type1, 1);
    }

    #[test]
    fn comment_only_differences_are_still_type1() {
        let r = report(&[("a.go", PROCESS_ITEMS), ("b.go", PROCESS_ITEMS_COMMENTED)]);
        assert_eq!(only_pair(&r).clone_type, CloneType::Type1);
    }

    #[test]
    fn renamed_pair_is_type2() {
        let r = report(&[("a.go", PROCESS_ITEMS), ("c.go", SUM_VALUES)]);
        let p = only_pair(&r);
        assert_eq!(p.clone_type, CloneType::Type2);
        assert!((p.similarity - 1.0).abs() < 1e-12);
    }

    #[test]
    fn added_statement_pair_is_type3_with_exact_jaccard() {
        let r = report(&[("a.go", PROCESS_ITEMS), ("d.go", PROCESS_ITEMS_PLUS)]);
        let p = only_pair(&r);
        assert_eq!(p.clone_type, CloneType::Type3);
        assert!(
            (p.similarity - T3_EXPECTED).abs() < 1e-9,
            "similarity {} != {T3_EXPECTED}",
            p.similarity
        );
    }

    #[test]
    fn unrelated_functions_are_not_paired() {
        let r = report(&[("a.go", PROCESS_ITEMS), ("e.go", DESCRIBE)]);
        assert_eq!(r.total_clone_pairs, 0);
        assert!(r.pairs.is_empty());
        assert_eq!(r.message, crate::MSG_NO_CLONES);
        assert!((r.clone_ratio).abs() < 1e-12);
    }

    #[test]
    fn single_function_has_no_self_pair() {
        let r = report(&[("a.go", PROCESS_ITEMS)]);
        assert_eq!(r.total_functions, 1);
        assert_eq!(r.total_clone_pairs, 0);
    }

    #[test]
    fn three_copies_yield_three_pairs_in_name_order() {
        let r = report(&[
            ("a.go", PROCESS_ITEMS),
            ("b.go", PROCESS_ITEMS),
            ("g.go", PROCESS_ITEMS),
        ]);
        assert_eq!(r.total_clone_pairs, 3);
        let names: Vec<String> = r.pairs.iter().map(crate::ClonePair::display_name).collect();
        assert_eq!(
            names,
            vec![
                "a.go::ProcessItems <-> b.go::ProcessItems",
                "a.go::ProcessItems <-> g.go::ProcessItems",
                "b.go::ProcessItems <-> g.go::ProcessItems",
            ]
        );
    }

    #[test]
    fn small_functions_are_counted_but_never_paired() {
        let r = report(&[("a.go", TINY_ADD), ("b.go", TINY_ADD)]);
        assert_eq!(r.total_functions, 2);
        assert_eq!(r.total_clone_pairs, 0);
    }

    #[test]
    fn per_file_summaries_track_cloned_functions_and_pairs() {
        let r = report(&[
            ("a.go", PROCESS_ITEMS),
            ("b.go", PROCESS_ITEMS),
            ("e.go", DESCRIBE),
        ]);
        assert_eq!(r.files.len(), 3);
        assert_eq!(r.files[0].path, "a.go");
        assert_eq!(r.files[0].total_functions, 1);
        assert_eq!(r.files[0].cloned_functions, 1);
        assert_eq!(r.files[0].pairs, vec![0]);
        assert_eq!(r.files[1].pairs, vec![0]);
        assert_eq!(r.files[2].cloned_functions, 0);
        assert!(r.files[2].pairs.is_empty());
        // 2 cloned of 3 functions.
        assert!((r.clone_ratio - 2.0 / 3.0).abs() < 1e-12);
        assert_eq!(r.message, crate::MSG_HIGH_CLONES);
    }

    #[test]
    fn ratio_bands_select_the_message() {
        // 2 cloned functions out of 40 => ratio 0.05 => low.
        let mut files: Vec<(String, &str)> =
            (0..38).map(|i| (format!("t{i}.go"), TINY_ADD)).collect();
        files.push(("a.go".to_string(), PROCESS_ITEMS));
        files.push(("b.go".to_string(), PROCESS_ITEMS));
        let refs: Vec<(&str, &str)> = files.iter().map(|(p, s)| (p.as_str(), *s)).collect();
        let r = report(&refs);
        assert_eq!(r.total_functions, 40);
        assert!((r.clone_ratio - 0.05).abs() < 1e-12);
        assert_eq!(r.message, crate::MSG_LOW_CLONES);
    }

    /// Scale gate for the pathological "every function is identical" input:
    /// 4000 copies form 7,998,000 pairs, which must be enumerated and
    /// classified well under 10 s (the old string-keyed engine took ~50 s and
    /// 1.7 GB). Run with:
    /// `cargo test --release -p cf-clones -- --ignored identical_functions`
    #[test]
    #[ignore = "release-mode scale gate; run with --release -- --ignored"]
    fn four_thousand_identical_functions_finish_under_ten_seconds() {
        const COPIES: usize = 4000;
        const BUDGET_SECS: u64 = 10;

        let parser = Parser::new();
        let root = parser
            .parse("f.go", PROCESS_ITEMS.as_bytes())
            .expect("parses");
        let scan = Visitor::new().scan(&root);
        let mut agg = Aggregator::new();
        for i in 0..COPIES {
            agg.add_file(&format!("f{i}.go"), scan.clone());
        }

        let start = std::time::Instant::now();
        let r = agg.finish();
        let elapsed = start.elapsed();

        assert_eq!(r.total_clone_pairs as usize, COPIES * (COPIES - 1) / 2);
        assert_eq!(r.pairs.len(), DEFAULT_MAX_CLONE_PAIRS);
        assert!((r.clone_ratio - 1.0).abs() < 1e-12);
        assert!(
            elapsed.as_secs() < BUDGET_SECS,
            "took {elapsed:?}, budget {BUDGET_SECS}s"
        );
    }
}
