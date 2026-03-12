// Package shotness provides shotness functionality.
package shotness

import (
	"context"
	"unicode/utf8"

	"github.com/sergi/go-diff/diffmatchpatch"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/plumbing"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/safeconv"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// Analyzer measures co-change frequency of code entities across commit history.
type Analyzer struct {
	*analyze.BaseHistoryAnalyzer[*ComputedMetrics]

	FileDiff  *plumbing.FileDiffAnalyzer
	UAST      *plumbing.UASTChangesAnalyzer
	nodes     map[string]*nodeShotness
	files     map[string]map[string]*nodeShotness
	merges    *analyze.MergeTracker
	DSLStruct string
	DSLName   string
}

// NodeDelta represents a single node's contribution in one commit.
type NodeDelta struct {
	// Summary identifies the node (type, name, file).
	Summary NodeSummary
	// CountDelta is the change count increment (1 for first touch in a commit, 0 otherwise).
	CountDelta int
}

// CommitData is the per-commit TC payload emitted by Consume().
// It captures per-commit node touch deltas; coupling pairs are derived
// inline by the aggregator from NodesTouched keys to avoid O(N²) allocation.
type CommitData struct {
	// NodesTouched maps node key to its delta for this commit.
	NodesTouched map[string]NodeDelta
}

// CommitSummary holds per-commit summary data for timeseries output.
type CommitSummary struct {
	NodesTouched  int `json:"nodes_touched"`
	CouplingPairs int `json:"coupling_pairs"`
}

// toMap returns a JSON-friendly representation of the commit summary.
// This is the single source of truth for serializing CommitSummary to map[string]any.
func (cs *CommitSummary) toMap() map[string]any {
	return map[string]any{
		"nodes_touched":  cs.NodesTouched,
		"coupling_pairs": cs.CouplingPairs,
	}
}

// TickData is the per-tick aggregated payload stored in analyze.TICK.Data.
type TickData struct {
	// Nodes maps node key to accumulated node data.
	Nodes map[string]*nodeShotnessData
	// CommitStats holds per-commit summary data for timeseries output.
	CommitStats map[string]*CommitSummary
}

// nodeShotnessData is the aggregator's per-node accumulation state.
type nodeShotnessData struct {
	Summary NodeSummary
	Count   int
	Couples map[string]int
}

type nodeShotness struct {
	Summary NodeSummary
	Count   int
}

// NodeSummary holds identifying information for a code node.
type NodeSummary struct {
	Type string
	Name string
	File string
}

func (ns *NodeSummary) String() string {
	return ns.Type + "_" + ns.Name + "_" + ns.File
}

const (
	// ConfigShotnessDSLStruct is the configuration key for the DSL structure expression.
	ConfigShotnessDSLStruct = "Shotness.DSLStruct"
	// ConfigShotnessDSLName is the configuration key for the DSL name expression.
	ConfigShotnessDSLName = "Shotness.DSLName"
	// DefaultShotnessDSLStruct is the default DSL expression for selecting code structures.
	DefaultShotnessDSLStruct = "filter(.roles has \"Function\")"
	// DefaultShotnessDSLName is the default DSL expression for extracting names.
	DefaultShotnessDSLName = ".props.name"

	// maxCouplingNodes caps the number of touched nodes per commit for coupling
	// pair computation. With N nodes, coupling generates N*(N-1)/2 pairs — at
	// N=500 that's 124,750 pairs (~12 MB of aggregator map entries). Beyond this
	// threshold the commit is a mass refactor and coupling signal is noise; we
	// still record the node touches and coupling pair count but skip the O(N²)
	// map updates.
	maxCouplingNodes = 500

	// combinatorialPairDivisor is the divisor in the combination formula
	// C(n,2) = n*(n-1)/2 for computing the number of coupling pairs.
	combinatorialPairDivisor = 2

	// minCouplingNodes is the minimum number of touched nodes required
	// to form at least one coupling pair.
	minCouplingNodes = 2
)

// NewAnalyzer creates a new shotness analyzer.
func NewAnalyzer() *Analyzer {
	a := &Analyzer{}
	a.BaseHistoryAnalyzer = &analyze.BaseHistoryAnalyzer[*ComputedMetrics]{
		Desc: analyze.Descriptor{
			ID:          "history/shotness",
			Description: "Structural hotness - a fine-grained alternative to --couples.",
			Mode:        analyze.ModeHistory,
		},
		Sequential:         false,
		CPUHeavyFlag:       true,
		EstimatedStateSize: workingStateSize,
		EstimatedTCSize:    avgTCSize,
		ConfigOptions: []pipeline.ConfigurationOption{
			{
				Name:        ConfigShotnessDSLStruct,
				Description: "UAST DSL query to use for filtering the nodes.",
				Flag:        "shotness-dsl-struct",
				Type:        pipeline.StringConfigurationOption,
				Default:     DefaultShotnessDSLStruct,
			},
			{
				Name:        ConfigShotnessDSLName,
				Description: "UAST DSL query to determine the names of the filtered nodes.",
				Flag:        "shotness-dsl-name",
				Type:        pipeline.StringConfigurationOption,
				Default:     DefaultShotnessDSLName,
			},
		},
		ComputeMetricsFn: analyze.SafeMetricComputer(ComputeAllMetrics, &ComputedMetrics{}),
		AggregatorFn:     newAggregator,
	}

	a.TicksToReportFn = ticksToReport
	a.SerializeTextFn = a.generateText
	a.SerializePlotFn = a.generatePlot

	return a
}

// Name returns the analyzer name.
func (s *Analyzer) Name() string {
	return "Shotness"
}

// Configure sets up the analyzer with the provided facts.
func (s *Analyzer) Configure(facts map[string]any) error {
	if val, exists := facts[ConfigShotnessDSLStruct].(string); exists {
		s.DSLStruct = val
	} else {
		s.DSLStruct = DefaultShotnessDSLStruct
	}

	if val, exists := facts[ConfigShotnessDSLName].(string); exists {
		s.DSLName = val
	} else {
		s.DSLName = DefaultShotnessDSLName
	}

	return nil
}

// Initialize prepares the analyzer for processing commits.
func (s *Analyzer) Initialize(_ *gitlib.Repository) error {
	s.nodes = map[string]*nodeShotness{}
	s.files = map[string]map[string]*nodeShotness{}
	s.merges = analyze.NewMergeTracker()

	return nil
}

// shouldConsumeCommit checks whether this commit should be processed,
// implementing OneShotMergeProcessor logic for merge commits.
func (s *Analyzer) shouldConsumeCommit(commit analyze.CommitLike) bool {
	if commit.NumParents() <= 1 {
		return true
	}

	return !s.merges.SeenOrAdd(commit.Hash())
}

// addNode registers or increments a node in the analyzer's tracking state.
func (s *Analyzer) addNode(name string, n *node.Node, fileName string, allNodes map[string]bool) {
	nodeSummary := NodeSummary{
		Type: string(n.Type),
		Name: name,
		File: fileName,
	}
	key := nodeSummary.String()
	exists := allNodes[key]
	allNodes[key] = true

	var count int
	if ns := s.nodes[key]; ns != nil {
		count = ns.Count
	}

	if count == 0 {
		s.nodes[key] = &nodeShotness{Summary: nodeSummary, Count: 1}

		fmap := s.files[nodeSummary.File]
		if fmap == nil {
			fmap = map[string]*nodeShotness{}
		}

		fmap[key] = s.nodes[key]
		s.files[nodeSummary.File] = fmap
	} else if !exists {
		s.nodes[key].Count = count + 1
	}
}

// handleDeletion removes all nodes and file entries associated with a deleted file.
func (s *Analyzer) handleDeletion(change uast.Change) {
	for key := range s.files[change.Change.From.Name] {
		delete(s.nodes, key)
	}

	delete(s.files, change.Change.From.Name)
}

// handleInsertion extracts nodes from a newly inserted file and registers them.
func (s *Analyzer) handleInsertion(change uast.Change, allNodes map[string]bool) {
	toName := change.Change.To.Name

	nodes, err := s.extractNodes(change.After)
	if err != nil {
		return
	}

	for name, n := range nodes {
		s.addNode(name, n, toName, allNodes)
	}
}

// handleModification processes a file modification, including renames and diff-based node tracking.
func (s *Analyzer) handleModification(
	change uast.Change,
	diffs map[string]pkgplumbing.FileDiffData,
	allNodes map[string]bool,
) {
	toName := change.Change.To.Name

	if change.Change.From.Name != toName {
		s.applyRename(change.Change.From.Name, toName)
	}

	nodesBefore, err := s.extractNodes(change.Before)
	if err != nil {
		return
	}

	nodesAfter, err := s.extractNodes(change.After)
	if err != nil {
		return
	}

	diff, ok := diffs[toName]
	if !ok {
		return
	}

	s.applyDiffEdits(toName, nodesBefore, nodesAfter, diff, allNodes)
}

// applyDiffEdits walks the diff edits and records which nodes were touched.
func (s *Analyzer) applyDiffEdits(
	toName string,
	nodesBefore, nodesAfter map[string]*node.Node,
	diff pkgplumbing.FileDiffData,
	allNodes map[string]bool,
) {
	reversedNodesBefore := reverseNodeMap(nodesBefore)
	reversedNodesAfter := reverseNodeMap(nodesAfter)
	line2nodeBefore := genLine2Node(nodesBefore, diff.OldLinesOfCode)
	line2nodeAfter := genLine2Node(nodesAfter, diff.NewLinesOfCode)

	var lineNumBefore, lineNumAfter int

	for _, edit := range diff.Diffs {
		size := utf8.RuneCountInString(edit.Text)

		switch edit.Type {
		case diffmatchpatch.DiffDelete:
			s.recordTouchedNodes(line2nodeBefore, reversedNodesBefore, lineNumBefore, size, toName, allNodes)
			lineNumBefore += size
		case diffmatchpatch.DiffInsert:
			s.recordTouchedNodes(line2nodeAfter, reversedNodesAfter, lineNumAfter, size, toName, allNodes)
			lineNumAfter += size
		case diffmatchpatch.DiffEqual:
			lineNumBefore += size
			lineNumAfter += size
		}
	}
}

// recordTouchedNodes marks nodes touched by a diff hunk spanning [startLine, startLine+size).
func (s *Analyzer) recordTouchedNodes(
	line2node [][]*node.Node,
	reversed map[string]string,
	startLine, size int,
	fileName string,
	allNodes map[string]bool,
) {
	for l := startLine; l < startLine+size; l++ {
		if l < len(line2node) {
			for _, n := range line2node[l] {
				if id, ok := reversed[n.ID]; ok {
					s.addNode(id, n, fileName, allNodes)
				}
			}
		}
	}
}

// applyRename updates internal state when a file is renamed from oldName to newName.
func (s *Analyzer) applyRename(oldName, newName string) {
	oldFile := s.files[oldName]
	newFile := map[string]*nodeShotness{}

	s.files[newName] = newFile

	for oldKey, ns := range oldFile {
		ns.Summary.File = newName
		newKey := ns.Summary.String()
		newFile[newKey] = ns

		s.nodes[newKey] = ns
		delete(s.nodes, oldKey)
	}

	delete(s.files, oldName)
}

// genLine2Node builds a mapping from line numbers to UAST nodes that span each line.
func genLine2Node(nodes map[string]*node.Node, linesNum int) [][]*node.Node {
	res := make([][]*node.Node, linesNum)

	for _, uastNode := range nodes {
		pos := uastNode.Pos
		if pos == nil {
			continue
		}

		startLine := safeconv.MustUintToInt(pos.StartLine)

		endLine := resolveEndLine(uastNode, pos)

		for line := startLine; line <= endLine; line++ {
			if line > 0 && line <= len(res) {
				lineNodes := res[line-1]
				if lineNodes == nil {
					lineNodes = []*node.Node{}
				}

				lineNodes = append(lineNodes, uastNode)
				res[line-1] = lineNodes
			}
		}
	}

	return res
}

// resolveEndLine determines the effective end line of a UAST node. If the node
// has an explicit end line greater than its start line, that value is used.
// Otherwise, the function walks the node's children to find the maximum line.
func resolveEndLine(uastNode *node.Node, pos *node.Positions) int {
	if pos.EndLine > pos.StartLine {
		return safeconv.MustUintToInt(pos.EndLine)
	}

	endLine := safeconv.MustUintToInt(pos.StartLine)

	uastNode.VisitPreOrder(func(child *node.Node) {
		if child.Pos == nil {
			return
		}

		candidate := safeconv.MustUintToInt(child.Pos.StartLine)
		if child.Pos.EndLine > child.Pos.StartLine {
			candidate = safeconv.MustUintToInt(child.Pos.EndLine)
		}

		if candidate > endLine {
			endLine = candidate
		}
	})

	return endLine
}

// Consume processes a single commit with the provided dependency results.
func (s *Analyzer) Consume(ctx context.Context, ac *analyze.Context) (analyze.TC, error) {
	if !s.shouldConsumeCommit(ac.Commit) {
		return analyze.TC{}, nil
	}

	changesList := s.UAST.Changes(ctx)
	diffs := s.FileDiff.FileDiffs
	allNodes := map[string]bool{}

	for change := range changesList {
		switch {
		case change.After == nil:
			s.handleDeletion(change)
		case change.Before == nil:
			s.handleInsertion(change, allNodes)
		default:
			s.handleModification(change, diffs, allNodes)
		}
	}

	cd := s.buildCommitData(allNodes)
	if cd == nil {
		return analyze.TC{}, nil
	}

	return analyze.TC{
		Data:       cd,
		CommitHash: ac.Commit.Hash(),
	}, nil
}

// buildCommitData extracts per-commit deltas from the set of touched nodes.
func (s *Analyzer) buildCommitData(allNodes map[string]bool) *CommitData {
	if len(allNodes) == 0 {
		return nil
	}

	nodesTouched := make(map[string]NodeDelta, len(allNodes))

	for key := range allNodes {
		ns := s.nodes[key]
		if ns == nil {
			continue
		}

		nodesTouched[key] = NodeDelta{
			Summary:    ns.Summary,
			CountDelta: 1,
		}
	}

	return &CommitData{
		NodesTouched: nodesTouched,
	}
}

// Fork creates a copy of the analyzer for parallel processing.
// Each fork gets independent mutable state while sharing read-only dependencies.
func (s *Analyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		clone := &Analyzer{
			FileDiff:  &plumbing.FileDiffAnalyzer{},
			UAST:      &plumbing.UASTChangesAnalyzer{},
			DSLStruct: s.DSLStruct,
			DSLName:   s.DSLName,
		}
		// Initialize independent state for each fork.
		clone.nodes = make(map[string]*nodeShotness)
		clone.files = make(map[string]map[string]*nodeShotness)
		clone.merges = analyze.NewMergeTracker()

		res[i] = clone
	}

	return res
}

// Merge combines results from forked analyzer branches.
func (s *Analyzer) Merge(branches []analyze.HistoryAnalyzer) {
	for _, branch := range branches {
		other, ok := branch.(*Analyzer)
		if !ok {
			continue
		}

		s.mergeNodes(other.nodes)
		// Merge trackers are not combined: each fork processes a disjoint
		// subset of commits, so merge dedup state doesn't need unification.
	}

	// Rebuild files map from merged nodes.
	s.rebuildFilesMap()
}

// mergeNodes combines node data from another analyzer branch.
// Only node counts and summaries are merged; coupling data is accumulated
// independently by the aggregator from per-commit TCs.
func (s *Analyzer) mergeNodes(other map[string]*nodeShotness) {
	for key, otherNode := range other {
		if existing := s.nodes[key]; existing != nil {
			existing.Count += otherNode.Count
		} else {
			s.nodes[key] = &nodeShotness{
				Summary: otherNode.Summary,
				Count:   otherNode.Count,
			}
		}
	}
}

// DiscardState clears cumulative node coupling state. In streaming timeseries
// mode, per-commit data is already captured in the TC; the accumulated nodes
// map (which grows O(N²) with coupling pairs) is only needed for the final
// report and can be discarded between chunks.
func (s *Analyzer) DiscardState() {
	s.nodes = map[string]*nodeShotness{}
	s.files = map[string]map[string]*nodeShotness{}
}

// NeedsUAST returns true to enable the UAST pipeline.
func (s *Analyzer) NeedsUAST() bool { return true }

// SnapshotPlumbing captures the current plumbing output state for parallel execution.
func (s *Analyzer) SnapshotPlumbing() analyze.PlumbingSnapshot {
	return plumbing.Snapshot{
		UASTChanges: s.UAST.TransferChanges(),
		FileDiffs:   s.FileDiff.FileDiffs,
	}
}

// ApplySnapshot restores plumbing state from a previously captured snapshot.
func (s *Analyzer) ApplySnapshot(snap analyze.PlumbingSnapshot) {
	ss, ok := snap.(plumbing.Snapshot)
	if !ok {
		return
	}

	s.UAST.SetChanges(ss.UASTChanges)
	s.FileDiff.FileDiffs = ss.FileDiffs
}

// ReleaseSnapshot releases UAST trees owned by the snapshot.
func (s *Analyzer) ReleaseSnapshot(snap analyze.PlumbingSnapshot) {
	ss, ok := snap.(plumbing.Snapshot)
	if !ok {
		return
	}

	for _, ch := range ss.UASTChanges {
		node.ReleaseTree(ch.Before)
		node.ReleaseTree(ch.After)
	}
}

// rebuildFilesMap rebuilds the files map from the nodes map.
func (s *Analyzer) rebuildFilesMap() {
	s.files = make(map[string]map[string]*nodeShotness)

	for key, ns := range s.nodes {
		fileName := ns.Summary.File
		if s.files[fileName] == nil {
			s.files[fileName] = make(map[string]*nodeShotness)
		}

		s.files[fileName][key] = ns
	}
}

// extractNodes selects structural nodes (e.g., functions) from a UAST and maps them by extracted name.
// Uses DSLStruct to find nodes and DSLName to get the display name. When multiple nodes yield
// the same name (e.g., nested functions with identical names), the last one wins—shallow-only:
// no qualified paths (e.g., Outer.inner) are built.
func (s *Analyzer) extractNodes(root *node.Node) (map[string]*node.Node, error) {
	if root == nil {
		return map[string]*node.Node{}, nil
	}

	structs, err := root.FindDSL(s.DSLStruct)
	if err != nil {
		return nil, err
	}

	res := map[string]*node.Node{}

	for _, structNode := range structs {
		// Name extraction.
		nameNodes, nameErr := structNode.FindDSL(s.DSLName)
		if nameErr == nil && len(nameNodes) > 0 {
			name := nameNodes[0].Token
			if name != "" {
				res[name] = structNode
			}
		} else if structNode.Token != "" {
			res[structNode.Token] = structNode
		}
	}

	return res, nil
}

func reverseNodeMap(nodes map[string]*node.Node) map[string]string {
	res := map[string]string{}
	for key, node := range nodes {
		res[node.ID] = key
	}

	return res
}

// ExtractCommitTimeSeries implements analyze.CommitTimeSeriesProvider.
// It extracts per-commit structural hotspot data for the unified timeseries output.
func (s *Analyzer) ExtractCommitTimeSeries(report analyze.Report) map[string]any {
	commitStats, ok := report["commit_stats"].(map[string]*CommitSummary)
	if !ok || len(commitStats) == 0 {
		return nil
	}

	result := make(map[string]any, len(commitStats))

	for hash, cs := range commitStats {
		result[hash] = cs.toMap()
	}

	return result
}
