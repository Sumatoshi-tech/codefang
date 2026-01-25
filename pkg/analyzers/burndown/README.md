# Burndown Analysis

## Preface
Understanding the evolution of a codebase is crucial for maintaining its health. Knowing how old the code is, who wrote it, and how actively it's being modified can reveal hidden risks and opportunities for improvement.

## Problem
As projects grow, "knowledge silos" form. Developers leave, and their code remains, often becoming "legacy" that no one dares to touch. It's difficult to answer questions like:
- "How much of the code is actively maintained?"
- "Who are the main contributors to this module?"
- "Is the project accumulating technical debt in the form of untouched, aging code?"

## How analyzer solves it
The Burndown analyzer calculates "line burndown" statistics. It tracks every line of code through the history of the project, recording when it was introduced and by whom. It generates a matrix where one dimension is time (when the line was last touched) and the other is the current time. This allows visualizing how code "survives" over time.

## Historical context
Code burndown charts are a well-established visualization in software engineering analytics. They provide a high-level view of code churn and stability, often used to assess project maturity and developer retention.

## Real world examples
- **Bus Factor Estimation:** By looking at code ownership (which developer "owns" which lines), you can identify modules that depend on a single person.
- **Refactoring Planning:** Identify "stable" (old) parts of the code vs. "volatile" (frequently changed) parts. Volatile legacy code is often a good target for refactoring.
- **Onboarding:** Help new developers identify who to ask about specific parts of the codebase.

## How analyzer works here
The analyzer iterates through the Git commit history.
1.  **Tree Diff:** It compares the file tree of the current commit with its parents to find modified files.
2.  **File Diff:** For modified files, it computes the line-level differences (diffs).
3.  **Ownership Tracking:** It maintains a data structure (using RBTree for efficiency) that maps every line in every file to its original author and creation time.
4.  **Sparse Matrix:** It aggregates this data into sparse matrices to save memory, representing the "burndown" state at sampled intervals.
5.  **Hibernation:** To handle large repositories, it supports "hibernating" file structures to disk to keep memory usage low.

## Limitations
- **Memory Usage:** Tracking every line of code in a massive repository can be memory-intensive, although the hibernation feature mitigates this.
- **Binary Files:** It only works effectively on text files.

## Further plans
- Improved visualization of the generated matrices.
- Deeper integration with team structure (mapping authors to teams).
