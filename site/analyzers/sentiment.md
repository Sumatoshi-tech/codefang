# Sentiment analyzer reference

The sentiment analyzer classifies **comment sentiment** across Git history:
for every commit it extracts the comments the commit **introduced or
changed** (the difference between the file's old and new UAST comment sets),
scores them with VADER, and reports one score per tick that carries such
comments.

For the conceptual model — how comments are diffed, filtered and scored, the
multilingual lexicon, and the software-engineering domain adjustments — see
[Understanding comment sentiment analysis](../explanation/sentiment.md). To run
it, see the [Quick start](../getting-started/quickstart.md).

---

## Configuration options

| Flag | Type | Default | Description |
|---|---|---|---|
| `--min-comment-len` | `int` | `20` | Minimum comment length in **characters** (after delimiters are stripped). Shorter comments are not scored. `0` disables the length filter; a negative value is rejected with `Error: invalid --min-comment-len: must be >= 0, got <n>`. |

The classification thresholds are fixed: a tick scores **positive** at
`>= 0.6`, **negative** at `<= 0.4`, **neutral** in between; a negative tick
is a **HIGH**-risk low-sentiment period at `<= 0.2` and **MEDIUM** otherwise;
a trend is **improving** / **declining** when the fitted endpoints differ by
more than `0.1`.

!!! note "`.codefang.yml` sentiment keys"
    The configuration loader parses `history.sentiment.*` keys
    (`min_comment_length`, `gap`, `neutralizer_weight`, `max_weight_ratio`,
    the thresholds), but the `codefang` command line does not apply them —
    `--min-comment-len` is the only sentiment setting the CLI honors. The
    keys are accepted so an existing file keeps loading; they have no effect.

The analyzer needs UAST support for a file's language to extract its
comments; files in unsupported languages are skipped (shell scripts fall back
to line-wise `#` comment extraction).

---

## Tick semantics

- A tick carries a `time_series` entry **only when** at least one of its
  commits introduced or changed a comment that passes the filters. A tick of
  commits without such comments has no score: it is absent from the time
  series, from the trend fit and from `low_sentiment_periods`, and it is not
  classified.
- `aggregate.total_ticks`, `positive_ticks`, `neutral_ticks` and
  `negative_ticks` count the scored ticks; `aggregate.total_commits` counts
  **every** commit of the window, scored tick or not; `total_comments` counts
  the scored comments.
- `trend.start_sentiment` and `trend.end_sentiment` are the least-squares
  line through the scored ticks evaluated at the first and last scored tick,
  **clamped to `[0, 1]`**; `change_percent` is computed from the clamped
  endpoints (`0` when the start endpoint is `0`).
- Every reported sentiment lies in `[0, 1]` (`0.5` is neutral).

---

## Output formats

### JSON output

```json
{
  "time_series": [
    {
      "tick": 0,
      "start_time": "2024-01-15T10:30:00Z",
      "end_time": "2024-01-16T08:45:00Z",
      "sentiment": 0.72,
      "comment_count": 12,
      "commit_count": 5,
      "classification": "positive"
    },
    {
      "tick": 2,
      "start_time": "2024-01-17T09:00:00Z",
      "end_time": "2024-01-17T18:30:00Z",
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
    "total_ticks": 9,
    "total_comments": 245,
    "total_commits": 89,
    "average_sentiment": 0.56,
    "positive_ticks": 4,
    "neutral_ticks": 2,
    "negative_ticks": 3
  }
}
```

Tick 1 above had commits but no new or changed comment, so it has no entry;
its commits are still part of `total_commits`.

### NDJSON and time series

`--format ndjson` emits one record per commit whose `data.Comments` array
holds the comments that commit introduced or changed (empty for a code-only
commit). `--format timeseries` emits per commit `{"comment_count": n,
"sentiment": s}` with `sentiment` present only when `n > 0`.

### Plot output

The HTML plot includes:

1. **Sentiment Over Time**: Line chart with sentiment score, positive/negative threshold bands (dashed lines), regression trend line, and comment count on a secondary axis
2. **Sentiment Distribution**: Donut chart showing the breakdown of positive, neutral, and negative time periods

The plot uses semantic colors (green = positive, yellow = neutral, red = negative) and includes interpretive hints.

### Terminal output

The terminal renderer provides a colored, Unicode-rich summary:

```text
┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
┃ SENTIMENT ANALYSIS                                       💬 ┃
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛

  Summary
────────────────────────────────────────────────────────────────
  Average Sentiment: [██████████░░░░░░░░░░] 5/10 😐
  Total Ticks:       9
  Total Comments:    245
  Total Commits:     89

  Distribution
────────────────────────────────────────────────────────────────
  😊 Positive (≥0.6)  █████████░░░░░░░░░░░  44%  (4)
  😐 Neutral           ████░░░░░░░░░░░░░░░░  22%  (2)
  😟 Negative (≤0.4)  ███████░░░░░░░░░░░░░  33%  (3)

  Trend
────────────────────────────────────────────────────────────────
  Direction: ↘ declining
  Start (tick 0): 0.68  →  End (tick 10): 0.52
  Change: -23.5%

  Sentiment Timeline
────────────────────────────────────────────────────────────────
  ▇▆▅▄▃▂▃▄▅
  neg              pos
```

---

## See also

- [Understanding comment sentiment analysis](../explanation/sentiment.md) — the mental model, VADER scoring, multilingual lexicon, SE-domain lexicon, and limitations.
- [Quick start](../getting-started/quickstart.md) — run history analysis.
