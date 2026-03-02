# Sentiment Analyzer

The sentiment analyzer classifies **comment sentiment** across Git history. For each commit, it extracts new or changed comments via UAST parsing, filters out noise, and classifies the remaining comments as positive, negative, or neutral. This reveals how developer sentiment evolves over time.

---

## Quick Start

```bash
codefang run -a history/sentiment .
```

!!! note "Requires UAST"
    The sentiment analyzer needs UAST support to extract comments. It is automatically enabled when the UAST pipeline is available.

---

## Architecture

The sentiment analyzer follows the **TC/Aggregator pattern**:

1. **Consume phase**: For each commit, `Consume()` extracts and filters comments from UAST changes, returning them as a `TC` (per-commit result). The analyzer retains no per-commit state.
2. **Aggregation phase**: A `sentiment.Aggregator` collects TCs, groups comments by time bucket (tick), computes sentiment scores, and produces `TICK` results.
3. **Serialization phase**: `SerializeTICKs()` converts aggregated TICKs into JSON, YAML, binary, or HTML plot output.

This separation enables streaming output, budget-aware memory spilling, and decoupled aggregation.

---

## What It Measures

### Per-Commit Comment Extraction

For each commit, the analyzer:

1. Computes the UAST diff between the old and new file versions
2. Extracts comment nodes from the new UAST
3. Merges adjacent comment lines into single blocks
4. Filters out noise (short comments, license headers, function signatures)
5. Returns the filtered comments as a `TC` payload (`sentiment.CommitResult`)

### Comment Filtering

Comments are filtered using several heuristics:

- **Minimum length**: Comments shorter than the threshold are skipped
- **Letter ratio**: At least 60% of characters must be Unicode letters (filters out commented-out code)
- **First character**: Must start with a Unicode letter or digit (supports all scripts)
- **License detection**: Comments matching license/copyright patterns (including UK "licence" spelling) are excluded
- **Function name removal**: Inline function references like `doThing()` are stripped before analysis

### Multilingual Support

The analyzer supports comments in any language that uses Unicode letters:

- **CJK**: Chinese, Japanese, Korean
- **Cyrillic**: Russian, Ukrainian, etc.
- **Arabic**: Right-to-left scripts
- **Latin**: English, French, German, Spanish, etc.
- All other Unicode letter scripts

Comment filtering uses Unicode-aware regex patterns (`\p{L}` for letters, `\p{N}` for digits) rather than ASCII-only ranges, ensuring comments in non-English languages are not silently dropped.

!!! info "Multilingual sentiment scoring"
    Sentiment scoring uses VADER's English lexicon as the base, extended with **93,000+ multilingual word entries** from the Chen-Skiena lexicon dataset (ACL 2014) covering **32 languages**. Non-ASCII words from the dataset are injected into VADER's lexicon at startup, enabling basic sentiment scoring for comments in Russian, Chinese, Japanese, Korean, Arabic, and 27 other languages. VADER's grammatical rules (negation, intensifiers) still operate on English syntax, so scoring accuracy for non-English comments is lower than for English — but significantly better than no coverage.

### Sentiment Classification

Filtered comments are classified as positive, negative, or neutral using **VADER** (Valence Aware Dictionary and sEntiment Reasoner) via the [GoVader](https://github.com/jonreiter/govader) library, enhanced with **software engineering domain adjustments**.

#### VADER Base Scoring

VADER is a lexicon and rule-based sentiment analyzer designed for social media and short text. It handles negations, intensifiers, and punctuation. The compound score (-1 to 1) is mapped to our [0, 1] range.

#### Multilingual Lexicon Extension

At startup, the analyzer injects ~93,000 multilingual word entries from the [Chen-Skiena lexicon dataset](https://aclanthology.org/P14-2063/) (ACL 2014) into VADER's lexicon. This covers 32 languages:

| Language Family | Languages |
|---|---|
| **Slavic** | Russian, Ukrainian, Polish, Czech, Slovak, Croatian, Bulgarian |
| **CJK** | Chinese, Japanese, Korean |
| **Romance** | Spanish, French, Portuguese, Italian, Romanian |
| **Germanic** | German, Dutch, Swedish, Danish, Norwegian, Finnish |
| **Other** | Arabic, Hebrew, Hindi, Thai, Turkish, Greek, Hungarian, Indonesian, Malay, Vietnamese, Persian |

Only non-ASCII words are injected to avoid overriding VADER's curated English entries. Words receive binary valence: +1.5 (positive) or -1.5 (negative), which is the mid-range of VADER's scale.

To regenerate lexicons from updated source data:

```bash
go run tools/lexgen/lexgen.go -pos pos_words.txt -neg neg_words.txt \
  -o internal/analyzers/sentiment/lexicons/lexicon_data.gen.go
```

#### SE-Domain Adjustments

VADER frequently misclassifies technical terms that sound negative in natural language but are neutral in software engineering. The analyzer applies domain-specific adjustments:

**Neutralized terms** (pushed toward neutral):
`kill`, `abort`, `fatal`, `terminate`, `dead`, `destroy`, `panic`, `deprecated`, `obsolete`, `execute`, `exploit`, `conflict`, `revert`, `reject`, `critical`

**Genuinely negative in SE** (pushed toward negative):
`hack`, `hacky`, `kludge`, `workaround`, `spaghetti`, `nightmare`, `technical debt`

#### Comment Length Weighting

Longer comments carry more weight in the sentiment score, as they tend to contain more meaningful sentiment signal. Weight is capped at 3x the average to prevent a single long comment from dominating.

#### Trend Analysis

Sentiment trend is computed using **linear regression** (least squares) rather than a simple first-to-last comparison. This makes the trend robust to outliers and intermediate noise.

---

## Configuration Options

| Option | Type | Default | Description |
|---|---|---|---|
| `CommentSentiment.MinLength` | `int` | `20` | Minimum character length for a comment to be analyzed. Comments shorter than this are skipped. |
| `CommentSentiment.Gap` | `float` | `0.5` | Sentiment score threshold. Values must be in the range (0, 1). Higher values require stronger sentiment signal to classify as positive/negative. |

```yaml
# .codefang.yml
history:
  sentiment:
    min_comment_length: 20
    gap: 0.5
```

!!! tip "Tuning the gap"
    A gap of `0.5` is conservative -- only strongly positive or negative comments are flagged. Lower it to `0.3` for more sensitivity, or raise to `0.7` to capture only the most extreme sentiments.

---

## Output Formats

### JSON Output

```json
{
  "time_series": [
    {
      "tick": 0,
      "sentiment": 0.72,
      "comment_count": 12,
      "commit_count": 5,
      "classification": "positive"
    },
    {
      "tick": 1,
      "sentiment": 0.35,
      "comment_count": 8,
      "commit_count": 3,
      "classification": "negative"
    }
  ],
  "trend": {
    "start_tick": 0,
    "end_tick": 10,
    "start_sentiment": 0.68,
    "end_sentiment": 0.52,
    "trend_direction": "declining",
    "change_percent": -23.5
  },
  "low_sentiment_periods": [
    {
      "tick": 5,
      "sentiment": 0.18,
      "comments": ["This is a terrible hack"],
      "risk_level": "HIGH"
    }
  ],
  "aggregate": {
    "total_ticks": 10,
    "total_comments": 245,
    "total_commits": 89,
    "average_sentiment": 0.56,
    "positive_ticks": 4,
    "neutral_ticks": 3,
    "negative_ticks": 3
  }
}
```

### Plot Output

The HTML plot includes:

1. **Sentiment Over Time**: Line chart with sentiment score, positive/negative threshold bands (dashed lines), regression trend line, and comment count on a secondary axis
2. **Sentiment Distribution**: Donut chart showing the breakdown of positive, neutral, and negative time periods

The plot uses semantic colors (green = positive, yellow = neutral, red = negative) and includes interpretive hints.

### Terminal Output

The terminal renderer provides a colored, Unicode-rich summary:

```
┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
┃ SENTIMENT ANALYSIS                                       💬 ┃
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛

  Summary
────────────────────────────────────────────────────────────────
  Average Sentiment: [██████████░░░░░░░░░░] 5/10 😐
  Total Ticks:       10
  Total Comments:    245
  Total Commits:     89

  Distribution
────────────────────────────────────────────────────────────────
  😊 Positive (≥0.6)  ████████░░░░░░░░░░░░  40%  (4)
  😐 Neutral           ██████░░░░░░░░░░░░░░  30%  (3)
  😟 Negative (≤0.4)  ██████░░░░░░░░░░░░░░  30%  (3)

  Trend
────────────────────────────────────────────────────────────────
  Direction: ↘ declining
  Start (tick 0): 0.68  →  End (tick 10): 0.52
  Change: -23.5%

  Sentiment Timeline
────────────────────────────────────────────────────────────────
  ▇▆▅▄▃▂▃▄▅▆
  neg              pos
```

---

## Use Cases

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

## SE-Domain Lexicon

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
