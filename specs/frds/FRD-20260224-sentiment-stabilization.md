# FRD: Sentiment Analyzer Stabilization

**Date:** 2026-02-24
**Status:** In Progress
**Analyzer:** history/sentiment

## Problem Statement

The sentiment analyzer has several correctness issues that produce inaccurate results. Since this tool influences team assessments, incorrect results could lead to wrong decisions. Critical issues found:

1. **Broken license regex** — `licenseRE` has invalid syntax that fails to properly detect license comments
2. **English-only filters** — Regex patterns use ASCII-only character classes, filtering out all non-English comments
3. **No SE-domain awareness** — VADER misclassifies technical terms like "kill", "abort", "fatal", "deprecated" as negative sentiment
4. **Weak trend calculation** — Uses only first/last tick, ignoring intermediate data points
5. **No comment length weighting** — Short throwaway comments have equal weight to substantial explanations
6. **Minimal plot output** — Single line chart without threshold indicators or contextual information

## Research Findings

### VADER Methodology
- VADER (Hutto & Gilbert 2014) is rule-based, designed for social media text
- Achieves F1=0.96 on social media but only F1=0.34-0.38 on domain-specific text (2025 study)
- Not multilingual by design; degraded accuracy for non-English text
- SentiCR (code-review-specific) achieves 83% accuracy on SE data vs VADER's poor performance

### Key Discrepancies vs Golden Implementations
- Technical terms ("kill process", "abort transaction", "fatal error") produce false negatives
- Code idioms ("deprecated", "hack", "workaround") produce inaccurate sentiment
- Non-English comments are silently dropped by ASCII-only regex filters
- Simple first/last trend ignores all intermediate data (regression is standard)

## Phase 1: Correctness Fixes

### 1.1 Fix License Regex
**Current:** `(?i)[li[cs]en[cs][ei]|copyright|©` (broken character class)
**Fixed:** `(?i)(licen[cs]e|copyright|©)`

### 1.2 Unicode-Aware Comment Filtering
- `filteredFirstCharRE`: Allow Unicode letters `\p{L}` not just `[a-zA-Z]`
- `charsRE`: Match Unicode letters `\p{L}` for letter ratio
- `filteredCharsRE`: Allow Unicode letters in comments

### 1.3 SE-Domain Lexicon Adjustments
Add domain-specific score adjustments for technical terms that VADER misclassifies:
- Negative-sounding but neutral: kill, abort, fatal, terminate, dead, deprecated
- Positive-sounding but neutral: master, execute, exploit, hit, strike
- Actually negative in SE context: hack, workaround, kludge, technical debt

### 1.4 Weighted Sentiment Scoring
- Weight comments by length (longer comments carry more signal)
- Cap weight to avoid single long comment dominating

### 1.5 Improved Trend Calculation
- Use linear regression (least squares) instead of first/last comparison
- More robust to outliers and intermediate variations

## Phase 2: UX Improvements

### 2.1 Enhanced Plot Output
- Add positive/negative threshold bands
- Show comment count as secondary axis
- Add trend line overlay
- Better color semantics (green=positive, red=negative, gray=neutral)

### 2.2 Terminal Text Output
- Colored summary with key metrics
- Sparkline for sentiment trend
- Risk indicators for low sentiment periods
- Progress bars for sentiment distribution

## Acceptance Criteria

- [ ] All regex patterns correctly handle their intended use cases
- [ ] Non-English comments are properly processed (not silently dropped)
- [ ] Technical terms do not produce false sentiment readings
- [ ] Trend calculation uses regression for robustness
- [ ] Plot shows threshold zones and contextual information
- [ ] `make lint` passes
- [ ] `make test` passes
- [ ] Coverage ≥80% for sentiment package
