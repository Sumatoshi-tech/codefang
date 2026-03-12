# Imports Analysis

## Preface
Modern software is built on giants' shoulders. Analyzing imports reveals the dependency structure of the project.

## Problem
- "What external libraries do we depend on?"
- "Who introduced this dependency?"
- "Is this library widely used or just used in one corner of the app?"
- "Are we accruing 'dependency debt'?"

## How analyzer solves it
The Imports analyzer extracts import statements from every file in every commit. It aggregates this usage data per developer and per language over time.

## Historical context
Dependency analysis is key to supply chain security and architecture management.

## Real world examples
- **Deprecation:** Finding all users of a library you want to deprecate.
- **License Compliance:** Checking if new dependencies are being introduced by specific developers.
- **Adoption Tracking:** Seeing how quickly a new internal library is being adopted by the team.

## How analyzer works here
1.  **Blob Processing:** Reads the file content for changed files.
2.  **Parsing:** Uses the UAST parser (or language-specific fallback) to identify `import`, `include`, or `require` statements.
3.  **Attribution:** Credits the commit author with the "usage" of those imports at that point in time.

## Limitations
- **Dynamic Imports:** Hard to track dynamic imports (e.g., Python's `__import__` with variables).
- **Build Systems:** Doesn't parse `package.json` or `go.mod` directly usually; looks at source files (imports in code).

## Further plans
- Support for package manager manifest files (pom.xml, package.json, etc.).
