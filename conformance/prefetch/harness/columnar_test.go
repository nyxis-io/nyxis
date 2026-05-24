package harness

import (
	"os"
	"path/filepath"
	"sync/atomic"
	"testing"

	nxs "github.com/nyxis-io/nyxis-drivers/go"
)

// TestPrefetchColumnarFastPath asserts §9.4 prefetch_columnar_fast_path:
// one range fetch for the column buffer before col_sum_f64.
func TestPrefetchColumnarFastPath(t *testing.T) {
	fixture := filepath.Join("..", "prefetch_columnar_fast_path.nxb")
	data, err := os.ReadFile(fixture)
	if err != nil {
		t.Skipf("fixture missing (run make conformance-generate): %v", err)
	}
	var fetches atomic.Int32
	r, err := nxs.NewReader(data, nxs.WithFetchRange(func(off, length int64) ([]byte, error) {
		fetches.Add(1)
		return sliceFixture(data, off, length)
	}))
	if err != nil {
		t.Fatal(err)
	}
	if err := r.PrefetchColumn("score"); err != nil {
		t.Fatal(err)
	}
	if got := fetches.Load(); got != 1 {
		t.Fatalf("prefetch_column fetches = %d, want 1", got)
	}
	sum := r.ColSumF64("score")
	if got := fetches.Load(); got != 1 {
		t.Fatalf("col_sum_f64 issued extra fetches: got %d, want 1", got)
	}
	const want = 2475.0
	if sum != want {
		t.Fatalf("col_sum_f64(score) = %v, want %v", sum, want)
	}
}

func sliceFixture(data []byte, off, length int64) ([]byte, error) {
	start := int(off)
	end := start + int(length)
	if start < 0 || end > len(data) {
		return nil, os.ErrInvalid
	}
	out := make([]byte, end-start)
	copy(out, data[start:end])
	return out, nil
}
