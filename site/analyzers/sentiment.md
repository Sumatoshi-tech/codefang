# Sentiment Analyzer

The sentiment analyzer classifies **comment sentiment** across Git history. For each commit, it extracts new or changed comments via UAST parsing, filters out noise, and classifies the remaining comments as positive or negative. This reveals how developer sentiment evolves over time.

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
4. Filters out noise (short comments, non-English text, license headers, function signatures)
5. Returns the filtered comments as a `TC` payload (`sentiment.CommitResult`)

### Comment Filtering

Comments are filtered using several heuristics:

- **Minimum length**: Comments shorter than the threshold are skipped
- **Letter ratio**: At least 60% of characters must be letters (filters out commented-out code)
- **First character**: Must start with an alphanumeric character
- **License detection**: Comments matching license/copyright patterns are excluded
- **Function name removal**: Inline function references like `doThing()` are stripped before analysis

### Sentiment Classification

Filtered comments are classified as positive, negative, or neutral using **VADER** (Valence Aware Dictionary and sEntiment Reasoner) via the [GoVader](https://github.com/jonreiter/govader) library. VADER is a lexicon and rule-based sentiment analyzer designed for social media and short text; it handles negations, intensifiers, and punctuation. The compound score (‑1 to 1) is mapped to our [0, 1] range; comments are averaged per tick.

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

## Example Output

=== "JSON"

    ```json
    {
      "sentiment_summary": {
        "total_comments": 245,
        "positive_comments": 42,
        "negative_comments": 18,
        "neutral_comments": 185,
        "positive_ratio": 0.171,
        "negative_ratio": 0.073
      },
      "time_series": [
        {
          "tick": 0,
          "comments": 12,
          "positive": 3,
          "negative": 1,
          "sentiment_score": 0.167
        },
        {
          "tick": 5,
          "comments": 8,
          "positive": 0,
          "negative": 3,
          "sentiment_score": -0.375
        }
      ],
      "negative_examples": [
        {
          "tick": 5,
          "text": "This is a terrible hack that should be fixed ASAP",
          "score": -0.82
        }
      ]
    }
    ```

=== "YAML"

    ```yaml
    sentiment_summary:
      total_comments: 245
      positive_comments: 42
      negative_comments: 18
      neutral_comments: 185
    time_series:
      - tick: 0
        comments: 12
        positive: 3
        negative: 1
      - tick: 5
        comments: 8
        positive: 0
        negative: 3
    ```

---

## Use Cases

- **Team morale tracking**: Monitor sentiment trends over time. A sustained drop in sentiment may correlate with deadline pressure, technical debt accumulation, or team issues.
- **Code quality signals**: Negative sentiment spikes often correspond to periods of rushed development, workarounds, or "hack" implementations.
- **Post-mortem analysis**: After incidents, examine whether comment sentiment degraded in the lead-up to the problem.
- **Documentation quality**: Projects with predominantly neutral or positive comments tend to have better documentation culture.

---

## Limitations

- **English only**: Sentiment classification is designed for English-language comments. Comments in other languages may produce unreliable scores.
- **UAST dependency**: Requires UAST parsing support for the target language. Files in unsupported languages are skipped.
- **False positives**: Sarcasm, irony, and domain-specific terminology can mislead the classifier. Comments like "this is a killer feature" may be scored as negative.
- **Comment extraction**: Only comments that appear in the UAST are analyzed. Preprocessor directives, build file comments, and non-code files are excluded.
- **CPU intensive**: The sentiment analyzer performs UAST parsing for every modified file in every commit. For large repositories, this is significantly slower than non-UAST analyzers. It benefits from parallel execution via the framework's worker pool.
- **Minimum length filter**: The default minimum of 20 characters filters out many short but potentially meaningful comments (e.g., `// FIXME`, `// HACK`). Lower `MinLength` to capture these, at the cost of more noise.
