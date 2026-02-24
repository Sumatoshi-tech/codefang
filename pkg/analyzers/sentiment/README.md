# Sentiment Analysis

## Preface
The mood of the development team is often reflected in their communication. Comments in code can serve as a proxy for team morale and code quality culture.

## Problem
Burnout, frustration, and toxic environments are killer for productivity. It's hard to detect these trends in a distributed team without reading every message.

## How analyzer solves it
The Sentiment analyzer scans source code comments introduced in each commit. It classifies the text as Positive, Negative, or Neutral using VADER sentiment analysis enhanced with software engineering domain adjustments.

## Historical context
Sentiment analysis (Opinion Mining) is a subfield of NLP. Applying it to software engineering (SE) data is a growing research area (e.g., "Emotion Mining in Software Engineering"). VADER (Hutto & Gilbert, 2014) is a rule-based model designed for short text; it achieves F1=0.96 on social media text. However, it requires domain adjustment for SE contexts where technical terms like "kill", "abort", and "fatal" are emotionally neutral.

## Real world examples
- **Burnout Detection:** A trend of increasingly negative comments might indicate team stress.
- **Hotspots:** Specific files or modules that trigger negative comments (e.g., "this hack fix again").
- **Technical Debt:** Comments containing "workaround", "hack", "kludge" signal accumulated shortcuts.

## How analyzer works here
1. **Comment Extraction:** Uses UAST to find comment nodes in the added/changed code.
2. **Preprocessing:** Cleans up the text (removes code snippets, formatting). Supports Unicode/multilingual comments.
3. **SE-Domain Adjustment:** Technical terms that VADER misclassifies are adjusted toward neutral (e.g., "kill process") or toward negative (e.g., "hacky workaround").
4. **Length-Weighted Scoring:** Longer comments carry more weight in the final score.
5. **Regression-Based Trend:** Linear regression instead of first/last comparison for robust trend detection.

## Key Features
- **Multilingual comment extraction** — Unicode-aware regex patterns support CJK, Cyrillic, Arabic, and all Unicode scripts
- **SE-domain lexicon** — Technical terms adjusted to avoid false negatives/positives
- **Length-weighted scoring** — Longer, more substantive comments carry proportionally more weight
- **Linear regression trend** — Robust to outliers and intermediate noise
- **Enhanced visualization** — Multi-series plot with threshold bands, trend line, comment count, and pie chart distribution

## Limitations
- **VADER is English-optimized:** Sentiment scoring accuracy degrades for non-English text
- **Sarcasm:** "Great job breaking production" might be classified as positive
- **Context:** Domain-specific meaning beyond the lexicon is not captured
