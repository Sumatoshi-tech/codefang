# Analysis Engine & Metrics

Codefang's power comes from its diverse set of analyzers. This document details the methodology and mathematical foundations of the core analyzers, with concrete examples of their usage and interpretation.

## The Analyzer Pipeline

The analysis engine (`pkg/analyzers`) uses a **Factory** pattern to orchestrate execution. Crucially, it employs a **Multi-Pass Optimization**.

### Visitor Pattern Optimization

Many analyzers (Complexity, Halstead, Shotness) need to traverse the AST. Naively running them sequentially would require $N$ traversals for $N$ analyzers.

Codefang uses a `MultiAnalyzerTraverser`:
1.  It performs a **single** depth-first traversal of the UAST.
2.  At each node, it invokes the `VisitNode` method of **all** registered visitor-based analyzers.
3.  This reduces the complexity from $O(N \times M)$ to $O(M)$, where $M$ is the number of AST nodes.

![Pipeline Diagram](diagrams/pipeline.puml)

---

## 1. Cyclomatic Complexity (`complexity`)

Measures the number of linearly independent paths through a program's source code.

*   **Formula**: $M = E - N + 2P$
    *   $E$: Number of edges in the control flow graph.
    *   $N$: Number of nodes.
    *   $P$: Number of connected components.
*   **Interpretation**:
    *   **1-10**: Simple, low risk.
    *   **11-20**: Moderate risk.
    *   **21-50**: High risk.
    *   **>50**: Untestable code.

### Case Study: The "God Function"

**High Complexity Example:**

```go
func processOrder(order Order) error {
    if order.Status == "NEW" {
        if order.Value > 1000 {
            // ... logic
        } else {
            // ... logic
        }
    } else if order.Status == "PENDING" {
        for _, item := range order.Items {
            if item.IsStocked {
                 // ... logic
            }
        }
    }
    // ... 50 more lines of nested ifs
}
```

**Codefang Output:**

```json
{
  "total_complexity": 24,
  "cognitive_complexity": 35,
  "message": "Critical: Function 'processOrder' exceeds complexity threshold."
}
```

**Refactoring Strategy:**
Split the function by responsibility. Move `NEW` status handling to `processNewOrder` and `PENDING` to `processPendingOrder`. This reduces the complexity of each individual function to manageable levels (< 10).

## 2. Cohesion (`cohesion`)

Measures how well the methods of a class belong together (LCOM - Lack of Cohesion of Methods).

*   **Concept**: In a cohesive class, methods access the same instance variables.
*   **Logic**: High LCOM indicates the class is doing too many unrelated things and should likely be split (God Object anti-pattern).

### Case Study: The "Mixed Utility" Class

**Low Cohesion Example (LCOM > 1):**

```java
class UserManager {
    private DBConnection db;
    private EmailService email;
    
    // Uses 'db' only
    public void createUser(String name) {
        db.insert(name);
    }

    // Uses 'email' only - unrelated to DB logic
    public void sendNewsletter(String subject) {
        email.send(subject);
    }
}
```

**Codefang Output:**

```json
{
  "cohesion_score": 0.2,
  "lcom": 1.5,
  "message": "Poor cohesion. Consider splitting 'UserManager'."
}
```

**Refactoring Strategy:**
Split `UserManager` into `UserRepository` (handles DB) and `NotificationService` (handles Email).

## 3. Halstead Complexity (`halstead`)

Measures the computational complexity based on operators and operands. This metric gives a sense of the "volume" and "effort" required to understand the code, independent of control flow.

*   **Vocabulary ($n$)**: $n_1$ (unique operators) + $n_2$ (unique operands).
*   **Length ($N$)**: $N_1$ (total operators) + $N_2$ (total operands).
*   **Volume ($V$)**: $N \times \log_2(n)$. Represents the information content.
*   **Difficulty ($D$)**: $(n_1 / 2) \times (N_2 / n_2)$. Represents the error-proneness.
*   **Effort ($E$)**: $D \times V$. Correlates with implementation time.

## 4. Burndown Analysis (`burndown`)

Tracks the survival rate of code over time.

*   **Concept**: Every line of code is born in a commit. It "dies" when it is modified or deleted.
*   **Metric**: Code churn and code age.
*   **Insight**: If a project has 50% of its code from 5 years ago, it might be stable (or stagnant). If 90% of code is < 1 month old, it is in rapid development (or chaotic rewrite).

**Example Output (Table):**

| Date       | Alive Lines | Survival Rate |
|------------|-------------|---------------|
| 2024-01-01 | 10,000      | 100%          |
| 2024-06-01 | 8,500       | 85%           |
| 2025-01-01 | 6,000       | 60%           |

## 5. Sentiment Analysis (`sentiment`)

Analyzes the emotional tone of comments.

*   **Logic**: Uses a lexicon-based approach to score comments as Positive, Negative, or Neutral.
*   **Use Case**: Clusters of negative comments ("this hack", "fixme", "stupid") often correlate with technical debt hot-spots.
