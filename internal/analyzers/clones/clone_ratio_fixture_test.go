package clones

import (
	"context"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
)

// Fixture-based clone ratio tests validate that computeCloneRatio
// (pairs / maxPossiblePairs) produces meaningful, bounded values
// for known duplication patterns parsed through the real UAST pipeline.

// parseAndAnalyze parses Go source through UAST and runs the clone analyzer.
func parseAndAnalyze(t *testing.T, source string) analyze.Report {
	t.Helper()

	parser, err := uast.NewParser()
	require.NoError(t, err)

	root, parseErr := parser.Parse(context.Background(), "fixture.go", []byte(source))
	require.NoError(t, parseErr)

	analyzer := NewAnalyzer()

	report, analyzeErr := analyzer.Analyze(root)
	require.NoError(t, analyzeErr)

	return report
}

// reportFuncs extracts the total function count from a clone report.
func reportFuncs(t *testing.T, r analyze.Report) int {
	t.Helper()

	v, ok := r[keyTotalFunctions].(int)
	require.True(t, ok, "report must contain int %s", keyTotalFunctions)

	return v
}

// reportPairs extracts the total clone pair count from a clone report.
func reportPairs(t *testing.T, r analyze.Report) int {
	t.Helper()

	v, ok := r[keyTotalClonePairs].(int)
	require.True(t, ok, "report must contain int %s", keyTotalClonePairs)

	return v
}

// reportRatio extracts the clone ratio from a clone report.
func reportRatio(t *testing.T, r analyze.Report) float64 {
	t.Helper()

	v, ok := r[keyCloneRatio].(float64)
	require.True(t, ok, "report must contain float64 %s", keyCloneRatio)

	return v
}

// fixtureAllUnique contains 4 functions with completely different logic.
// Expected: 0 clone pairs, ratio = 0.
const fixtureAllUnique = `package fixture

func Sum(nums []int) int {
	total := 0
	for _, n := range nums {
		total += n
	}
	return total
}

func Reverse(s string) string {
	runes := []rune(s)
	for i, j := 0, len(runes)-1; i < j; i, j = i+1, j-1 {
		runes[i], runes[j] = runes[j], runes[i]
	}
	return string(runes)
}

func IsPrime(n int) bool {
	if n < 2 {
		return false
	}
	for i := 2; i*i <= n; i++ {
		if n%i == 0 {
			return false
		}
	}
	return true
}

func Fibonacci(n int) int {
	if n <= 1 {
		return n
	}
	a, b := 0, 1
	for i := 2; i <= n; i++ {
		a, b = b, a+b
	}
	return b
}
`

// fixtureAllIdentical contains 4 functions with identical bodies (Type-1 clones).
// Expected: 6 clone pairs (C(4,2)=6), ratio = 1.0.
const fixtureAllIdentical = `package fixture

func ProcessA(data []int) int {
	result := 0
	for _, v := range data {
		if v > 0 {
			result += v * 2
		} else {
			result -= v
		}
	}
	if result > 100 {
		result = 100
	}
	return result
}

func ProcessB(data []int) int {
	result := 0
	for _, v := range data {
		if v > 0 {
			result += v * 2
		} else {
			result -= v
		}
	}
	if result > 100 {
		result = 100
	}
	return result
}

func ProcessC(data []int) int {
	result := 0
	for _, v := range data {
		if v > 0 {
			result += v * 2
		} else {
			result -= v
		}
	}
	if result > 100 {
		result = 100
	}
	return result
}

func ProcessD(data []int) int {
	result := 0
	for _, v := range data {
		if v > 0 {
			result += v * 2
		} else {
			result -= v
		}
	}
	if result > 100 {
		result = 100
	}
	return result
}
`

// fixtureRenamedClones contains 3 functions: 2 are Type-2 clones (same AST
// structure, different variable names), 1 is unique.
const fixtureRenamedClones = `package fixture

func CalcScore(items []int) int {
	score := 0
	for _, item := range items {
		if item > 10 {
			score += item * 3
		} else {
			score += item
		}
	}
	if score > 1000 {
		score = 1000
	}
	return score
}

func ComputeTotal(entries []int) int {
	total := 0
	for _, entry := range entries {
		if entry > 10 {
			total += entry * 3
		} else {
			total += entry
		}
	}
	if total > 1000 {
		total = 1000
	}
	return total
}

func FormatOutput(s string) string {
	if len(s) == 0 {
		return "<empty>"
	}
	return "[" + s + "]"
}
`

// fixtureHalfClones contains 6 functions: 3 identical clones + 3 unique.
// maxPairs = C(6,2) = 15, clone pairs among the 3 identical = C(3,2) = 3.
const fixtureHalfClones = `package fixture

func CloneA(data []int) int {
	sum := 0
	for i := 0; i < len(data); i++ {
		if data[i] > 0 {
			sum += data[i]
		}
	}
	if sum > 500 {
		sum = 500
	}
	return sum
}

func CloneB(data []int) int {
	sum := 0
	for i := 0; i < len(data); i++ {
		if data[i] > 0 {
			sum += data[i]
		}
	}
	if sum > 500 {
		sum = 500
	}
	return sum
}

func CloneC(data []int) int {
	sum := 0
	for i := 0; i < len(data); i++ {
		if data[i] > 0 {
			sum += data[i]
		}
	}
	if sum > 500 {
		sum = 500
	}
	return sum
}

func UniqueX(n int) bool {
	if n < 2 {
		return false
	}
	for i := 2; i*i <= n; i++ {
		if n%i == 0 {
			return false
		}
	}
	return true
}

func UniqueY(s string) int {
	count := 0
	for _, r := range s {
		if r >= 'a' && r <= 'z' {
			count++
		}
	}
	return count
}

func UniqueZ(a, b int) int {
	for b != 0 {
		a, b = b, a%b
	}
	return a
}
`

func TestFixture_AllUnique_ZeroRatio(t *testing.T) {
	t.Parallel()

	report := parseAndAnalyze(t, fixtureAllUnique)
	require.Equal(t, 4, reportFuncs(t, report))

	assert.InDelta(t, 0.0, reportRatio(t, report), 0.05,
		"4 unique functions must produce near-zero clone ratio")
}

func TestFixture_AllIdentical_FullRatio(t *testing.T) {
	t.Parallel()

	report := parseAndAnalyze(t, fixtureAllIdentical)
	require.Equal(t, 4, reportFuncs(t, report))

	assert.Equal(t, 6, reportPairs(t, report),
		"4 identical functions must produce C(4,2)=6 clone pairs")
	assert.InDelta(t, 1.0, reportRatio(t, report), 0.01,
		"all-identical functions must produce ratio near 1.0")

	section := NewReportSection(report)
	assert.InDelta(t, 0.0, section.Score(), 0.01)
}

func TestFixture_RenamedClones_Detected(t *testing.T) {
	t.Parallel()

	report := parseAndAnalyze(t, fixtureRenamedClones)
	require.Equal(t, 3, reportFuncs(t, report))
	assert.GreaterOrEqual(t, reportPairs(t, report), 1,
		"Type-2 renamed clones must be detected")

	ratio := reportRatio(t, report)
	assert.Greater(t, ratio, 0.0, "renamed clones must produce non-zero ratio")
	assert.LessOrEqual(t, ratio, 1.0, "ratio must be bounded to [0, 1]")
}

func TestFixture_HalfClones_PartialRatio(t *testing.T) {
	t.Parallel()

	report := parseAndAnalyze(t, fixtureHalfClones)
	require.Equal(t, 6, reportFuncs(t, report))
	assert.GreaterOrEqual(t, reportPairs(t, report), 3,
		"3 identical + 3 unique must produce at least 3 clone pairs")

	ratio := reportRatio(t, report)
	// 3 cloned functions out of 6 total → 0.5.
	assert.InDelta(t, 0.5, ratio, 0.1, "ratio must reflect partial duplication")
}

func TestFixture_RatioBounded(t *testing.T) {
	t.Parallel()

	fixtures := map[string]string{
		"all_unique":    fixtureAllUnique,
		"all_identical": fixtureAllIdentical,
		"renamed":       fixtureRenamedClones,
		"half_clones":   fixtureHalfClones,
	}

	for name, source := range fixtures {
		t.Run(name, func(t *testing.T) {
			t.Parallel()

			ratio := reportRatio(t, parseAndAnalyze(t, source))
			assert.GreaterOrEqual(t, ratio, 0.0, "clone ratio must be >= 0")
			assert.LessOrEqual(t, ratio, 1.0, "clone ratio must be <= 1")
		})
	}
}

func TestFixture_MonotonicOrdering(t *testing.T) {
	t.Parallel()

	ratioUnique := reportRatio(t, parseAndAnalyze(t, fixtureAllUnique))
	ratioHalf := reportRatio(t, parseAndAnalyze(t, fixtureHalfClones))
	ratioFull := reportRatio(t, parseAndAnalyze(t, fixtureAllIdentical))

	assert.Less(t, ratioUnique, ratioHalf, "unique < half-cloned")
	assert.Less(t, ratioHalf, ratioFull, "half-cloned < fully-cloned")
}

// Kubernetes-derived fixtures: real patterns from kubernetes/kubernetes
// adapted to be self-contained. Validates detection on production-grade code.

// fixtureK8sValidation is adapted from pkg/apis/rbac/validation.
// ValidateRoleBinding and ValidateClusterRoleBinding are near-identical.
const fixtureK8sValidation = `package fixture

type ErrorList []string
type ObjectMeta struct{ Name string }

type Ref struct{ APIGroup, Kind, Name string }
type Subject struct{ Name string }
type RoleBinding struct{ ObjectMeta; Role Ref; Subjects []Subject }
type ClusterRoleBinding struct{ ObjectMeta; Role Ref; Subjects []Subject }

func ValidateRoleBinding(rb *RoleBinding) ErrorList {
	allErrs := ErrorList{}
	if rb.ObjectMeta.Name == "" {
		allErrs = append(allErrs, "metadata.name is required")
	}
	if rb.Role.APIGroup != "rbac.authorization.k8s.io" {
		allErrs = append(allErrs, "roleRef.apiGroup not supported")
	}
	switch rb.Role.Kind {
	case "Role", "ClusterRole":
	default:
		allErrs = append(allErrs, "roleRef.kind not supported")
	}
	if len(rb.Role.Name) == 0 {
		allErrs = append(allErrs, "roleRef.name is required")
	}
	for _, subject := range rb.Subjects {
		if subject.Name == "" {
			allErrs = append(allErrs, "subject.name is required")
		}
	}
	return allErrs
}

func ValidateClusterRoleBinding(rb *ClusterRoleBinding) ErrorList {
	allErrs := ErrorList{}
	if rb.ObjectMeta.Name == "" {
		allErrs = append(allErrs, "metadata.name is required")
	}
	if rb.Role.APIGroup != "rbac.authorization.k8s.io" {
		allErrs = append(allErrs, "roleRef.apiGroup not supported")
	}
	switch rb.Role.Kind {
	case "ClusterRole":
	default:
		allErrs = append(allErrs, "roleRef.kind not supported")
	}
	if len(rb.Role.Name) == 0 {
		allErrs = append(allErrs, "roleRef.name is required")
	}
	for _, subject := range rb.Subjects {
		if subject.Name == "" {
			allErrs = append(allErrs, "subject.name is required")
		}
	}
	return allErrs
}

func ValidateRoleBindingUpdate(rb *RoleBinding, old *RoleBinding) ErrorList {
	allErrs := ValidateRoleBinding(rb)
	if old.Role != rb.Role {
		allErrs = append(allErrs, "cannot change roleRef")
	}
	return allErrs
}

func ValidateClusterRoleBindingUpdate(rb *ClusterRoleBinding, old *ClusterRoleBinding) ErrorList {
	allErrs := ValidateClusterRoleBinding(rb)
	if old.Role != rb.Role {
		allErrs = append(allErrs, "cannot change roleRef")
	}
	return allErrs
}
`

// fixtureK8sEventHandlers is adapted from client-go/tools/cache/controller.go.
// Three receiver types implement OnAdd/OnUpdate/OnDelete.
const fixtureK8sEventHandlers = `package fixture

type ResourceEventHandlerFuncs struct {
	AddFunc    func(obj interface{})
	UpdateFunc func(oldObj, newObj interface{})
	DeleteFunc func(obj interface{})
}

func (r ResourceEventHandlerFuncs) OnAdd(obj interface{}, isInInitialList bool) {
	if r.AddFunc != nil {
		r.AddFunc(obj)
	}
}

func (r ResourceEventHandlerFuncs) OnUpdate(oldObj, newObj interface{}) {
	if r.UpdateFunc != nil {
		r.UpdateFunc(oldObj, newObj)
	}
}

func (r ResourceEventHandlerFuncs) OnDelete(obj interface{}) {
	if r.DeleteFunc != nil {
		r.DeleteFunc(obj)
	}
}

type ResourceEventHandlerDetailedFuncs struct {
	AddFunc    func(obj interface{}, isInInitialList bool)
	UpdateFunc func(oldObj, newObj interface{})
	DeleteFunc func(obj interface{})
}

func (r ResourceEventHandlerDetailedFuncs) OnAdd(obj interface{}, isInInitialList bool) {
	if r.AddFunc != nil {
		r.AddFunc(obj, isInInitialList)
	}
}

func (r ResourceEventHandlerDetailedFuncs) OnUpdate(oldObj, newObj interface{}) {
	if r.UpdateFunc != nil {
		r.UpdateFunc(oldObj, newObj)
	}
}

func (r ResourceEventHandlerDetailedFuncs) OnDelete(obj interface{}) {
	if r.DeleteFunc != nil {
		r.DeleteFunc(obj)
	}
}

type FilteringResourceEventHandler struct {
	FilterFunc func(obj interface{}) bool
	Handler    interface{ OnAdd(interface{}, bool); OnUpdate(interface{}, interface{}); OnDelete(interface{}) }
}

func (r FilteringResourceEventHandler) OnAdd(obj interface{}, isInInitialList bool) {
	if !r.FilterFunc(obj) {
		return
	}
	r.Handler.OnAdd(obj, isInInitialList)
}

func (r FilteringResourceEventHandler) OnUpdate(oldObj, newObj interface{}) {
	newer := r.FilterFunc(newObj)
	older := r.FilterFunc(oldObj)
	switch {
	case newer && older:
		r.Handler.OnUpdate(oldObj, newObj)
	case newer && !older:
		r.Handler.OnAdd(newObj, false)
	case !newer && older:
		r.Handler.OnDelete(oldObj)
	}
}

func (r FilteringResourceEventHandler) OnDelete(obj interface{}) {
	if !r.FilterFunc(obj) {
		return
	}
	r.Handler.OnDelete(obj)
}
`

// fixtureK8sDeepCopy is adapted from zz_generated.deepcopy.go files.
// Machine-generated DeepCopyInto methods on different receiver types.
const fixtureK8sDeepCopy = `package fixture

type TokenConfig struct{ Token, TTL, Expires *int64; Usages, Groups []string }
type SecretConfig struct{ Name, TTL, Expires *int64; Labels, Scopes []string }
type CertConfig struct{ Issuer, TTL, Expires *int64; SANs, Orgs []string }

func (in *TokenConfig) DeepCopyInto(out *TokenConfig) {
	*out = *in
	if in.Token != nil { cp := *in.Token; out.Token = &cp }
	if in.TTL != nil { cp := *in.TTL; out.TTL = &cp }
	if in.Expires != nil { cp := *in.Expires; out.Expires = &cp }
	if in.Usages != nil { out.Usages = make([]string, len(in.Usages)); copy(out.Usages, in.Usages) }
	if in.Groups != nil { out.Groups = make([]string, len(in.Groups)); copy(out.Groups, in.Groups) }
}

func (in *SecretConfig) DeepCopyInto(out *SecretConfig) {
	*out = *in
	if in.Name != nil { cp := *in.Name; out.Name = &cp }
	if in.TTL != nil { cp := *in.TTL; out.TTL = &cp }
	if in.Expires != nil { cp := *in.Expires; out.Expires = &cp }
	if in.Labels != nil { out.Labels = make([]string, len(in.Labels)); copy(out.Labels, in.Labels) }
	if in.Scopes != nil { out.Scopes = make([]string, len(in.Scopes)); copy(out.Scopes, in.Scopes) }
}

func (in *CertConfig) DeepCopyInto(out *CertConfig) {
	*out = *in
	if in.Issuer != nil { cp := *in.Issuer; out.Issuer = &cp }
	if in.TTL != nil { cp := *in.TTL; out.TTL = &cp }
	if in.Expires != nil { cp := *in.Expires; out.Expires = &cp }
	if in.SANs != nil { out.SANs = make([]string, len(in.SANs)); copy(out.SANs, in.SANs) }
	if in.Orgs != nil { out.Orgs = make([]string, len(in.Orgs)); copy(out.Orgs, in.Orgs) }
}
`

func TestFixtureK8s_Validation_DetectsClonePairs(t *testing.T) {
	t.Parallel()

	report := parseAndAnalyze(t, fixtureK8sValidation)
	require.Equal(t, 4, reportFuncs(t, report))
	assert.GreaterOrEqual(t, reportPairs(t, report), 2,
		"RBAC validation clones must produce at least 2 clone pairs")

	ratio := reportRatio(t, report)
	assert.Greater(t, ratio, 0.0)
	assert.LessOrEqual(t, ratio, 1.0)
}

func TestFixtureK8s_EventHandlers_DetectsClones(t *testing.T) {
	t.Parallel()

	report := parseAndAnalyze(t, fixtureK8sEventHandlers)
	assert.GreaterOrEqual(t, reportFuncs(t, report), 9)
	assert.GreaterOrEqual(t, reportPairs(t, report), 1,
		"identical handler methods across receiver types must be detected")

	ratio := reportRatio(t, report)
	assert.Greater(t, ratio, 0.0, "event handler clones must produce non-zero ratio")
	assert.LessOrEqual(t, ratio, 1.0)
}

func TestFixtureK8s_DeepCopy_HighCloneRatio(t *testing.T) {
	t.Parallel()

	report := parseAndAnalyze(t, fixtureK8sDeepCopy)
	require.Equal(t, 3, reportFuncs(t, report))
	assert.Equal(t, 3, reportPairs(t, report),
		"3 identical DeepCopyInto methods must produce C(3,2)=3 clone pairs")
	assert.InDelta(t, 1.0, reportRatio(t, report), 0.01,
		"all-identical DeepCopyInto methods must produce ratio near 1.0")
}

func TestFixtureK8s_AllBounded(t *testing.T) {
	t.Parallel()

	fixtures := map[string]string{
		"validation":     fixtureK8sValidation,
		"event_handlers": fixtureK8sEventHandlers,
		"deepcopy":       fixtureK8sDeepCopy,
	}

	for name, source := range fixtures {
		t.Run(name, func(t *testing.T) {
			t.Parallel()

			ratio := reportRatio(t, parseAndAnalyze(t, source))
			assert.GreaterOrEqual(t, ratio, 0.0, "clone ratio must be >= 0")
			assert.LessOrEqual(t, ratio, 1.0, "clone ratio must be <= 1")
		})
	}
}
