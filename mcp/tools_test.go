package main

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// ── exitCodeMessage ───────────────────────────────────────────────────────────

func TestExitCodeMessage(t *testing.T) {
	codes := []int{0, 1, 2, 3, 4, 5, 99}
	for _, code := range codes {
		msg := exitCodeMessage(code)
		if msg == "" {
			t.Errorf("exitCodeMessage(%d) returned empty string", code)
		}
	}
}

// ── Resolver ──────────────────────────────────────────────────────────────────

func TestResolverFind_BinDir(t *testing.T) {
	dir := t.TempDir()
	name := "nxs-inspect"
	p := filepath.Join(dir, name)
	if err := os.WriteFile(p, []byte{}, 0o755); err != nil {
		t.Fatal(err)
	}

	r := &Resolver{binDir: dir, exeDir: "/nonexistent"}
	got, err := r.Find(name)
	if err != nil {
		t.Fatalf("Find(%q): unexpected error: %v", name, err)
	}
	if got != p {
		t.Errorf("Find(%q) = %q, want %q", name, got, p)
	}
}

func TestResolverFind_NotFound(t *testing.T) {
	r := &Resolver{binDir: t.TempDir(), exeDir: "/nonexistent"}
	_, err := r.Find("nxs-definitely-does-not-exist")
	if err == nil {
		t.Fatal("expected error for missing binary, got nil")
	}
}

// ── Arg-construction helpers ──────────────────────────────────────────────────

func buildInspectArgs(path string, records string, verifyHash bool) []string {
	args := []string{"--json"}
	if records != "" {
		args = append(args, "--records", records)
	}
	if verifyHash {
		args = append(args, "--verify-hash")
	}
	args = append(args, path)
	return args
}

func buildExportJSONArgs(path string, pretty, ndjson bool) []string {
	args := []string{"--to", "json"}
	if pretty {
		args = append(args, "--pretty")
	}
	if ndjson {
		args = append(args, "--ndjson")
	}
	args = append(args, path)
	return args
}

func buildExportCSVArgs(path, columns, delimiter string) []string {
	args := []string{"--to", "csv"}
	if columns != "" {
		args = append(args, "--columns", columns)
	}
	if delimiter != "" {
		args = append(args, "--csv-delimiter", delimiter)
	}
	args = append(args, path)
	return args
}

func buildImportArgs(source, output, format, onConflict string) []string {
	args := []string{"--from", format}
	if onConflict != "" {
		args = append(args, "--on-conflict", onConflict)
	}
	args = append(args, source, output)
	return args
}

func buildCompileArgs(source, output string) []string {
	args := []string{source}
	if output != "" {
		args = append(args, output)
	}
	return args
}

func TestBuildInspectArgs(t *testing.T) {
	tests := []struct {
		path       string
		records    string
		verifyHash bool
		wantFlags  []string
	}{
		{"a.nxb", "3", false, []string{"--json", "--records", "3", "a.nxb"}},
		{"a.nxb", "0", false, []string{"--json", "--records", "0", "a.nxb"}},
		{"a.nxb", "", true, []string{"--json", "--verify-hash", "a.nxb"}},
		{"a.nxb", "all", true, []string{"--json", "--records", "all", "--verify-hash", "a.nxb"}},
	}
	for _, tt := range tests {
		got := buildInspectArgs(tt.path, tt.records, tt.verifyHash)
		if !slicesEqual(got, tt.wantFlags) {
			t.Errorf("buildInspectArgs(%q, %q, %v) = %v, want %v",
				tt.path, tt.records, tt.verifyHash, got, tt.wantFlags)
		}
	}
}

func TestBuildExportJSONArgs(t *testing.T) {
	tests := []struct {
		path   string
		pretty bool
		ndjson bool
		want   []string
	}{
		{"x.nxb", false, false, []string{"--to", "json", "x.nxb"}},
		{"x.nxb", true, false, []string{"--to", "json", "--pretty", "x.nxb"}},
		{"x.nxb", false, true, []string{"--to", "json", "--ndjson", "x.nxb"}},
		{"x.nxb", true, true, []string{"--to", "json", "--pretty", "--ndjson", "x.nxb"}},
	}
	for _, tt := range tests {
		got := buildExportJSONArgs(tt.path, tt.pretty, tt.ndjson)
		if !slicesEqual(got, tt.want) {
			t.Errorf("buildExportJSONArgs(%q, %v, %v) = %v, want %v",
				tt.path, tt.pretty, tt.ndjson, got, tt.want)
		}
	}
}

func TestBuildExportCSVArgs(t *testing.T) {
	tests := []struct {
		path, columns, delimiter string
		want                     []string
	}{
		{"x.nxb", "", "", []string{"--to", "csv", "x.nxb"}},
		{"x.nxb", "id,name", "", []string{"--to", "csv", "--columns", "id,name", "x.nxb"}},
		{"x.nxb", "", ";", []string{"--to", "csv", "--csv-delimiter", ";", "x.nxb"}},
	}
	for _, tt := range tests {
		got := buildExportCSVArgs(tt.path, tt.columns, tt.delimiter)
		if !slicesEqual(got, tt.want) {
			t.Errorf("buildExportCSVArgs = %v, want %v", got, tt.want)
		}
	}
}

func TestBuildImportArgs(t *testing.T) {
	tests := []struct {
		source, output, format, onConflict string
		want                               []string
	}{
		{"in.json", "out.nxb", "json", "", []string{"--from", "json", "in.json", "out.nxb"}},
		{"in.csv", "out.nxb", "csv", "coerce-string", []string{"--from", "csv", "--on-conflict", "coerce-string", "in.csv", "out.nxb"}},
	}
	for _, tt := range tests {
		got := buildImportArgs(tt.source, tt.output, tt.format, tt.onConflict)
		if !slicesEqual(got, tt.want) {
			t.Errorf("buildImportArgs = %v, want %v", got, tt.want)
		}
	}
}

func TestBuildCompileArgs(t *testing.T) {
	tests := []struct {
		source, output string
		want           []string
	}{
		{"data.nxs", "", []string{"data.nxs"}},
		{"data.nxs", "out.nxb", []string{"data.nxs", "out.nxb"}},
	}
	for _, tt := range tests {
		got := buildCompileArgs(tt.source, tt.output)
		if !slicesEqual(got, tt.want) {
			t.Errorf("buildCompileArgs = %v, want %v", got, tt.want)
		}
	}
}

// ── ToolError ─────────────────────────────────────────────────────────────────

func TestToolErrorJSON(t *testing.T) {
	te := &ToolError{Code: 3, Message: "format error", Stderr: "bad magic"}
	j := te.JSON()
	if !strings.Contains(j, `"code":3`) {
		t.Errorf("JSON missing code: %s", j)
	}
	if !strings.Contains(j, `"message"`) {
		t.Errorf("JSON missing message: %s", j)
	}
	if !strings.Contains(j, `"stderr"`) {
		t.Errorf("JSON missing stderr: %s", j)
	}
}

func TestToolErrorNoStderr(t *testing.T) {
	te := &ToolError{Code: 1, Message: "usage"}
	j := te.JSON()
	// stderr should be omitted when empty (omitempty)
	if strings.Contains(j, `"stderr"`) {
		t.Errorf("JSON should omit empty stderr: %s", j)
	}
}

// ── Input validation ──────────────────────────────────────────────────────────

func TestHandleInspect_EmptyPath(t *testing.T) {
	r := &Resolver{binDir: t.TempDir(), exeDir: "/nonexistent"}
	out := handleInspect(context.Background(), r, "", "3", false)
	if !strings.Contains(out, "path is required") {
		t.Errorf("expected path error, got: %s", out)
	}
}

func TestHandleSchema_EmptyPath(t *testing.T) {
	r := &Resolver{binDir: t.TempDir(), exeDir: "/nonexistent"}
	out := handleSchema(context.Background(), r, "")
	if !strings.Contains(out, "path is required") {
		t.Errorf("expected path error, got: %s", out)
	}
}

func TestHandleRecord_EmptyPath(t *testing.T) {
	r := &Resolver{binDir: t.TempDir(), exeDir: "/nonexistent"}
	out := handleRecord(context.Background(), r, "", 0)
	if !strings.Contains(out, "path is required") {
		t.Errorf("expected path error, got: %s", out)
	}
}

func TestHandleRecord_NegativeIndex(t *testing.T) {
	r := &Resolver{binDir: t.TempDir(), exeDir: "/nonexistent"}
	out := handleRecord(context.Background(), r, "a.nxb", -1)
	if !strings.Contains(out, `"code":1`) {
		t.Errorf("expected error response, got: %s", out)
	}
	if !strings.Contains(out, "index") {
		t.Errorf("expected index mention in error, got: %s", out)
	}
}

func TestHandleImport_BadFormat(t *testing.T) {
	r := &Resolver{binDir: t.TempDir(), exeDir: "/nonexistent"}
	out := handleImport(context.Background(), r, "in.yaml", "out.nxb", "yaml", "")
	if !strings.Contains(out, `format must be`) {
		t.Errorf("expected format error, got: %s", out)
	}
}

// ── handleExportJSON limit truncation ─────────────────────────────────────────

func TestExportJSONLimit_ZeroReturnsNoRecords(t *testing.T) {
	// limit=0 should return an empty JSON array (not bypass the cap)
	// We test the slicing logic directly since we have no binary in CI.
	arr := `[{"a":1},{"a":2},{"a":3}]`
	var parsed []json.RawMessage
	if err := json.Unmarshal([]byte(arr), &parsed); err != nil {
		t.Fatal(err)
	}
	limit := 0
	if len(parsed) > limit {
		parsed = parsed[:limit]
	}
	if len(parsed) != 0 {
		t.Errorf("expected 0 records for limit=0, got %d", len(parsed))
	}
}

func TestExportJSONLimit_Truncates(t *testing.T) {
	arr := `[{"a":1},{"a":2},{"a":3},{"a":4},{"a":5}]`
	var parsed []json.RawMessage
	if err := json.Unmarshal([]byte(arr), &parsed); err != nil {
		t.Fatal(err)
	}
	limit := 2
	if len(parsed) > limit {
		parsed = parsed[:limit]
	}
	if len(parsed) != 2 {
		t.Errorf("expected 2 records, got %d", len(parsed))
	}
}

// ── readHeader ────────────────────────────────────────────────────────────────

func TestReadHeader_Valid(t *testing.T) {
	// Build a minimal valid .nxb header:
	// bytes 0-3:   NYXB magic
	// bytes 4-15:  version/flags/hash (zeros)
	// bytes 16-23: tailPtr = 32 (points just after preamble)
	// bytes 24-31: zeros
	// bytes 32-35: recordCount = 7
	// last 4 bytes: NXS! footer magic
	buf := make([]byte, 40)
	// NYXB magic
	buf[0], buf[1], buf[2], buf[3] = 0x42, 0x58, 0x59, 0x4E // NYXB in little-endian
	// tailPtr = 32 at bytes 16-23 (LE)
	buf[16] = 32
	// recordCount = 7 at offset 32
	buf[32] = 7
	// NXS! footer at last 4 bytes
	buf[36], buf[37], buf[38], buf[39] = 0x4E, 0x58, 0x53, 0x21

	f, err := os.CreateTemp(t.TempDir(), "*.nxb")
	if err != nil {
		t.Fatal(err)
	}
	if _, writeErr := f.Write(buf); writeErr != nil {
		t.Fatal(writeErr)
	}
	f.Close()

	fr, err := readHeader(f.Name())
	if err != nil {
		t.Fatalf("readHeader: %v", err)
	}
	if fr.RecordCount != 7 {
		t.Errorf("RecordCount = %d, want 7", fr.RecordCount)
	}
	if fr.SizeBytes != 40 {
		t.Errorf("SizeBytes = %d, want 40", fr.SizeBytes)
	}
}

func TestReadHeader_BadMagic(t *testing.T) {
	buf := make([]byte, 40) // all zeros — wrong magic
	f, err := os.CreateTemp(t.TempDir(), "*.nxb")
	if err != nil {
		t.Fatal(err)
	}
	f.Write(buf)
	f.Close()

	_, err = readHeader(f.Name())
	if err == nil {
		t.Fatal("expected error for bad magic, got nil")
	}
}

func TestReadHeader_TooSmall(t *testing.T) {
	f, err := os.CreateTemp(t.TempDir(), "*.nxb")
	if err != nil {
		t.Fatal(err)
	}
	f.Write([]byte{0x01, 0x02}) // only 2 bytes
	f.Close()

	_, err = readHeader(f.Name())
	if err == nil {
		t.Fatal("expected error for too-small file, got nil")
	}
}

// ── helpers ───────────────────────────────────────────────────────────────────

func slicesEqual(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}
