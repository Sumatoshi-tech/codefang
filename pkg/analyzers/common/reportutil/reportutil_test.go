package reportutil //nolint:testpackage // testing internal implementation.

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

func TestGetFloat64_Float(t *testing.T) {
	t.Parallel()

	r := analyze.Report{"key": 3.14}
	if got := GetFloat64(r, "key"); got != 3.14 {
		t.Errorf("GetFloat64() = %v, want 3.14", got)
	}
}

func TestGetFloat64_Int(t *testing.T) {
	t.Parallel()

	r := analyze.Report{"key": 5}
	if got := GetFloat64(r, "key"); got != 5.0 {
		t.Errorf("GetFloat64() = %v, want 5.0", got)
	}
}

func TestGetFloat64_Missing(t *testing.T) {
	t.Parallel()

	r := analyze.Report{}
	if got := GetFloat64(r, "key"); got != 0 {
		t.Errorf("GetFloat64() = %v, want 0", got)
	}
}

func TestGetInt_Int(t *testing.T) {
	t.Parallel()

	r := analyze.Report{"key": 42}
	if got := GetInt(r, "key"); got != 42 {
		t.Errorf("GetInt() = %v, want 42", got)
	}
}

func TestGetInt_Float(t *testing.T) {
	t.Parallel()

	r := analyze.Report{"key": 42.0}
	if got := GetInt(r, "key"); got != 42 {
		t.Errorf("GetInt() = %v, want 42", got)
	}
}

func TestGetInt_Missing(t *testing.T) {
	t.Parallel()

	r := analyze.Report{}
	if got := GetInt(r, "key"); got != 0 {
		t.Errorf("GetInt() = %v, want 0", got)
	}
}

func TestGetString_Present(t *testing.T) {
	t.Parallel()

	r := analyze.Report{"key": "hello"}
	if got := GetString(r, "key"); got != "hello" {
		t.Errorf("GetString() = %q, want %q", got, "hello")
	}
}

func TestGetString_Missing(t *testing.T) {
	t.Parallel()

	r := analyze.Report{}
	if got := GetString(r, "key"); got != "" {
		t.Errorf("GetString() = %q, want empty", got)
	}
}

func TestGetString_WrongType(t *testing.T) {
	t.Parallel()

	r := analyze.Report{"key": 42}
	if got := GetString(r, "key"); got != "" {
		t.Errorf("GetString() = %q, want empty for wrong type", got)
	}
}

func TestGetFunctions_Present(t *testing.T) {
	t.Parallel()

	fns := []map[string]any{{"name": "foo"}}
	r := analyze.Report{"functions": fns}

	got := GetFunctions(r, "functions")
	if len(got) != 1 {
		t.Errorf("GetFunctions() len = %d, want 1", len(got))
	}
}

func TestGetFunctions_Missing(t *testing.T) {
	t.Parallel()

	r := analyze.Report{}

	got := GetFunctions(r, "functions")
	if got != nil {
		t.Errorf("GetFunctions() = %v, want nil", got)
	}
}

func TestGetStringSlice_Present(t *testing.T) {
	t.Parallel()

	r := analyze.Report{"imports": []string{"os", "fmt"}}

	got := GetStringSlice(r, "imports")
	if len(got) != 2 {
		t.Errorf("GetStringSlice() len = %d, want 2", len(got))
	}
}

func TestGetStringSlice_Missing(t *testing.T) {
	t.Parallel()

	r := analyze.Report{}

	got := GetStringSlice(r, "imports")
	if got != nil {
		t.Errorf("GetStringSlice() = %v, want nil", got)
	}
}

func TestGetStringIntMap_Present(t *testing.T) {
	t.Parallel()

	r := analyze.Report{"counts": map[string]int{"os": 3}}

	got := GetStringIntMap(r, "counts")
	if got["os"] != 3 {
		t.Errorf("GetStringIntMap()[os] = %d, want 3", got["os"])
	}
}

func TestGetStringIntMap_Missing(t *testing.T) {
	t.Parallel()

	r := analyze.Report{}

	got := GetStringIntMap(r, "counts")
	if got != nil {
		t.Errorf("GetStringIntMap() = %v, want nil", got)
	}
}

func TestMapString_Present(t *testing.T) {
	t.Parallel()

	m := map[string]any{"name": "foo"}
	if got := MapString(m, "name"); got != "foo" {
		t.Errorf("MapString() = %q, want %q", got, "foo")
	}
}

func TestMapString_Missing(t *testing.T) {
	t.Parallel()

	m := map[string]any{}
	if got := MapString(m, "name"); got != "" {
		t.Errorf("MapString() = %q, want empty", got)
	}
}

func TestMapInt_Int(t *testing.T) {
	t.Parallel()

	m := map[string]any{"val": 10}
	if got := MapInt(m, "val"); got != 10 {
		t.Errorf("MapInt() = %d, want 10", got)
	}
}

func TestMapInt_Float(t *testing.T) {
	t.Parallel()

	m := map[string]any{"val": 10.0}
	if got := MapInt(m, "val"); got != 10 {
		t.Errorf("MapInt() = %d, want 10", got)
	}
}

func TestMapInt_Missing(t *testing.T) {
	t.Parallel()

	m := map[string]any{}
	if got := MapInt(m, "val"); got != 0 {
		t.Errorf("MapInt() = %d, want 0", got)
	}
}

func TestMapFloat64_Float(t *testing.T) {
	t.Parallel()

	m := map[string]any{"val": 0.75}
	if got := MapFloat64(m, "val"); got != 0.75 {
		t.Errorf("MapFloat64() = %v, want 0.75", got)
	}
}

func TestMapFloat64_Int(t *testing.T) {
	t.Parallel()

	m := map[string]any{"val": 5}
	if got := MapFloat64(m, "val"); got != 5.0 {
		t.Errorf("MapFloat64() = %v, want 5.0", got)
	}
}

func TestMapFloat64_Missing(t *testing.T) {
	t.Parallel()

	m := map[string]any{}
	if got := MapFloat64(m, "val"); got != 0 {
		t.Errorf("MapFloat64() = %v, want 0", got)
	}
}

func TestFormatInt(t *testing.T) {
	t.Parallel()

	if got := FormatInt(42); got != "42" {
		t.Errorf("FormatInt(42) = %q, want %q", got, "42")
	}
}

func TestFormatFloat(t *testing.T) {
	t.Parallel()

	if got := FormatFloat(3.14159); got != "3.1" {
		t.Errorf("FormatFloat(3.14159) = %q, want %q", got, "3.1")
	}
}

func TestFormatPercent(t *testing.T) {
	t.Parallel()

	if got := FormatPercent(0.85); got != "85.0%" {
		t.Errorf("FormatPercent(0.85) = %q, want %q", got, "85.0%")
	}
}

func TestPct_Normal(t *testing.T) {
	t.Parallel()

	if got := Pct(3, 10); got != 0.3 {
		t.Errorf("Pct(3,10) = %v, want 0.3", got)
	}
}

func TestPct_Zero(t *testing.T) {
	t.Parallel()

	if got := Pct(0, 0); got != 0 {
		t.Errorf("Pct(0,0) = %v, want 0", got)
	}
}
