package burndown

import (
	"sync"
	"testing"
)

func TestPathInterner_InternLookup(t *testing.T) {
	t.Parallel()

	pi := NewPathInterner()

	id0 := pi.Intern("a.go")

	id1 := pi.Intern("b.go")
	if id0 == id1 {
		t.Errorf("different paths must get different ids: %d == %d", id0, id1)
	}

	if pi.Lookup(id0) != "a.go" {
		t.Errorf("Lookup(%d) = %s want a.go", id0, pi.Lookup(id0))
	}

	if pi.Lookup(id1) != "b.go" {
		t.Errorf("Lookup(%d) = %s want b.go", id1, pi.Lookup(id1))
	}

	// Same path returns same id.
	id0Again := pi.Intern("a.go")
	if id0Again != id0 {
		t.Errorf("re-Intern(a.go) = %d want %d", id0Again, id0)
	}
}

func TestPathInterner_Len(t *testing.T) {
	t.Parallel()

	pi := NewPathInterner()
	if pi.Len() != 0 {
		t.Errorf("Len() = %d want 0", pi.Len())
	}

	pi.Intern("x")

	if pi.Len() != 1 {
		t.Errorf("after Intern(x) Len() = %d want 1", pi.Len())
	}

	pi.Intern("y")

	if pi.Len() != 2 {
		t.Errorf("after Intern(y) Len() = %d want 2", pi.Len())
	}

	pi.Intern("x")

	if pi.Len() != 2 {
		t.Errorf("after re-Intern(x) Len() = %d want 2", pi.Len())
	}
}

func TestPathInterner_Concurrent(t *testing.T) {
	t.Parallel()

	pi := NewPathInterner()

	var wg sync.WaitGroup
	for i := range 100 {
		wg.Add(1)

		go func(j int) {
			defer wg.Done()

			path := string(rune('a'+j%26)) + "/file.go"

			id := pi.Intern(path)
			if pi.Lookup(id) != path {
				t.Errorf("Lookup(%d) != %s", id, path)
			}
		}(i)
	}

	wg.Wait()
}
