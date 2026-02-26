# Comments Analysis

## Preface
Code tells you *how*, but comments tell you *why*. Good documentation is essential for long-term project maintainability.

## Problem
It's easy to write code and forget to document it. Over time, this leads to a codebase where the intent is lost, making onboarding difficult and increasing the risk of bugs during modification. Conversely, too many trivial comments can clutter the code.

## How analyzer solves it
The Comments analyzer scans the codebase to evaluate the density, placement, and "quality" of comments. It checks if functions and classes are documented and if comments are placed correctly (e.g., immediately preceding the target).

## Historical context
Documentation coverage is a standard metric in CI/CD pipelines (e.g., Golint, Javadoc checks). This analyzer generalizes it across languages using UAST.

## Real world examples
- **Documentation Audits:** Identifying critical modules that lack documentation.
- **Code Quality Gates:** Preventing code with low documentation coverage from being merged.

## How analyzer works here
1.  **Node Identification:** Finds all comment nodes and function/method/class nodes in the UAST.
2.  **Association:** Heuristically associates comments with the nearest logical code block (e.g., a function definition) based on line numbers.
3.  **Metrics Calculation:**
    - **Documentation Coverage:** % of functions with associated comments.
    - **Placement Score:** Are comments "orphaned" or attached to code?
    - **Length Checks:** Filters out trivial/short comments.

## Limitations
- **Content Analysis:** It checks for the *presence* and *placement* of comments, but doesn't deeply "read" them to judge if they make sense (though sentiment analysis is a separate analyzer).
- **Heuristics:** Association is distance-based and might misattribute comments in complex layouts.

## Further plans
- NLP integration to detect "comment rot" (comments that contradict the code).
- Support for detecting specific documentation standards (e.g., Javadoc vs. Doxygen).
