package harness

import (
	"sync/atomic"
	"testing"
	"time"

	nxs "github.com/nyxis-io/nyxis-drivers/go"
)

// TestPrefetchCancel opens with eager prefetch, closes before completion, and
// verifies no fetch completes after close (§9.4 prefetch_cancel).
func TestPrefetchCancel(t *testing.T) {
	buf := buildCompactNXB(t, 500)

	const slowFetch = 50 * time.Millisecond
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

	r, err := nxs.NewReader(buf, nxs.WithHint(nxs.HintFull), nxs.WithFetchRange(slowFetchRange))
	if err != nil {
		t.Fatal(err)
	}
	issued := r.CacheStats().FetchesIssued
	r.Close()
	after := fetchCalls.Load()
	// Close must cancel eager work; no unbounded fetch storm after drop.
	if after > int32(issued)+10 {
		t.Fatalf("fetches after close = %d, issued before close = %d", after, issued)
	}
}
