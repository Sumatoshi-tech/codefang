# Understanding comment sentiment analysis

This page explains the mental model behind the sentiment analyzer: how it
extracts and filters comments, how VADER and the multilingual lexicon score
them, and the software-engineering domain adjustments that keep technical
language from skewing the result. For configuration keys and the output schema,
see the [Sentiment reference](../analyzers/sentiment.md).

---

## What it measures

The sentiment analyzer classifies **comment sentiment** across Git history. For
each commit, it extracts new or changed comments via UAST parsing, filters out
noise, and classifies the remaining comments as positive, negative, or neutral.
This reveals how developer sentiment evolves over time.

### Per-commit comment extraction

For each commit, the analyzer:

1. Computes the UAST diff between the old and new file versions
2. Extracts comment nodes from the new UAST
3. Merges adjacent comment lines into single blocks
4. Filters out noise (short comments, license headers, function signatures)
5. Returns the filtered comments as a `TC` payload (`sentiment.CommitResult`)

### Comment filtering

Comments are filtered using several heuristics:

- **Minimum length**: Comments shorter than the threshold are skipped
- **Letter ratio**: At least 60% of characters must be Unicode letters (filters out commented-out code)
- **First character**: Must start with a Unicode letter or digit (supports all scripts)
- **License detection**: Comments matching license/copyright patterns (including UK "licence" spelling) are excluded
- **Function name removal**: Inline function references like `doThing()` are stripped before analysis

### Multilingual support

The analyzer supports comments in any language that uses Unicode letters:

- **CJK**: Chinese, Japanese, Korean
- **Cyrillic**: Russian, Ukrainian, etc.
- **Arabic**: Right-to-left scripts
- **Latin**: English, French, German, Spanish, etc.
- All other Unicode letter scripts

Comment filtering uses Unicode-aware regex patterns (`\p{L}` for letters, `\p{N}` for digits) rather than ASCII-only ranges, ensuring comments in non-English languages are not silently dropped.

!!! info "Multilingual sentiment scoring"
    Sentiment scoring uses VADER's English lexicon as the base, extended with **93,000+ multilingual word entries** from the Chen-Skiena lexicon dataset (ACL 2014) covering **32 languages**. Non-ASCII words from the dataset are injected into VADER's lexicon at startup, enabling basic sentiment scoring for comments in Russian, Chinese, Japanese, Korean, Arabic, and 27 other languages. VADER's grammatical rules (negation, intensifiers) still operate on English syntax, so scoring accuracy for non-English comments is lower than for English — but significantly better than no coverage.

### Sentiment classification

Filtered comments are classified as positive, negative, or neutral using **VADER** (Valence Aware Dictionary and sEntiment Reasoner) via the [GoVader](https://github.com/jonreiter/govader) library, enhanced with **software engineering domain adjustments**.

#### VADER base scoring

VADER is a lexicon and rule-based sentiment analyzer designed for social media and short text. It handles negations, intensifiers, and punctuation. The compound score (-1 to 1) is mapped to our [0, 1] range.

#### Multilingual lexicon extension

At startup, the analyzer injects ~93,000 multilingual word entries from the [Chen-Skiena lexicon dataset](https://aclanthology.org/P14-2063/) (ACL 2014) into VADER's lexicon. This covers 32 languages:

| Language Family | Languages |
|---|---|
| **Slavic** | Russian, Ukrainian, Polish, Czech, Slovak, Croatian, Bulgarian |
| **CJK** | Chinese, Japanese, Korean |
| **Romance** | Spanish, French, Portuguese, Italian, Romanian |
| **Germanic** | German, Dutch, Swedish, Danish, Norwegian, Finnish |
| **Other** | Arabic, Hebrew, Hindi, Thai, Turkish, Greek, Hungarian, Indonesian, Malay, Vietnamese, Persian |

Only non-ASCII words are injected to avoid overriding VADER's curated English entries. Words receive binary valence: +1.5 (positive) or -1.5 (negative), which is the mid-range of VADER's scale.

The lexicon data is vendored as one tab-separated file per language under
`crates/cf-sentiment-lexicons/data/` (e.g. `de.tsv`, `ja.tsv`), with a
`MANIFEST.tsv` listing them. The `cf-sentiment-lexicons` build script compiles
these tables into the crate at build time, so updating a lexicon is a matter of
editing the corresponding `data/*.tsv` file and rebuilding — no code generation
step is required.

#### SE-domain adjustments

VADER frequently misclassifies technical terms that sound negative in natural language but are neutral in software engineering. The analyzer applies domain-specific adjustments:

**Neutralized terms** (pushed toward neutral):
`kill`, `abort`, `fatal`, `terminate`, `dead`, `destroy`, `panic`, `deprecated`, `obsolete`, `execute`, `exploit`, `conflict`, `revert`, `reject`, `critical`

**Genuinely negative in SE** (pushed toward negative):
`hack`, `hacky`, `kludge`, `workaround`, `spaghetti`, `nightmare`, `technical debt`

#### Comment length weighting

Longer comments carry more weight in the sentiment score, as they tend to contain more meaningful sentiment signal. Weight is capped at 3x the average to prevent a single long comment from dominating.

#### Trend analysis

Sentiment trend is computed using **linear regression** (least squares) rather than a simple first-to-last comparison. This makes the trend robust to outliers and intermediate noise.

---

## SE-domain lexicon

The following terms have special handling in the sentiment scorer:

### Technical terms neutralized (not negative)

These terms are common in SE but trigger VADER's negative scoring:

| Term | Context |
|---|---|
| `kill` | Process management |
| `abort` | Transaction/operation cancellation |
| `fatal` | Error severity levels |
| `terminate` | Process/thread lifecycle |
| `panic` | Error handling (Go, Rust) |
| `deprecated` | API lifecycle |
| `execute` | Command/query execution |
| `conflict` | Merge conflicts |
| `critical` | Severity levels |

### Terms with genuine negative sentiment

These terms indicate real frustration or code quality issues:

| Term | Indication |
|---|---|
| `hack` / `hacky` | Quick-and-dirty solutions |
| `kludge` | Inelegant fixes |
| `workaround` | Avoiding root cause |
| `spaghetti` | Poor code structure |
| `nightmare` | Maintenance difficulty |
| `technical debt` | Accumulated shortcuts |

---

## Use cases

- **Team morale tracking**: Monitor sentiment trends over time. A sustained drop in sentiment may correlate with deadline pressure, technical debt accumulation, or team issues.
- **Code quality signals**: Negative sentiment spikes often correspond to periods of rushed development, workarounds, or "hack" implementations.
- **Post-mortem analysis**: After incidents, examine whether comment sentiment degraded in the lead-up to the problem.
- **Documentation quality**: Projects with predominantly neutral or positive comments tend to have better documentation culture.
- **Technical debt detection**: Comments containing terms like "hack", "workaround", "kludge" are flagged as genuinely negative in SE context.

---

## Limitations

- **Non-English scoring accuracy**: While 32 languages have lexicon coverage via the Chen-Skiena dataset, VADER's grammatical rules (negation handling, intensifiers, punctuation effects) are English-specific. Non-English comments get word-level sentiment from the lexicon but miss syntactic nuances.
- **UAST dependency**: Requires UAST parsing support for the target language. Files in unsupported languages are skipped.
- **Sarcasm**: Sarcasm, irony, and context-dependent meaning can mislead the classifier. Comments like "great, another production outage" may be scored as positive.
- **Comment extraction**: Only comments that appear in the UAST are analyzed. Preprocessor directives, build file comments, and non-code files are excluded.
- **CPU intensive**: The sentiment analyzer performs UAST parsing for every modified file in every commit. For large repositories, this is significantly slower than non-UAST analyzers. It benefits from parallel execution via the framework's worker pool.
- **Minimum length filter**: The default minimum of 20 characters filters out many short but potentially meaningful comments (e.g., `// FIXME`, `// HACK`). Lower `MinLength` to capture these, at the cost of more noise.

---

## See also

- [Sentiment reference](../analyzers/sentiment.md) — configuration keys and output schema.
- [Quick start](../getting-started/quickstart.md) — run history analysis.
