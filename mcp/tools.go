package main

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"os/exec"
	"strings"
)

// runBinary executes a Rust binary and returns its stdout.
// On non-zero exit it returns a *ToolError.
func runBinary(ctx context.Context, resolver *Resolver, binary string, args []string) (string, error) {
	path, err := resolver.Find(binary)
	if err != nil {
		return "", &ToolError{Code: 1, Message: err.Error()}
	}

	var stdout, stderr bytes.Buffer
	cmd := exec.CommandContext(ctx, path, args...)
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	runErr := cmd.Run()
	if runErr == nil {
		return stdout.String(), nil
	}

	code := 1
	if exitErr, ok := runErr.(*exec.ExitError); ok {
		code = exitErr.ExitCode()
	}
	return "", &ToolError{
		Code:    code,
		Message: exitCodeMessage(code),
		Stderr:  strings.TrimSpace(stderr.String()),
	}
}

// toolResult formats a result string for MCP tool response.
// If err is a *ToolError, returns its JSON representation.
func toolResult(out string, err error) string {
	if err != nil {
		if te, ok := err.(*ToolError); ok {
			return te.JSON()
		}
		return (&ToolError{Code: 1, Message: err.Error()}).JSON()
	}
	return out
}

// ── Tool handlers ─────────────────────────────────────────────────────────────

// handleInspect runs nxs-inspect --json on a .nxb file.
func handleInspect(ctx context.Context, resolver *Resolver, path string, records string, verifyHash bool) string {
	if path == "" {
		return (&ToolError{Code: 1, Message: "path is required"}).JSON()
	}
	args := []string{"--json"}
	if records != "" {
		args = append(args, "--records", records)
	}
	if verifyHash {
		args = append(args, "--verify-hash")
	}
	args = append(args, path)
	out, err := runBinary(ctx, resolver, "nxs-inspect", args)
	return toolResult(out, err)
}

// handleSchema returns schema info only (no record data).
func handleSchema(ctx context.Context, resolver *Resolver, path string) string {
	if path == "" {
		return (&ToolError{Code: 1, Message: "path is required"}).JSON()
	}
	out, err := runBinary(ctx, resolver, "nxs-inspect", []string{"--json", "--records", "0", path})
	return toolResult(out, err)
}

// handleRecord returns a single record by zero-based index.
// Uses --record-index for O(1) tail-index lookup in the Rust binary.
func handleRecord(ctx context.Context, resolver *Resolver, path string, index int) string {
	if path == "" {
		return (&ToolError{Code: 1, Message: "path is required"}).JSON()
	}
	if index < 0 {
		return (&ToolError{Code: 1, Message: "index must be >= 0"}).JSON()
	}
	args := []string{"--json", "--record-index", fmt.Sprintf("%d", index), path}
	out, err := runBinary(ctx, resolver, "nxs-inspect", args)
	if err != nil {
		return toolResult("", err)
	}

	// Extract records[0].fields from the JSON envelope for a clean response.
	var result map[string]json.RawMessage
	if jsonErr := json.Unmarshal([]byte(out), &result); jsonErr != nil {
		return out
	}
	recordsRaw, ok := result["records"]
	if !ok {
		return out
	}
	var records []json.RawMessage
	if jsonErr := json.Unmarshal(recordsRaw, &records); jsonErr != nil || len(records) == 0 {
		return out
	}
	b, _ := json.MarshalIndent(records[0], "", "  ")
	return string(b)
}

// handleExportJSON exports a .nxb file as JSON with an optional record limit.
// When a limit is set, the binary is run with --ndjson and a streaming decoder
// stops after `limit` records — avoiding loading the full output into memory.
func handleExportJSON(ctx context.Context, resolver *Resolver, path string, pretty, ndjson bool, limit int) string {
	if path == "" {
		return (&ToolError{Code: 1, Message: "path is required"}).JSON()
	}

	// No limit: pass through directly (caller chose --all or default unlimited).
	if limit < 0 {
		args := []string{"--to", "json"}
		if pretty {
			args = append(args, "--pretty")
		}
		if ndjson {
			args = append(args, "--ndjson")
		}
		args = append(args, path)
		out, err := runBinary(ctx, resolver, "nxs-export", args)
		return toolResult(out, err)
	}

	// With a limit: stream NDJSON and stop after `limit` records to avoid
	// loading the full dataset into memory.
	binPath, findErr := resolver.Find("nxs-export")
	if findErr != nil {
		return (&ToolError{Code: 1, Message: findErr.Error()}).JSON()
	}

	var stderrBuf bytes.Buffer
	cmd := exec.CommandContext(ctx, binPath, "--to", "json", "--ndjson", path)
	cmd.Stderr = &stderrBuf
	stdout, pipeErr := cmd.StdoutPipe()
	if pipeErr != nil {
		return (&ToolError{Code: 1, Message: pipeErr.Error()}).JSON()
	}
	if startErr := cmd.Start(); startErr != nil {
		return (&ToolError{Code: 1, Message: startErr.Error()}).JSON()
	}

	dec := json.NewDecoder(stdout)
	var records []json.RawMessage
	for len(records) < limit {
		var raw json.RawMessage
		if err := dec.Decode(&raw); err != nil {
			break // EOF or error — stop cleanly
		}
		records = append(records, raw)
	}
	// Drain and close the pipe so the child process can exit cleanly.
	stdout.Close()
	cmd.Wait() //nolint:errcheck // exit code after partial read is irrelevant

	if ndjson {
		var sb strings.Builder
		for i, r := range records {
			if i > 0 {
				sb.WriteByte('\n')
			}
			sb.Write(r)
		}
		return sb.String()
	}

	if pretty {
		b, _ := json.MarshalIndent(records, "", "  ")
		return string(b)
	}
	b, _ := json.Marshal(records)
	return string(b)
}

// handleExportCSV exports a .nxb file as CSV.
func handleExportCSV(ctx context.Context, resolver *Resolver, path, columns, delimiter string) string {
	args := []string{"--to", "csv"}
	if columns != "" {
		args = append(args, "--columns", columns)
	}
	if delimiter != "" {
		args = append(args, "--csv-delimiter", delimiter)
	}
	args = append(args, path)
	out, err := runBinary(ctx, resolver, "nxs-export", args)
	return toolResult(out, err)
}

// handleImport converts a source file (JSON/CSV/XML) into a .nxb file.
func handleImport(ctx context.Context, resolver *Resolver, source, output, format, onConflict string) string {
	if format != "json" && format != "csv" && format != "xml" {
		return (&ToolError{Code: 1, Message: `format must be "json", "csv", or "xml"`}).JSON()
	}
	args := []string{"--from", format}
	if onConflict != "" {
		args = append(args, "--on-conflict", onConflict)
	}
	args = append(args, source, output)
	out, err := runBinary(ctx, resolver, "nxs-import", args)
	if err != nil {
		return toolResult("", err)
	}
	result := map[string]string{
		"output_path": output,
		"message":     strings.TrimSpace(out),
	}
	b, _ := json.Marshal(result)
	return string(b)
}

// handleCompile compiles a .nxs text file into a .nxb binary.
func handleCompile(ctx context.Context, resolver *Resolver, source, output string) string {
	args := []string{source}
	if output != "" {
		args = append(args, output)
	}
	out, err := runBinary(ctx, resolver, "nxs", args)
	if err != nil {
		return toolResult("", err)
	}
	outPath := output
	if outPath == "" {
		// nxs writes <source>.nxb by default
		outPath = strings.TrimSuffix(source, ".nxs") + ".nxb"
	}
	result := map[string]string{
		"output_path": outPath,
		"message":     strings.TrimSpace(out),
	}
	b, _ := json.Marshal(result)
	return string(b)
}
