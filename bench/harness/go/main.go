// Cross-format benchmark harness (Go). Same CLI contract as C harness.
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"time"

	nxs "github.com/nyxis-io/nyxis-drivers/go"
)

const warmup = 100
const samples = 1000

type result struct {
	Workload   string  `json:"workload"`
	Format     string  `json:"format"`
	Records    uint    `json:"records"`
	Metric     string  `json:"metric"`
	P50Ns      int64   `json:"p50_ns,omitempty"`
	P99Ns      int64   `json:"p99_ns,omitempty"`
	IqrNs      int64   `json:"iqr_ns,omitempty"`
	Samples    int     `json:"samples,omitempty"`
	Bytes      int64   `json:"bytes,omitempty"`
	Population float64 `json:"population,omitempty"`
}

func measure(fn func()) (p50, p99, iqr int64) {
	for i := 0; i < warmup; i++ {
		fn()
	}
	buf := make([]int64, samples)
	for i := 0; i < samples; i++ {
		start := time.Now()
		fn()
		buf[i] = time.Since(start).Nanoseconds()
	}
	sort.Slice(buf, func(i, j int) bool { return buf[i] < buf[j] })
	q1 := buf[samples/4]
	q3 := buf[(3*samples)/4]
	iqr = q3 - q1
	trim := buf[samples/4 : (3*samples)/4+1]
	sort.Slice(trim, func(i, j int) bool { return trim[i] < trim[j] })
	p50 = trim[len(trim)/2]
	p99 = trim[int(float64(len(trim)-1)*0.99)]
	return p50, p99, iqr
}

func defaultPath(dataDir, workload string, records uint, pop float64) string {
	if workload == "A" && pop >= 0 {
		pct := int(pop*100 + 0.5)
		return filepath.Join(dataDir, fmt.Sprintf("workload_%s_nxs_%d_pop%02d.nxb", workload, records, pct))
	}
	return filepath.Join(dataDir, fmt.Sprintf("workload_%s_nxs_%d.nxb", workload, records))
}

func main() {
	workload := flag.String("workload", "", "A|B|C")
	format := flag.String("format", "nxs", "nxs|proto|fb|capnp|arrow")
	records := flag.Uint("records", 0, "record count")
	pop := flag.Float64("population", -1, "Workload A population 0–1")
	metric := flag.String("metric", "", "size|open|access|scan|selective")
	dataDir := flag.String("data-dir", "bench/data/bin", "binary fixtures")
	path := flag.String("path", "", "override fixture path")
	flag.Parse()

	if *workload == "" || *records == 0 || *metric == "" {
		fmt.Fprintln(os.Stderr, "required: --workload --records --metric")
		os.Exit(2)
	}
	if *format != "nxs" {
		fmt.Fprintf(os.Stderr, "go harness: only nxs implemented\n")
		os.Exit(1)
	}

	p := *path
	if p == "" {
		p = defaultPath(*dataDir, *workload, *records, *pop)
	}

	if *metric == "size" {
		st, err := os.Stat(p)
		if err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}
		out, _ := json.Marshal(result{
			Workload: *workload, Format: *format, Records: *records,
			Metric: "size", Bytes: st.Size(), Population: *pop,
		})
		fmt.Println(string(out))
		return
	}

	data, err := os.ReadFile(p)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}

	field := "score"
	if *workload == "A" {
		field = "f36"
	}

	switch *metric {
	case "open":
		r, err := nxs.NewReader(data)
		if err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}
		p50, p99, iqr := measure(func() {
			rec := r.Record(0)
			if rec != nil {
				_, _ = rec.GetF64(field)
			}
		})
		emit(*workload, *records, *metric, p50, p99, iqr, *pop)
	case "access":
		r, err := nxs.NewReader(data)
		if err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}
		var recIdx uint32
		access := func() {
			n := r.RecordCount()
			if n == 0 {
				return
			}
			recIdx = (recIdx*997 + 1) % uint32(n)
			rec := r.Record(int(recIdx))
			if rec != nil {
				_, _ = rec.GetF64(field)
			}
		}
		for i := 0; i < warmup; i++ {
			access()
		}
		p50, p99, iqr := measure(access)
		emit(*workload, *records, *metric, p50, p99, iqr, *pop)
	case "scan":
		r, err := nxs.NewReader(data)
		if err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}
		p50, p99, iqr := measure(func() {
			_ = r.SumF64(field)
		})
		emit(*workload, *records, *metric, p50, p99, iqr, *pop)
	case "selective":
		if *workload != "A" {
			fmt.Fprintln(os.Stderr, "selective metric only for workload A")
			os.Exit(2)
		}
		rr, err := nxs.NewReader(data)
		if err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}
		var recIdx uint32
		n := rr.RecordCount()
		readSel := func(rec *nxs.Object) {
			if rec == nil {
				return
			}
			_, _ = rec.GetI64("i01")
			_, _ = rec.GetStr("s21")
			_, _ = rec.GetF64("f36")
			_, _ = rec.GetBool("b46")
			_, _ = rec.GetI64("i10")
		}
		for i := 0; i < warmup; i++ {
			if n == 0 {
				break
			}
			recIdx = (recIdx*997 + 1) % uint32(n)
			readSel(rr.Record(int(recIdx)))
		}
		p50, p99, iqr := measure(func() {
			if n == 0 {
				return
			}
			recIdx = (recIdx*997 + 1) % uint32(n)
			readSel(rr.Record(int(recIdx)))
		})
		emit(*workload, *records, *metric, p50, p99, iqr, *pop)
	default:
		fmt.Fprintf(os.Stderr, "unknown metric %s\n", *metric)
		os.Exit(2)
	}
}

func emit(wl string, rec uint, met string, p50, p99, iqr int64, pop float64) {
	out, _ := json.Marshal(result{
		Workload: wl, Format: "nxs", Records: rec, Metric: met,
		P50Ns: p50, P99Ns: p99, IqrNs: iqr, Samples: samples, Population: pop,
	})
	fmt.Println(string(out))
}
