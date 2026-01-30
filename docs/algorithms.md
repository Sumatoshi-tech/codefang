# Core Algorithms & Data Structures

Codefang is built to handle massive repositories (e.g., the Linux kernel, Kubernetes) with decades of history. Standard library data structures are often insufficient for this scale due to Garbage Collection (GC) overhead and memory fragmentation.

This section details the specialized algorithms and data structures implemented in Codefang.

## 1. Memory-Efficient Red-Black Trees (`pkg/rbtree`)

The `burndown` analysis requires tracking the "liveness" of millions of lines of code across thousands of commits. A naive implementation using standard pointers (`*Node`) creates millions of small objects on the heap, causing massive GC pauses.

Codefang implements a **custom Red-Black Tree** backed by a contiguous memory allocator.

### The Allocator Pattern

Instead of allocating each node separately, we allocate a large slice of `node` structs (`[]node`). Pointers are replaced by `uint32` indices into this slice.

```go
type node struct {
    item                Item
    parent, left, right uint32 // 32-bit indices, not 64-bit pointers
    color               bool
}
```

**Benefits:**
1.  **Memory Density**: 32-bit indices halve the pointer overhead compared to 64-bit pointers.
2.  **Cache Locality**: Nodes are stored contiguously in memory, improving CPU cache hit rates during traversal.
3.  **Zero GC Overhead**: The GC only sees one large slice, not millions of tiny objects.
4.  **Hibernation**: The entire tree can be serialized to disk simply by writing the slice byte-for-byte, enabling "hibernation" of analysis states.

![RBTree Memory Model](diagrams/rbtree_memory.puml)

*Note: The diagram above is defined in PlantUML.*

### Hibernation & Booting

To support analysis of repositories larger than available RAM, the `Allocator` supports `Hibernate()`:
1.  The active `storage` slice is compressed (using varint encoding for gaps).
2.  The data is written to disk or held in a compressed in-memory buffer.
3.  When needed, `Boot()` decompresses the structure, restoring the exact state of the tree.

**Real-World Impact**: In benchmarks against the standard `hercules`, this approach reduced memory usage by **~40%** and improved load times for the Linux kernel history by **~60%**.

## 2. Topological Sorting & String Interning (`pkg/toposort`)

Dependency analysis involves graph operations on thousands of files. String comparisons are slow and consume memory.

### Interned Graph

Codefang uses a **Symbol Table** to map file paths (strings) to `int` IDs. The core graph algorithms (`IntGraph`) operate purely on integers.

**Example Walkthrough:**

1.  **AddNode("pkg/fmt")**:
    *   Check Symbol Table.
    *   If new, assign ID `42`.
    *   Store `42 -> "pkg/fmt"` and `"pkg/fmt" -> 42`.
2.  **AddEdge("pkg/main", "pkg/fmt")**:
    *   Resolve IDs: `10 -> 42`.
    *   Add edge in `IntGraph` using adjacency list: `nodes[10].append(42)`.

### Cycle Detection

The `FindCycle` method uses Depth-First Search (DFS) with a recursion stack tracker to identify circular dependencies. This is critical for the `imports` analyzer to detect architectural violations.

**Example:**
If `A -> B -> C -> A`, Codefang reports a cycle: `["pkg/a", "pkg/b", "pkg/c"]`.

## 3. Universal AST Diffing (`pkg/uast`)

To understand *how* code changes, Codefang performs structural diffing on UASTs, not just line-based text diffing.

### The Algorithm

1.  **Node Matching**: Nodes are matched based on `(Type, Token, Position)`.
2.  **Child Alignment**: A simplified version of the GumTree algorithm is used to align children of matching nodes.
3.  **Change Classification**:
    *   **Added**: Node exists in `After` but not `Before`.
    *   **Removed**: Node exists in `Before` but not `After`.
    *   **Modified**: Node exists in both but attributes (e.g., variable name) differ.

**Why this matters?**
Traditional diffs say:
```diff
- var x = 10
+ var y = 10
```
Codefang says:
`VariableDeclaration(x)` changed to `VariableDeclaration(y)` (Rename Refactoring).
