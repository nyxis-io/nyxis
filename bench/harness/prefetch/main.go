// Workload F native bench — remote-style fetch recorder via Go driver.
//
// Usage (from nyxis/):
//
//	cd ../nyxis-drivers/go && go run ../../nyxis/bench/harness/prefetch/main.go \
//	  --path ../../nyxis/bench/data/bin/workload_B_nxs_1000000.nxb --scenario all
package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"math/rand"
	"os"
	"runtime"
	"sort"
	"time"

	nxs "github.com/nyxis-io/nyxis-drivers/go"
)

type line struct {
	Workload     string  `json:"workload"`
	Scenario     string  `json:"scenario"`
	Mode         string  `json:"mode"`
	Records      int     `json:"records"`
	FileBytes    int64   `json:"file_bytes"`
	LatencyUs    int     `json:"latency_us"`
	Metric       string  `json:"metric"`
	Value        float64 `json:"value"`
	Unit         string  `json:"unit"`
	Fetches      int     `json:"fetches,omitempty"`
	CacheHits    int64   `json:"cache_hits,omitempty"`
	CacheMisses  int64   `json:"cache_misses,omitempty"`
	Driver       string  `json:"driver"`
}

type remoteStore struct {
	data      []byte
	latency   time.Duration
	fetches   int
}

func (rs *remoteStore) fetch(off, length int64) ([]byte, error) {
	if rs.latency > 0 {
		time.Sleep(rs.latency)
	}
	rs.fetches++
	start := int(off)
	end := start + int(length)
	if start < 0 || end > len(rs.data) {
		return nil, os.ErrInvalid
	}
	out := make([]byte, end-start)
	copy(out, rs.data[start:end])
	return out, nil
}

func newRemoteReader(data []byte, rs *remoteStore, opts ...nxs.ReaderOption) (*nxs.Reader, error) {
	base := append([]nxs.ReaderOption{
		nxs.WithFetchRange(func(off, length int64) ([]byte, error) {
			return rs.fetch(off, length)
		}),
	}, opts...)
	return nxs.NewReader(data, base...)
}

func emit(l line) {
	b, _ := json.Marshal(l)
	fmt.Println(string(b))
}

func readField(r *nxs.Reader, idx int) {
	obj := r.Record(idx)
	_, _ = obj.GetStr("username")
}

func runF1(data []byte, rs *remoteStore, n int, latencyUs int, fileBytes int64) {
	end := min(49, n-1)

	lazy := func() {
		rs.fetches = 0
		r, err := newRemoteReader(data, rs)
		if err != nil {
			panic(err)
		}
		for i := 0; i <= end; i++ {
			readField(r, i)
		}
	}

	prefetch := func() {
		rs.fetches = 0
		r, err := newRemoteReader(data, rs)
		if err != nil {
			panic(err)
		}
		if err := r.PrefetchViewport(context.Background(), 0, end); err != nil {
			panic(err)
		}
		for i := 0; i <= end; i++ {
			readField(r, i)
		}
	}

	lazyMs := medianMs(lazy, 20)
	rs.fetches = 0
	lazy()
	lazyFetches := rs.fetches

	prefetchMs := medianMs(prefetch, 20)
	rs.fetches = 0
	prefetch()
	prefetchFetches := rs.fetches

	emit(line{
		Workload: "F", Scenario: "F1", Mode: "lazy", Records: n, FileBytes: fileBytes,
		LatencyUs: latencyUs, Metric: "viewport_warm", Value: lazyMs, Unit: "ms",
		Fetches: lazyFetches, Driver: "go",
	})
	emit(line{
		Workload: "F", Scenario: "F1", Mode: "prefetch_viewport", Records: n, FileBytes: fileBytes,
		LatencyUs: latencyUs, Metric: "viewport_warm", Value: prefetchMs, Unit: "ms",
		Fetches: prefetchFetches, Driver: "go",
	})
}

func runF2(data []byte, rs *remoteStore, n int, step int, latencyUs int, fileBytes int64) {
	lazy := func() {
		rs.fetches = 0
		r, err := newRemoteReader(data, rs)
		if err != nil {
			panic(err)
		}
		for start := 0; start < n; start += step {
			end := min(start+step-1, n-1)
			for i := start; i <= end; i++ {
				readField(r, i)
			}
		}
	}

	prefetch := func() {
		rs.fetches = 0
		r, err := newRemoteReader(data, rs, nxs.WithHint(nxs.HintSequential))
		if err != nil {
			panic(err)
		}
		for start := 0; start < n; start += step {
			end := min(start+step-1, n-1)
			if err := r.PrefetchViewport(context.Background(), start, end); err != nil {
				panic(err)
			}
			for i := start; i <= end; i++ {
				readField(r, i)
			}
		}
	}

	start := time.Now()
	lazy()
	lazySec := time.Since(start).Seconds()
	lazyFetches := rs.fetches

	start = time.Now()
	prefetch()
	prefetchSec := time.Since(start).Seconds()
	prefetchFetches := rs.fetches

	emit(line{
		Workload: "F", Scenario: "F2", Mode: "lazy", Records: n, FileBytes: fileBytes,
		LatencyUs: latencyUs, Metric: "scroll_total", Value: lazySec, Unit: "s",
		Fetches: lazyFetches, Driver: "go",
	})
	emit(line{
		Workload: "F", Scenario: "F2", Mode: "prefetch_adaptive", Records: n, FileBytes: fileBytes,
		LatencyUs: latencyUs, Metric: "scroll_total", Value: prefetchSec, Unit: "s",
		Fetches: prefetchFetches, Driver: "go",
	})
}

func runF3(data []byte, rs *remoteStore, n int, reads int, latencyUs int, fileBytes int64) {
	rng := rand.New(rand.NewSource(0x4E595849))
	idxs := make([]int, reads)
	for i := range idxs {
		idxs[i] = rng.Intn(n)
	}

	lazy := func() {
		rs.fetches = 0
		r, err := newRemoteReader(data, rs)
		if err != nil {
			panic(err)
		}
		for _, idx := range idxs {
			readField(r, idx)
		}
	}

	prefetch := func() {
		rs.fetches = 0
		r, err := newRemoteReader(data, rs, nxs.WithHint(nxs.HintRandom))
		if err != nil {
			panic(err)
		}
		for _, idx := range idxs {
			readField(r, idx)
		}
	}

	lazyMs := medianMs(lazy, 5)
	rs.fetches = 0
	lazy()
	lazyFetches := rs.fetches

	prefetchMs := medianMs(prefetch, 5)
	rs.fetches = 0
	prefetch()
	prefetchFetches := rs.fetches

	emit(line{
		Workload: "F", Scenario: "F3", Mode: "lazy", Records: n, FileBytes: fileBytes,
		LatencyUs: latencyUs, Metric: "random_1k", Value: lazyMs, Unit: "ms",
		Fetches: lazyFetches, Driver: "go",
	})
	emit(line{
		Workload: "F", Scenario: "F3", Mode: "prefetch_random_hint", Records: n, FileBytes: fileBytes,
		LatencyUs: latencyUs, Metric: "random_1k", Value: prefetchMs, Unit: "ms",
		Fetches: prefetchFetches, Driver: "go",
	})
}

func runF4(data []byte, rs *remoteStore, n int, step int, latencyUs int, fileBytes int64) {
	const maxPages = 64
	measure := func(mode string, hint nxs.AccessHint, usePrefetch bool) {
		rs.fetches = 0
		runtime.GC()
		var before runtime.MemStats
		runtime.ReadMemStats(&before)
		peakSys := before.Sys

		opts := []nxs.ReaderOption{
			nxs.WithMaxPages(maxPages),
			nxs.WithHint(hint),
		}
		r, err := newRemoteReader(data, rs, opts...)
		if err != nil {
			panic(err)
		}
		for start := 0; start < n; start += step {
			end := min(start+step-1, n-1)
			if usePrefetch {
				if err := r.PrefetchViewport(context.Background(), start, end); err != nil {
					panic(err)
				}
			}
			for i := start; i <= end; i++ {
				readField(r, i)
			}
			var ms runtime.MemStats
			runtime.ReadMemStats(&ms)
			if ms.Sys > peakSys {
				peakSys = ms.Sys
			}
		}
		stats := r.CacheStats()
		emit(line{
			Workload: "F", Scenario: "F4", Mode: mode, Records: n, FileBytes: fileBytes,
			LatencyUs: latencyUs, Metric: "peak_sys_mb", Value: float64(peakSys) / (1024 * 1024),
			Unit: "MB", Fetches: rs.fetches, CacheHits: int64(stats.CacheHits), CacheMisses: int64(stats.CacheMisses),
			Driver: "go",
		})
	}

	measure("lazy", nxs.HintUnknown, false)
	measure("prefetch_adaptive", nxs.HintSequential, true)
}

func medianMs(fn func(), runs int) float64 {
	vals := make([]float64, runs)
	for i := 0; i < runs; i++ {
		start := time.Now()
		fn()
		vals[i] = float64(time.Since(start).Microseconds()) / 1000.0
	}
	sort.Float64s(vals)
	return vals[runs/2]
}

func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}

func main() {
	path := flag.String("path", "", "row-layout .nxb fixture")
	scenario := flag.String("scenario", "all", "F1|F2|F3|F4|all|smoke")
	latencyUs := flag.Int("latency-us", 100, "simulated per-fetch latency (µs); 0 = count-only")
	viewport := flag.Int("viewport", 50, "records per viewport step (F2/F4)")
	randomReads := flag.Int("random-reads", 1000, "random record reads (F3)")
	maxRecords := flag.Int("max-records", 0, "cap effective record count (0 = all)")
	flag.Parse()

	if *path == "" {
		fmt.Fprintln(os.Stderr, "required: --path")
		os.Exit(2)
	}

	data, err := os.ReadFile(*path)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}

	r, err := nxs.NewReader(data)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	n := r.RecordCount()
	if *maxRecords > 0 && *maxRecords < n {
		n = *maxRecords
	}
	fileBytes := int64(len(data))

	rs := &remoteStore{
		data:    data,
		latency: time.Duration(*latencyUs) * time.Microsecond,
	}

	run := func(name string) {
		switch name {
		case "F1":
			runF1(data, rs, n, *latencyUs, fileBytes)
		case "F2":
			runF2(data, rs, n, *viewport, *latencyUs, fileBytes)
		case "F3":
			runF3(data, rs, n, *randomReads, *latencyUs, fileBytes)
		case "F4":
			runF4(data, rs, n, *viewport, *latencyUs, fileBytes)
		default:
			fmt.Fprintf(os.Stderr, "unknown scenario %q\n", name)
			os.Exit(2)
		}
	}

	switch *scenario {
	case "all":
		run("F1")
		run("F2")
		run("F3")
		run("F4")
	case "smoke":
		if *maxRecords == 0 {
			*maxRecords = 10000
		}
		n = min(n, *maxRecords)
		run("F1")
		run("F2")
		run("F3")
		run("F4")
	default:
		run(*scenario)
	}
}
