# Understanding comment sentiment analysis

This page explains the mental model behind the sentiment analyzer: how it
finds the comments a commit introduced or changed, how it filters them, how
VADER and the multilingual lexicon score them, and the software-engineering
domain adjustments that keep technical language from skewing the result. For
the flags and the output schema, see the
[Sentiment reference](../analyzers/sentiment.md).

---

## What it measures

The sentiment analyzer tracks the **emotional tone of the comments developers
write** over Git history. For each commit it looks at the comments that
commit added or edited, filters out noise, scores the rest, and reports one
score per tick that received such comments. This reveals how developer
sentiment evolves over time — without re-counting the unchanged comments of
every touched file.

### Per-commit comment extraction

For every Insert or Modify change of a commit that survives the shared change
filter, the analyzer:

1. Parses the **new** version of the file into a UAST and collects its
   `Comment` nodes; for a Modify it parses the **old** version too (both
   parses go through the per-commit parse cache, so each blob is parsed once).
2. Merges adjacent comment lines into blocks on each side (a block comment, a
   run of `//` lines, a Javadoc / Doxygen block whose ` * ` continuation
   markers are stripped).
3. Filters each side's blocks (see below).
4. Keeps the **multiset difference new − old**: a block whose text also
   occurs in the old version cancels one occurrence and is not counted. A
   commit that only adds code to a commented file therefore contributes no
   comment; an edited comment counts once, as new text; a brand-new file
   contributes all of its comments.

### Comment filtering

Comments are filtered using several heuristics:

- **Minimum length**: comments shorter than `--min-comment-len` characters (default
  20, after delimiters are stripped) are skipped; `0` disables the check
- **Letter ratio**: at least 60% of the bytes must be Unicode letters (filters out commented-out code)
- **First character**: must be a Unicode letter or digit (supports all scripts)
- **License detection**: comments matching license/copyright patterns (including UK "licence" spelling) are excluded
- **Function name removal**: inline function references like `doThing()` are stripped before analysis

### Multilingual support

The analyzer supports comments in any language that uses Unicode letters:

- **CJK**: Chinese, Japanese, Korean
- **Cyrillic**: Russian, Ukrainian, etc.
- **Arabic**: Right-to-left scripts
- **Latin**: English, French, German, Spanish, etc.
- All other Unicode letter scripts

Comment filtering uses Unicode-aware character classes (`\p{L}` for letters,
`\p{N}` for digits) rather than ASCII-only ranges, ensuring comments in
non-English languages are not silently dropped.

!!! info "Multilingual sentiment scoring"
    Sentiment scoring uses VADER's English lexicon as the base, extended with **93,000+ multilingual word entries** from the Chen-Skiena lexicon dataset (ACL 2014) covering **32 languages**. Non-ASCII words from the dataset are injected into VADER's lexicon at startup, enabling basic sentiment scoring for comments in Russian, Chinese, Japanese, Korean, Arabic, and 27 other languages. VADER's grammatical rules (negation, intensifiers) still operate on English syntax, so scoring accuracy for non-English comments is lower than for English — but significantly better than no coverage.

### Sentiment scoring

Filtered comments are scored with **VADER** (Valence Aware Dictionary and
sEntiment Reasoner), implemented in `cf-sentiment` over the vendored VADER
lexicon and emoji table, enhanced with **software engineering domain
adjustments**.

#### VADER base scoring

VADER is a lexicon and rule-based sentiment analyzer designed for social
media and short text. It handles negations, intensifiers, contrastive `but`,
idioms, ALLCAPS emphasis, `!` / `?` emphasis and emoji. The compound score in
`[-1, 1]` is mapped to the reported `[0, 1]` range (`score = (compound + 1) /
2`, so `0.5` is neutral). The implementation reproduces the published VADER
example scores (for instance `VADER is smart, handsome, and funny!` →
compound `0.8439`).

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

VADER scores some words negative that are neutral in software engineering,
and it does not know some words that are negative there. The scorer matches
the lists below as **whole words** (or whole phrases) — `deadline` is not
`dead`, `skill` is not `kill`, `hacker` is not `hack` — and adjusts the
comment's compound score:

- every **neutralizer** hit contributes `(0 − compound) × 0.8`;
- every **negative term** hit contributes its shift;
- the mean contribution is added to the compound, which is then clamped to
  `[-1, 1]`.

**Neutralized terms** (whole-word hits pull the comment toward neutral). The
list holds exactly the terms the VADER lexicon scores negative that are
neutral in code comments; a word VADER does not score cannot be misclassified
by it and is deliberately not listed:

| Term | Context |
|---|---|
| `kill`, `killed`, `killing`, `kills` | process management |
| `fatal` | error severity levels |
| `dead` | dead code, dead letters, dead-lock |
| `destroy`, `destroyed` | object lifecycle |
| `panic` | error handling (Go, Rust) |
| `obsolete` | API lifecycle |
| `exploit` | security |
| `conflict`, `conflicts` | merge conflicts |
| `reject`, `rejected` | request handling, promises |
| `critical` | severity levels, critical sections |
| `argument`, `arguments` | function arguments |
| `block`, `blocked` | blocks, blocking calls |
| `interrupt`, `interrupted` | signals, threads |
| `trap` | signal / instruction traps |
| `dump` | core dumps, data dumps |

**Genuinely negative in SE** (whole-word hits push the comment toward
negative; none of these is scored by the VADER lexicon):

| Term | Shift | Indication |
|---|---|---|
| `hack` | −0.3 | quick-and-dirty solutions |
| `hacky` | −0.4 | quick-and-dirty solutions |
| `kludge` | −0.5 | inelegant fixes |
| `workaround` | −0.2 | avoiding the root cause |
| `technical debt` | −0.3 | accumulated shortcuts |
| `spaghetti` | −0.4 | poor code structure |
| `nightmare` | −0.4 | maintenance difficulty |

Words VADER already scores negative (`awful`, `terrible`, `horrible`, `debt`,
`bug`, …) are not listed: they keep their lexicon valence.

#### Comment length weighting

Longer comments carry more weight in a tick's score, as they tend to contain
more meaningful sentiment signal. A comment's weight is its length in characters
over the tick's average comment length, capped at 3× to prevent a single long
comment from dominating.

#### Tick scores, trend and periods

A tick's score is the length-weighted mean of its comments' scores. Only
ticks with at least one scored comment receive a score; a tick of code-only
commits is not "neutral" and not "negative" — it has no data and is absent
from the time series. The trend is the least-squares line through the scored
ticks, evaluated at the first and last scored tick and clamped to `[0, 1]`;
the low-sentiment periods are the scored ticks at or below the negative
threshold, worst first.

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
- **UAST dependency**: Requires UAST parsing support for the target language. Files in unsupported languages are skipped (shell scripts fall back to line-wise `#` comments).
- **Sarcasm**: Sarcasm, irony, and context-dependent meaning can mislead the classifier. Comments like "great, another production outage" may be scored as positive.
- **Comment extraction**: Only comments that appear in the UAST are analyzed. Preprocessor directives, build file comments, and non-code files are excluded.
- **Whole-comment adjustment**: an SE-domain hit adjusts the whole comment's compound score, not just the matched word, so a positive remark that also mentions `kill` is pulled toward neutral.
- **CPU intensive**: The sentiment analyzer parses both versions of every modified file in every commit. For large repositories, this is significantly slower than non-UAST analyzers; the per-commit pre-pass runs in parallel.
- **Minimum length filter**: The default minimum of 20 characters filters out many short but potentially meaningful comments (e.g., `// FIXME`, `// HACK`). Lower `--min-comment-len` to capture these, at the cost of more noise.

---

## See also

- [Sentiment reference](../analyzers/sentiment.md) — flags, tick semantics and output schema.
- [Quick start](../getting-started/quickstart.md) — run history analysis.
