# Usage Guide & Scenarios

Codefang is designed to solve specific engineering problems. This guide walks through real-world scenarios where Codefang shines.

## Scenario 1: The "Legacy Monolith" Refactor

**Context:** You have inherited a 10-year-old Go codebase. It's brittle. Developers are afraid to touch `core/billing.go` because "it breaks everything."

**The Goal:** Identify the most dangerous coupling and find safe starting points for refactoring.

**Step 1: Identify Coupling (The "Blast Radius")**
Run the coupling analysis to see what files change together.

```bash
codefang history -a couples --head . > coupling.csv
```

**Output Interpretation:**
You see that `core/billing.go` has a coupling strength of 85% with `utils/logger.go` and `api/handlers.go`.
*   **Insight**: Changing billing logic almost always requires changing the logger. This suggests tight coupling and likely a violation of the Dependency Inversion Principle.

**Step 2: Check Complexity**
Is `core/billing.go` hard to read?

```bash
uast parse core/billing.go | codefang analyze -a complexity
```

**Output:**
```
Complexity: 45 (CRITICAL)
Cognitive Complexity: 60
```
*   **Insight**: The code is not just coupled; it's also incredibly complex.

**Step 3: Action Plan**
1.  Decouple the logger (interface injection).
2.  Write unit tests for the 45 independent paths (detected by complexity).
3.  Refactor.

## Scenario 2: The "Knowledge Silo" Risk

**Context:** Your lead developer, Alice, is leaving the company. You need to know what knowledge is walking out the door.

**The Goal:** Find files where Alice is the *only* expert.

**Step 1: Developer Expertise Map**

```bash
codefang history -a file-history .
```

**Output:**
```
File: pkg/crypto/hashing.go
  commits: ["abc1234", "def5678"]
  people: {1:[10,5,0], 2:[50,0,0]} # ID:[Added, Removed, Changed]
```

**Insight:**
*   Developer `2` (Alice) added 50 lines, while Developer `1` (Bob) only added 10.
*   `pkg/crypto/hashing.go` is a major risk. If Alice leaves, knowledge is lost.
*   **Action**: Schedule immediate knowledge transfer sessions for the `crypto` package.

> **Performance Note:** On massive repositories like Kubernetes (100k+ commits), full history analysis can take significant time. Use `--head` for quick snapshots or `--since` (planned feature) to limit the range.

## Scenario 3: AI-Assisted Code Review

**Context:** You are using an LLM (Claude/ChatGPT) to review pull requests. You want to ground the AI's opinion in data.

**The Goal:** Provide the AI with structural metrics so it doesn't just guess.

**Step 1: Generate Context**
The developer submits a PR changing `service.py`. You run Codefang in your CI/CD pipeline.

```bash
# In CI pipeline
uast parse service.py | codefang analyze -f json > metrics.json
```

**Step 2: Prompt the AI**
"Review this code. Note that Codefang reports a Cyclomatic Complexity of 18 (Threshold is 15). Is this complexity justified by the business logic, or can it be simplified?"

**Insight:**
The AI now has an objective fact ("Complexity is 18") to anchor its critique, reducing hallucinations and making the feedback more actionable.
