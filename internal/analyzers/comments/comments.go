package comments

import (
	"encoding/json"
	"fmt"
	"io"
	"sort"

	"gopkg.in/yaml.v3"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/renderer"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/reportutil"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/terminal"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/safeconv"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// Configuration constants for comment analysis scoring.
const (
	// ScoreValue is the base penalty applied when a comment quality issue is detected.
	ScoreValue       = 0.2
	gapThresholdHigh = 2
	lenArg50         = 50
	magic0p4         = 0.4
	magic0p4_1       = 0.4
	magic0p4_2       = 0.4
	magic0p6         = 0.6
	magic0p6_1       = 0.6
	magic0p6_2       = 0.6
	magic0p8         = 0.8
	magic0p8_1       = 0.8
	magic0p8_2       = 0.8
	magic1000        = 1000
	magic3           = 3
	magic999         = 999
	unknownName      = "unknown"
)

// Name returns the analyzer name.
func (c *Analyzer) Name() string {
	return "comments"
}

// Flag returns the CLI flag for the analyzer.
func (c *Analyzer) Flag() string {
	return "comments-analysis"
}

// Description returns the analyzer description.
func (c *Analyzer) Description() string {
	return c.Descriptor().Description
}

// Descriptor returns stable analyzer metadata.
func (c *Analyzer) Descriptor() analyze.Descriptor {
	return analyze.NewDescriptor(
		analyze.ModeStatic,
		c.Name(),
		"Analyzes code comments and documentation coverage.",
	)
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (c *Analyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{}
}

// Configure configures the analyzer.
func (c *Analyzer) Configure(_ map[string]any) error {
	return nil
}

// Thresholds returns the quality thresholds for this analyzer.
func (c *Analyzer) Thresholds() analyze.Thresholds {
	return analyze.Thresholds{
		"overall_score": {
			"red":    magic0p4,
			"yellow": magic0p6,
			"green":  magic0p8,
		},
		"good_comments_ratio": {
			"red":    magic0p4_1,
			"yellow": magic0p6_1,
			"green":  magic0p8_1,
		},
		"documentation_coverage": {
			"red":    magic0p4_2,
			"yellow": magic0p6_2,
			"green":  magic0p8_2,
		},
	}
}

// Analyze performs comment analysis using default configuration.
func (c *Analyzer) Analyze(root *node.Node) (analyze.Report, error) {
	if root == nil {
		return nil, analyze.ErrNilRootNode
	}

	comments := c.findComments(root)
	functions := c.findFunctions(root)

	if len(comments) == 0 {
		return c.buildEmptyResult(), nil
	}

	config := c.DefaultConfig()
	commentDetails := c.analyzeCommentPlacement(comments, functions, config)
	metrics := c.calculateMetrics(commentDetails, functions)

	return c.buildResult(commentDetails, functions, metrics), nil
}

// FormatReport formats comment analysis results as human-readable text.
func (c *Analyzer) FormatReport(report analyze.Report, w io.Writer) error {
	section := NewReportSection(report)
	config := terminal.NewConfig()
	r := renderer.NewSectionRenderer(config.Width, false, config.NoColor)

	_, err := fmt.Fprint(w, r.Render(section))
	if err != nil {
		return fmt.Errorf("formatreport: %w", err)
	}

	return nil
}

// FormatReportJSON formats comment analysis results as JSON.
func (c *Analyzer) FormatReportJSON(report analyze.Report, w io.Writer) error {
	metrics, err := ComputeAllMetrics(report)
	if err != nil {
		metrics = &ComputedMetrics{}
	}

	jsonData, err := json.MarshalIndent(metrics, "", "  ")
	if err != nil {
		return fmt.Errorf("formatreportjson: %w", err)
	}

	_, err = fmt.Fprint(w, string(jsonData))
	if err != nil {
		return fmt.Errorf("formatreportjson: %w", err)
	}

	return nil
}

// FormatReportYAML formats comment analysis results as YAML.
func (c *Analyzer) FormatReportYAML(report analyze.Report, w io.Writer) error {
	metrics, err := ComputeAllMetrics(report)
	if err != nil {
		metrics = &ComputedMetrics{}
	}

	data, err := yaml.Marshal(metrics)
	if err != nil {
		return fmt.Errorf("formatreportyaml: %w", err)
	}

	_, err = w.Write(data)
	if err != nil {
		return fmt.Errorf("formatreportyaml: %w", err)
	}

	return nil
}

// FormatReportBinary formats comment analysis results as binary envelope.
func (c *Analyzer) FormatReportBinary(report analyze.Report, w io.Writer) error {
	metrics, err := ComputeAllMetrics(report)
	if err != nil {
		metrics = &ComputedMetrics{}
	}

	err = reportutil.EncodeBinaryEnvelope(metrics, w)
	if err != nil {
		return fmt.Errorf("formatreportbinary: %w", err)
	}

	return nil
}

// findComments finds all comment nodes using the generic traverser.
func (c *Analyzer) findComments(root *node.Node) []*node.Node {
	return c.traverser.FindNodesByType(root, []string{node.UASTComment})
}

// findFunctions finds all function/method/class nodes using the generic traverser.
func (c *Analyzer) findFunctions(root *node.Node) []*node.Node {
	functionTypes := []string{
		node.UASTFunction,
		node.UASTMethod,
		node.UASTClass,
		node.UASTInterface,
		node.UASTStruct,
	}

	return c.traverser.FindNodesByType(root, functionTypes)
}

// analyzeCommentPlacement analyzes the placement of comments relative to their targets.
func (c *Analyzer) analyzeCommentPlacement(
	comments, functions []*node.Node,
	config CommentConfig,
) []CommentDetail {
	commentBlocks := c.groupCommentsIntoBlocks(comments)

	return c.analyzeCommentBlocks(commentBlocks, functions, config)
}

// groupCommentsIntoBlocks groups consecutive comment lines into blocks.
func (c *Analyzer) groupCommentsIntoBlocks(comments []*node.Node) []CommentBlock {
	if len(comments) == 0 {
		return []CommentBlock{}
	}

	sortedComments := c.sortCommentsByLine(comments)

	return c.createCommentBlocks(sortedComments)
}

// sortCommentsByLine sorts comments by line number.
func (c *Analyzer) sortCommentsByLine(comments []*node.Node) []*node.Node {
	sortedComments := make([]*node.Node, len(comments))
	copy(sortedComments, comments)
	sort.Slice(sortedComments, func(i, j int) bool {
		if sortedComments[i].Pos == nil || sortedComments[j].Pos == nil {
			return false
		}

		return sortedComments[i].Pos.StartLine < sortedComments[j].Pos.StartLine
	})

	return sortedComments
}

// createCommentBlocks creates comment blocks from sorted comments.
func (c *Analyzer) createCommentBlocks(sortedComments []*node.Node) []CommentBlock {
	var blocks []CommentBlock

	var currentBlock CommentBlock

	for _, comment := range sortedComments {
		if comment.Pos == nil {
			continue
		}

		commentStart := safeconv.MustUintToInt(comment.Pos.StartLine)
		commentEnd := safeconv.MustUintToInt(comment.Pos.EndLine)

		if c.shouldStartNewBlock(currentBlock, commentStart) {
			blocks = c.addBlockIfValid(blocks, currentBlock)
			currentBlock = c.createNewBlock(comment, commentStart, commentEnd)
		} else {
			currentBlock = c.extendCurrentBlock(currentBlock, comment, commentEnd)
		}
	}

	return c.addBlockIfValid(blocks, currentBlock)
}

// shouldStartNewBlock determines if a new comment block should be started.
func (c *Analyzer) shouldStartNewBlock(currentBlock CommentBlock, commentStart int) bool {
	return len(currentBlock.Comments) == 0 || commentStart > currentBlock.EndLine+1
}

// addBlockIfValid adds a block to the list if it contains comments.
func (c *Analyzer) addBlockIfValid(blocks []CommentBlock, block CommentBlock) []CommentBlock {
	if len(block.Comments) > 0 {
		blocks = append(blocks, block)
	}

	return blocks
}

// createNewBlock creates a new comment block.
func (c *Analyzer) createNewBlock(comment *node.Node, startLine, endLine int) CommentBlock {
	return CommentBlock{
		Comments:  []*node.Node{comment},
		StartLine: startLine,
		EndLine:   endLine,
		FullText:  comment.Token,
	}
}

// extendCurrentBlock extends the current block with a new comment.
func (c *Analyzer) extendCurrentBlock(block CommentBlock, comment *node.Node, endLine int) CommentBlock {
	block.Comments = append(block.Comments, comment)
	block.EndLine = endLine
	block.FullText += "\n" + comment.Token

	return block
}

// analyzeCommentBlocks analyzes multiple comment blocks.
func (c *Analyzer) analyzeCommentBlocks(blocks []CommentBlock, functions []*node.Node, config CommentConfig) []CommentDetail {
	details := make([]CommentDetail, 0, len(blocks))

	for _, block := range blocks {
		blockDetails := c.analyzeCommentBlock(block, functions, config)
		details = append(details, blockDetails...)
	}

	return details
}

// analyzeCommentBlock analyzes a comment block as a single unit.
func (c *Analyzer) analyzeCommentBlock(block CommentBlock, functions []*node.Node, config CommentConfig) []CommentDetail {
	blockNode := c.createBlockNode(block)
	blockDetail := c.analyzeSingleComment(blockNode, functions, config)

	return c.createCommentDetails(block, blockDetail)
}

// createBlockNode creates a virtual comment node representing the entire block.
func (c *Analyzer) createBlockNode(block CommentBlock) *node.Node {
	return &node.Node{
		Type:  node.UASTComment,
		Token: block.FullText,
		Pos: &node.Positions{
			StartLine: safeconv.MustIntToUint(block.StartLine),
			EndLine:   safeconv.MustIntToUint(block.EndLine),
		},
	}
}

// createCommentDetails creates comment details for all comments in a block.
func (c *Analyzer) createCommentDetails(block CommentBlock, blockDetail CommentDetail) []CommentDetail {
	details := make([]CommentDetail, 0, len(block.Comments))
	for _, comment := range block.Comments {
		detail := CommentDetail{
			Type:       string(comment.Type),
			Token:      comment.Token,
			Score:      blockDetail.Score,
			IsGood:     blockDetail.IsGood,
			TargetType: blockDetail.TargetType,
			TargetName: blockDetail.TargetName,
			Position:   blockDetail.Position,
			LineNumber: safeconv.MustUintToInt(comment.Pos.StartLine),
		}
		details = append(details, detail)
	}

	return details
}

// analyzeSingleComment analyzes a single comment's placement and quality.
func (c *Analyzer) analyzeSingleComment(comment *node.Node, functions []*node.Node, config CommentConfig) CommentDetail {
	lineNumber := c.getCommentLineNumber(comment)
	detail := c.createCommentDetail(comment, lineNumber)
	target := c.findClosestTarget(comment, functions)

	if target != nil {
		c.analyzeCommentWithTarget(comment, target, config, &detail)
	} else {
		c.analyzeCommentWithoutTarget(&detail)
	}

	return detail
}

// getCommentLineNumber gets the line number of a comment.
func (c *Analyzer) getCommentLineNumber(comment *node.Node) int {
	if comment.Pos != nil {
		return safeconv.MustUintToInt(comment.Pos.StartLine)
	}

	return 0
}

// createCommentDetail creates a basic comment detail.
func (c *Analyzer) createCommentDetail(comment *node.Node, lineNumber int) CommentDetail {
	return CommentDetail{
		Type:       string(comment.Type),
		Token:      comment.Token,
		Score:      0.0,
		IsGood:     false,
		LineNumber: lineNumber,
	}
}

// AnalyzeCommentWithTarget analyzes a comment that has a target.
func (c *Analyzer) analyzeCommentWithTarget(
	comment, target *node.Node, config CommentConfig, detail *CommentDetail,
) {
	detail.TargetType = string(target.Type)
	detail.TargetName = c.extractTargetName(target)
	detail.Position = c.determinePosition(comment, target)

	if c.isCommentProperlyPlaced(comment, target) {
		detail.Score = config.RewardScore
		detail.IsGood = true
	} else {
		detail.Score = c.getPenaltyScore(target, config)
		detail.IsGood = false
	}
}

// analyzeCommentWithoutTarget analyzes a comment without a target.
func (c *Analyzer) analyzeCommentWithoutTarget(detail *CommentDetail) {
	detail.Score = -ScoreValue
	detail.IsGood = false
	detail.Position = "unassociated"
}

// getPenaltyScore gets the penalty score for a target type.
func (c *Analyzer) getPenaltyScore(target *node.Node, config CommentConfig) float64 {
	if penalty, exists := config.PenaltyScores[string(target.Type)]; exists {
		return penalty
	}

	return -0.1
}

// findClosestTarget finds the closest function/class to a comment.
func (c *Analyzer) findClosestTarget(comment *node.Node, functions []*node.Node) *node.Node {
	var closest *node.Node

	minDistance := -1

	for _, function := range functions {
		distance := c.calculateDistance(comment, function)
		if minDistance == -1 || distance < minDistance {
			minDistance = distance
			closest = function
		}
	}

	return closest
}

// calculateDistance calculates the line distance between comment and target.
func (c *Analyzer) calculateDistance(comment, target *node.Node) int {
	if comment.Pos == nil || target.Pos == nil {
		return magic999
	}

	commentEndLine := safeconv.MustUintToInt(comment.Pos.EndLine)
	targetLine := safeconv.MustUintToInt(target.Pos.StartLine)

	if commentEndLine < targetLine {
		return targetLine - commentEndLine
	}

	return magic1000 + (commentEndLine - targetLine)
}

// IsCommentProperlyPlaced checks if a comment is properly placed above its target.
func (c *Analyzer) isCommentProperlyPlaced(comment, target *node.Node) bool {
	if comment.Pos == nil || target.Pos == nil {
		return false
	}

	commentStartLine := safeconv.MustUintToInt(comment.Pos.StartLine)
	commentEndLine := safeconv.MustUintToInt(comment.Pos.EndLine)
	targetLine := safeconv.MustUintToInt(target.Pos.StartLine)

	if commentEndLine >= targetLine {
		return false
	}

	gap := targetLine - commentEndLine

	return c.isGapAcceptable(commentStartLine, commentEndLine, gap)
}

// isGapAcceptable checks if the gap between comment and target is acceptable.
func (c *Analyzer) isGapAcceptable(commentStartLine, commentEndLine, gap int) bool {
	if commentStartLine == commentEndLine {
		return gap <= gapThresholdHigh
	}

	return gap <= magic3
}

// determinePosition determines the relative position of comment to target.
func (c *Analyzer) determinePosition(comment, target *node.Node) string {
	if comment.Pos == nil || target.Pos == nil {
		return unknownName
	}

	commentEndLine := safeconv.MustUintToInt(comment.Pos.EndLine)
	targetLine := safeconv.MustUintToInt(target.Pos.StartLine)

	if commentEndLine < targetLine {
		return "above"
	}

	commentStartLine := safeconv.MustUintToInt(comment.Pos.StartLine)
	if commentStartLine > targetLine {
		return "below"
	}

	return "inline"
}

// extractTargetName extracts the name of a target node using generic extractor.
func (c *Analyzer) extractTargetName(target *node.Node) string {
	if name, ok := c.extractor.ExtractName(target, "function_name"); ok && name != "" {
		return name
	}

	if name, ok := common.ExtractFunctionName(target); ok && name != "" {
		return name
	}

	return unknownName
}

// calculateMetrics calculates overall metrics from comment details and functions.
func (c *Analyzer) calculateMetrics(details []CommentDetail, functions []*node.Node) CommentMetrics {
	metrics := CommentMetrics{
		TotalComments:       len(details),
		GoodComments:        0,
		BadComments:         0,
		OverallScore:        0.0,
		CommentDetails:      details,
		FunctionSummary:     make(map[string]FunctionInfo),
		TotalFunctions:      len(functions),
		DocumentedFunctions: 0,
	}

	c.countCommentQuality(details, &metrics)
	c.calculateOverallScore(&metrics)
	c.buildFunctionSummary(functions, details, &metrics)

	return metrics
}

// countCommentQuality counts good and bad comments.
func (c *Analyzer) countCommentQuality(details []CommentDetail, metrics *CommentMetrics) {
	for _, detail := range details {
		if detail.IsGood {
			metrics.GoodComments++
		} else {
			metrics.BadComments++
		}
	}
}

// calculateOverallScore calculates the overall comment quality score.
func (c *Analyzer) calculateOverallScore(metrics *CommentMetrics) {
	if metrics.TotalComments > 0 {
		metrics.OverallScore = float64(metrics.GoodComments) / float64(metrics.TotalComments)
	}
}

// buildFunctionSummary builds the function summary with documentation status.
func (c *Analyzer) buildFunctionSummary(functions []*node.Node, details []CommentDetail, metrics *CommentMetrics) {
	for _, function := range functions {
		funcName := c.extractTargetName(function)
		funcInfo := FunctionInfo{
			Name:       funcName,
			Type:       string(function.Type),
			HasComment: false,
		}

		if c.hasGoodComment(funcName, details) {
			funcInfo.HasComment = true
			funcInfo.CommentType = c.getCommentType(funcName, details)
			metrics.DocumentedFunctions++
		}

		metrics.FunctionSummary[funcName] = funcInfo
	}
}

// hasGoodComment checks if a function has a good comment.
func (c *Analyzer) hasGoodComment(funcName string, details []CommentDetail) bool {
	for _, detail := range details {
		if detail.TargetName == funcName && detail.IsGood {
			return true
		}
	}

	return false
}

// getCommentType gets the comment type for a function.
func (c *Analyzer) getCommentType(funcName string, details []CommentDetail) string {
	for _, detail := range details {
		if detail.TargetName == funcName && detail.IsGood {
			return detail.Type
		}
	}

	return ""
}

// getCommentMessage returns a message based on the comment quality score.
func (c *Analyzer) getCommentMessage(score float64) string {
	if score >= scoreThresholdHigh {
		return "Excellent comment quality and placement"
	}

	if score >= scoreThresholdMedium {
		return msgGoodCommentQuality
	}

	if score >= scoreThresholdLow {
		return "Fair comment quality - consider improving placement"
	}

	return "Poor comment quality - significant improvement needed"
}

// buildEmptyResult creates an empty result when no comments are found.
func (c *Analyzer) buildEmptyResult() analyze.Report {
	return common.NewResultBuilder().BuildCustomEmptyResult(map[string]any{
		"total_comments":       0,
		"good_comments":        0,
		"bad_comments":         0,
		"overall_score":        0.0,
		"total_functions":      0,
		"documented_functions": 0,
		"message":              "No comments found",
	})
}

// buildResult builds the complete analysis result.
func (c *Analyzer) buildResult(commentDetails []CommentDetail, functions []*node.Node, metrics CommentMetrics) analyze.Report {
	commentDetailsInterface := c.buildCommentDetailsInterface(commentDetails)
	detailedCommentsTable := c.buildDetailedCommentsTable(commentDetails)
	detailedFunctionsTable := c.buildDetailedFunctionsTable(functions, metrics)
	functionSummaryInterface := c.buildFunctionSummaryInterface(metrics)

	return analyze.Report{
		"total_comments":         metrics.TotalComments,
		"good_comments":          metrics.GoodComments,
		"bad_comments":           metrics.BadComments,
		"overall_score":          metrics.OverallScore,
		"total_functions":        metrics.TotalFunctions,
		"documented_functions":   metrics.DocumentedFunctions,
		"good_comments_ratio":    safeDiv(float64(metrics.GoodComments), float64(metrics.TotalComments)),
		"documentation_coverage": safeDiv(float64(metrics.DocumentedFunctions), float64(metrics.TotalFunctions)),
		"total_comment_details":  len(commentDetails),
		"comment_details":        commentDetailsInterface,
		"comments":               detailedCommentsTable,
		"functions":              detailedFunctionsTable,
		"function_summary":       functionSummaryInterface,
		"message":                c.getCommentMessage(metrics.OverallScore),
	}
}

// buildCommentDetailsInterface builds the comment details interface.
func (c *Analyzer) buildCommentDetailsInterface(commentDetails []CommentDetail) []map[string]any {
	commentDetailsInterface := make([]map[string]any, 0, len(commentDetails))
	for _, detail := range commentDetails {
		commentDetailsInterface = append(commentDetailsInterface, map[string]any{
			"type":        detail.Type,
			"token":       detail.Token,
			"position":    detail.Position,
			"score":       detail.Score,
			"is_good":     detail.IsGood,
			"target_type": detail.TargetType,
			"target_name": detail.TargetName,
			"line_number": detail.LineNumber,
		})
	}

	return commentDetailsInterface
}

// buildDetailedCommentsTable builds the detailed comments table for display.
func (c *Analyzer) buildDetailedCommentsTable(commentDetails []CommentDetail) []map[string]any {
	detailedCommentsTable := make([]map[string]any, 0, len(commentDetails))
	for _, detail := range commentDetails {
		assessment := c.getCommentAssessment(detail.IsGood)
		commentBody := c.truncateCommentBody(detail.Token)

		detailedCommentsTable = append(detailedCommentsTable, map[string]any{
			"line":       detail.LineNumber,
			"comment":    commentBody,
			"placement":  detail.Position,
			"target":     detail.TargetName,
			"assessment": assessment,
		})
	}

	return detailedCommentsTable
}

// buildDetailedFunctionsTable builds the detailed functions table for display.
func (c *Analyzer) buildDetailedFunctionsTable(functions []*node.Node, metrics CommentMetrics) []map[string]any {
	detailedFunctionsTable := make([]map[string]any, 0, len(functions))
	for _, function := range functions {
		funcName := c.extractTargetName(function)
		funcInfo := metrics.FunctionSummary[funcName]

		assessment, commentType := c.getFunctionAssessment(funcInfo)
		funcType := c.getFunctionType(function)
		lineCount := c.getFunctionLineCount(function)

		detailedFunctionsTable = append(detailedFunctionsTable, map[string]any{
			"function":   funcName,
			"type":       funcType,
			"lines":      lineCount,
			"comment":    commentType,
			"assessment": assessment,
		})
	}

	return detailedFunctionsTable
}

// buildFunctionSummaryInterface builds the function summary interface.
func (c *Analyzer) buildFunctionSummaryInterface(metrics CommentMetrics) map[string]any {
	functionSummaryInterface := make(map[string]any)
	for name, info := range metrics.FunctionSummary {
		functionSummaryInterface[name] = map[string]any{
			"name":         info.Name,
			"type":         info.Type,
			"has_comment":  info.HasComment,
			"comment_type": info.CommentType,
		}
	}

	return functionSummaryInterface
}

// getCommentAssessment gets the assessment string for a comment.
func (c *Analyzer) getCommentAssessment(isGood bool) string {
	if isGood {
		return "✅ OK"
	}

	return "❌ Not OK"
}

// truncateCommentBody truncates comment body if too long for table display.
func (c *Analyzer) truncateCommentBody(commentBody string) string {
	if len(commentBody) > lenArg50 {
		return commentBody[:47] + "..."
	}

	return commentBody
}

// getFunctionAssessment gets the assessment and comment type for a function.
func (c *Analyzer) getFunctionAssessment(funcInfo FunctionInfo) (assessment, commentType string) {
	if funcInfo.HasComment {
		return "✅ Well Documented", funcInfo.CommentType
	}

	return "❌ No Comment", "None"
}

// getFunctionType gets the function type.
func (c *Analyzer) getFunctionType(function *node.Node) string {
	funcType := string(function.Type)
	if funcType == "" {
		return "Unknown"
	}

	return funcType
}

// getFunctionLineCount gets the line count of a function.
func (c *Analyzer) getFunctionLineCount(function *node.Node) int {
	if function.Pos != nil {
		return safeconv.MustUintToInt(function.Pos.EndLine - function.Pos.StartLine + 1)
	}

	return 0
}

// safeDiv performs division, returning 0 when the divisor is 0.
func safeDiv(a, b float64) float64 {
	if b == 0 {
		return 0
	}

	return a / b
}
