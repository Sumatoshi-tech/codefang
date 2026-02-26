package burndown

import (
	"testing"
)

// Test constants for node pool tests.
const (
	// poolTestLength is the node length used in pool tests.
	poolTestLength = 42

	// poolTestValue is the node value used in pool tests.
	poolTestValue = 7

	// poolTestPriority is the node priority used in pool tests.
	poolTestPriority = 99

	// poolTestSize is the node size used in pool tests.
	poolTestSize = 42

	// poolTestBulkCount is the number of nodes for bulk pool tests.
	poolTestBulkCount = 100

	// poolTestReplaceCount is the number of Replace operations for pool reuse tests.
	poolTestReplaceCount = 1000

	// poolTestFileLen is the initial file length for pool integration tests.
	poolTestFileLen = 1000

	// poolTestInsLen is the insertion length for pool Replace tests.
	poolTestInsLen = 5

	// poolTestDelLen is the deletion length for pool Replace tests.
	poolTestDelLen = 3

	// poolTestTimeMod is the time modulo for pool Replace tests.
	poolTestTimeMod = 50

	// poolTestPosMod is the position modulo for pool Replace tests.
	poolTestPosMod = 900

	// poolTestPosMultiplier is the position multiplier for pseudo-random placement.
	poolTestPosMultiplier = 31

	// poolTestSubtreeNodes is the number of nodes in a test subtree.
	poolTestSubtreeNodes = 3
)

// TestNodePool_AcquireRelease verifies basic acquire and release cycle.
func TestNodePool_AcquireRelease(t *testing.T) {
	t.Parallel()

	var pool nodePool

	// Acquire a new node from empty pool.
	node := pool.acquire()

	if node == nil {
		t.Fatal("acquire returned nil")
	}

	// Set fields to verify they get zeroed on release.
	node.length = poolTestLength
	node.value = poolTestValue
	node.priority = poolTestPriority
	node.size = poolTestSize

	pool.release(node)

	// Pool should have one free node.
	if len(pool.free) != 1 {
		t.Errorf("expected 1 free node, got %d", len(pool.free))
	}
}

// TestNodePool_ReleaseZerosFields verifies that release zeros all node fields.
func TestNodePool_ReleaseZerosFields(t *testing.T) {
	t.Parallel()

	var pool nodePool

	node := pool.acquire()
	node.length = poolTestLength
	node.value = poolTestValue
	node.priority = poolTestPriority
	node.size = poolTestSize
	node.left = node  // Self-reference to test pointer clearing.
	node.right = node // Self-reference to test pointer clearing.

	pool.release(node)

	// Re-acquire — should be the same node.
	reacquired := pool.acquire()

	if reacquired.length != 0 {
		t.Errorf("length not zeroed: got %d", reacquired.length)
	}

	if reacquired.value != 0 {
		t.Errorf("value not zeroed: got %d", reacquired.value)
	}

	if reacquired.priority != 0 {
		t.Errorf("priority not zeroed: got %d", reacquired.priority)
	}

	if reacquired.size != 0 {
		t.Errorf("size not zeroed: got %d", reacquired.size)
	}

	if reacquired.left != nil {
		t.Error("left not nil after release")
	}

	if reacquired.right != nil {
		t.Error("right not nil after release")
	}
}

// TestNodePool_Reuse verifies that acquire after release returns the same pointer.
func TestNodePool_Reuse(t *testing.T) {
	t.Parallel()

	var pool nodePool

	node := pool.acquire()

	pool.release(node)

	reacquired := pool.acquire()

	if reacquired != node {
		t.Error("expected reuse of released node pointer")
	}
}

// TestNodePool_GrowsOnDemand verifies that pool allocates new nodes when free-list is empty.
func TestNodePool_GrowsOnDemand(t *testing.T) {
	t.Parallel()

	var pool nodePool

	nodes := make([]*treapNode, poolTestBulkCount)
	for i := range poolTestBulkCount {
		nodes[i] = pool.acquire()

		if nodes[i] == nil {
			t.Fatalf("acquire returned nil at index %d", i)
		}
	}

	// All nodes should be distinct.
	seen := make(map[*treapNode]bool, poolTestBulkCount)

	for i, n := range nodes {
		if seen[n] {
			t.Errorf("duplicate node pointer at index %d", i)
		}

		seen[n] = true
	}
}

// TestNodePool_ReleaseSubtree verifies that an entire subtree is released to the pool.
func TestNodePool_ReleaseSubtree(t *testing.T) {
	t.Parallel()

	var pool nodePool

	// Build a small tree: root with left and right children.
	root := pool.acquire()
	root.left = pool.acquire()
	root.right = pool.acquire()

	pool.releaseSubtree(root)

	if len(pool.free) != poolTestSubtreeNodes {
		t.Errorf("expected %d free nodes after releasing subtree, got %d",
			poolTestSubtreeNodes, len(pool.free))
	}
}

// TestNodePool_ReleaseNil verifies that releasing nil does not panic.
func TestNodePool_ReleaseNil(t *testing.T) {
	t.Parallel()

	var pool nodePool

	// Must not panic.
	pool.release(nil)
	pool.releaseSubtree(nil)

	if len(pool.free) != 0 {
		t.Errorf("expected 0 free nodes after nil releases, got %d", len(pool.free))
	}
}

// TestReplace_PoolReuse verifies that Replace reuses pooled nodes after warmup.
func TestReplace_PoolReuse(t *testing.T) {
	t.Parallel()

	tl := NewTreapTimeline(0, poolTestFileLen)

	// Perform many replaces — pool should absorb and reuse nodes.
	for i := range poolTestReplaceCount {
		time := TimeKey(i % poolTestTimeMod)

		pos := (i * poolTestPosMultiplier) % poolTestPosMod
		if pos < 0 {
			pos = -pos
		}

		tl.Replace(pos, poolTestDelLen, poolTestInsLen, time)
	}

	tl.Validate()

	// Verify pool has accumulated free nodes.
	freeCount := len(tl.pool.free)

	if freeCount == 0 {
		t.Error("expected pool to have free nodes after many replacements")
	}
}

// TestCloneDeep_IndependentPool verifies that CloneDeep produces an independent copy.
func TestCloneDeep_IndependentPool(t *testing.T) {
	t.Parallel()

	tl := NewTreapTimeline(0, poolTestFileLen)
	tl.Replace(poolTestInsLen, 0, poolTestInsLen, coalesceTestTimeA)

	clone := tl.CloneDeep()

	// Modify clone.
	clone.Replace(0, poolTestDelLen, poolTestInsLen, coalesceTestTimeB)
	clone.Validate()

	// Original should be unchanged.
	tl.Validate()

	if tl.Len() == clone.Len() {
		t.Error("expected different Len after modifying clone")
	}
}

// TestErase_PopulatesPool verifies that Erase releases nodes to the pool.
func TestErase_PopulatesPool(t *testing.T) {
	t.Parallel()

	tl := NewTreapTimeline(0, poolTestFileLen)
	tl.Replace(0, 0, poolTestInsLen, coalesceTestTimeA)

	nodesBefore := tl.Nodes()

	tl.Erase()

	// Pool should have received all nodes.
	if len(tl.pool.free) < nodesBefore {
		t.Errorf("expected at least %d free nodes after Erase, got %d",
			nodesBefore, len(tl.pool.free))
	}

	// Verify timeline is empty.
	if tl.Len() != 0 {
		t.Errorf("expected Len 0 after Erase, got %d", tl.Len())
	}

	// New operations should reuse pooled nodes.
	tl.root = tl.merge(tl.newNode(poolTestFileLen, 0), tl.newNode(0, TreeEnd))
	tl.totalLength = poolTestFileLen
	tl.Validate()
}

// TestReconstruct_UsesPool verifies that Reconstruct releases old nodes and allocates from pool.
func TestReconstruct_UsesPool(t *testing.T) {
	t.Parallel()

	tl := NewTreapTimeline(0, poolTestFileLen)

	// Create extra fragmentation so old tree has more nodes than the rebuilt one.
	for i := range poolTestBulkCount {
		pos := (i * poolTestPosMultiplier) % poolTestPosMod

		tl.Replace(pos, poolTestDelLen, poolTestInsLen, TimeKey(i%poolTestTimeMod))
	}

	nodesBefore := tl.Nodes()
	lines := tl.Flatten()

	tl.Reconstruct(lines)
	tl.Validate()

	nodesAfter := tl.Nodes()

	if tl.Len() != len(lines) {
		t.Errorf("Len after Reconstruct: got %d, want %d", tl.Len(), len(lines))
	}

	// If old tree had more nodes than new tree, pool should have free nodes.
	if nodesBefore > nodesAfter && len(tl.pool.free) == 0 {
		t.Error("expected pool to have free nodes after Reconstruct with fewer segments")
	}
}
