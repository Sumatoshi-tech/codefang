# Sentiment Analysis

## Preface
The mood of the development team is often reflected in their communication. Comments in code and commit messages can serve as a proxy for team morale.

## Problem
Burnout, frustration, and toxic environments are killer for productivity. It's hard to detect these trends in a distributed team without reading every message.

## How analyzer solves it
The Sentiment analyzer scans source code comments (and potentially commit messages) introduced in each commit. It classifies the text as Positive, Negative, or Neutral.

## Historical context
Sentiment analysis (Opinion Mining) is a subfield of NLP. applying it to software engineering (SE) data is a growing research area (e.g., "Emotion Mining in Software Engineering").

## Real world examples
- **Burnout Detection:** A trend of increasingly negative comments might indicate team stress.
- **Hotspots:** specific files or modules that trigger negative comments (e.g., "this hack fix again").

## How analyzer works here
1.  **Comment Extraction:** Uses UAST to find comment nodes in the added/changed code.
2.  **Preprocessing:** Cleans up the text (removes code snippets, formatting).
3.  **Classification:** Uses a heuristic or ML-based approach (depending on configuration/implementation details) to score the sentiment. *Note: The current implementation often relies on simple dictionaries or external model integration.*

## Limitations
- **Sarcasm:** "Great job breaking production" might be classified as positive.
- **Technical Terms:** Words like "kill", "abort", "fatal" are technical but might be scored as negative.

## Further plans
- Integration with LLMs for better context understanding.
