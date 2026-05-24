package harness

import (
	"strconv"
	"sync/atomic"
	"testing"
	"time"

	nxs "github.com/nyxis-io/nyxis-drivers/go"
)

// buildCompactNXB writes a small row-layout file for prefetch tests (<10 MB).
func buildCompactNXB(t *testing.T, n int) []byte {
	t.Helper()
	schema := nxs.NewSchema([]string{"id", "tag"})
	w := nxs.NewWriter(schema)
	for i := 0; i < n; i++ {
		w.BeginObject()
		w.WriteI64(0, int64(i))
		w.WriteStr(1, "r"+strconv.Itoa(i))
		w.EndObject()
	}
	return w.Finish()
}

// TestSpeculativeNonBlocking asserts record() does not wait on speculative prefetch (§2.4).
// Baseline: first record() before sequential pattern is classified (no speculative work).
// After warmup: record() with sequential pattern must return in similar wall time.
func TestSpeculativeNonBlocking(t *testing.T) {
	buf := buildCompactNXB(t, 200)

	const slowFetch = 80 * time.Millisecond
	var fetchCalls atomic.Int32
	slowFetchRange := func(off, length int64) ([]byte, error) {
		fetchCalls.Add(1)
		time.Sleep(slowFetch)
		if off < 0 || length < 0 || off > int64(len(buf)) {
			return nil, nil
		}
		end := off + length
		if end > int64(len(buf)) {
			end = int64(len(buf))
		}
		if end <= off {
			return nil, nil
		}
		out := make([]byte, end-off)
		copy(out, buf[int(off):int(end)])
		return out, nil
	}

	r, err := nxs.NewReader(buf, nxs.WithFetchRange(slowFetchRange))
	if err != nil {
		t.Fatal(err)
	}
	defer r.Close()

	// Baseline: no sequential pattern yet — record() must not block on prefetch.
	start := time.Now()
	_ = r.Record(0)
	baseline := time.Since(start)
	if baseline > slowFetch/2 {
		t.Fatalf("baseline record() took %v; expected no slow fetch before pattern detection", baseline)
	}

	// Establish sequential pattern (MinObservations=8, then sequential deltas).
	for i := 1; i < 25; i++ {
		_ = r.Record(i)
	}
	stats := r.CacheStats()
	if stats.Pattern != "sequential" {
		t.Fatalf("pattern = %q, want sequential before non-blocking check", stats.Pattern)
	}
	fetchCalls.Store(0)

	start = time.Now()
	_ = r.Record(26)
	withPrefetch := time.Since(start)

	if withPrefetch > slowFetch/2 {
		t.Fatalf("record() blocked on speculative prefetch: %v (baseline %v, fetches during call=%d)",
			withPrefetch, baseline, fetchCalls.Load())
	}
	// Allow generous noise for scheduler jitter; blocking fetch would dominate.
	maxAllowed := baseline*20 + 5*time.Millisecond
	if withPrefetch > maxAllowed && withPrefetch > baseline*3 {
		t.Fatalf("record() with prefetch took %v vs baseline %v (max allowed %v)",
			withPrefetch, baseline, maxAllowed)
	}
}
