# Sentiment analyzer reference

The sentiment analyzer classifies **comment sentiment** across Git history,
extracting new or changed comments via UAST parsing and classifying them as
positive, negative, or neutral.

For the conceptual model — how comments are filtered and scored, the
multilingual lexicon, and the software-engineering domain adjustments — see
[Understanding comment sentiment analysis](../explanation/sentiment.md). To run
it, see the [Quick start](../getting-started/quickstart.md).

---

## Configuration options

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

The analyzer requires UAST support to extract comments and is automatically enabled when the UAST pipeline is available.

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
      "tick": 1,
      "start_time": "2024-01-16T09:00:00Z",
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

## See also

- [Understanding comment sentiment analysis](../explanation/sentiment.md) — the mental model, VADER scoring, multilingual lexicon, SE-domain lexicon, and limitations.
- [Quick start](../getting-started/quickstart.md) — run history analysis.
