package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"log"
	"os"
	"os/signal"
	"syscall"

	"github.com/modelcontextprotocol/go-sdk/mcp"
)

func main() {
	dataDir := flag.String("data-dir", "", "Directory to scan for .nxb resources")
	binDir := flag.String("bin-dir", "", "Directory containing nxs-inspect, nxs-export, nxs-import, nxs binaries")
	flag.Parse()

	resolver := NewResolver(*binDir)

	s := mcp.NewServer(&mcp.Implementation{Name: "nxs-mcp", Version: "1.0.0"}, nil)

	registerTools(s, resolver)
	registerResources(s, resolver, *dataDir)

	ctx, cancel := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer cancel()

	log.SetOutput(os.Stderr)
	log.Printf("nxs-mcp starting (data-dir=%q bin-dir=%q)", *dataDir, *binDir)

	if err := s.Run(ctx, &mcp.StdioTransport{}); err != nil {
		fmt.Fprintf(os.Stderr, "nxs-mcp: %v\n", err)
		os.Exit(1)
	}
}

// ── Tool input structs ────────────────────────────────────────────────────────

// Required fields are non-pointer; optional fields are pointer or have omitempty.
// The jsonschema tag value is the field description (must not start with WORD=).

type InspectInput struct {
	Path       string  `json:"path"                  jsonschema:"Absolute or relative path to the .nxb file"`
	Records    *string `json:"records,omitempty"     jsonschema:"Number of records to decode or all. Default: 3"`
	VerifyHash *bool   `json:"verify_hash,omitempty" jsonschema:"Recompute and verify the schema DictHash"`
}

type SchemaInput struct {
	Path string `json:"path" jsonschema:"Absolute or relative path to the .nxb file"`
}

type RecordInput struct {
	Path  string `json:"path"  jsonschema:"Absolute or relative path to the .nxb file"`
	Index int    `json:"index" jsonschema:"Zero-based record index"`
}

type ExportJSONInput struct {
	Path   string `json:"path"              jsonschema:"Absolute or relative path to the .nxb file"`
	Pretty *bool  `json:"pretty,omitempty"  jsonschema:"2-space indented JSON output"`
	NDJSON *bool  `json:"ndjson,omitempty"  jsonschema:"Newline-delimited JSON (one record per line)"`
	Limit  *int   `json:"limit,omitempty"   jsonschema:"Max records to return. Default 100. Pass -1 for all records."`
}

type ExportCSVInput struct {
	Path      string  `json:"path"                jsonschema:"Absolute or relative path to the .nxb file"`
	Columns   *string `json:"columns,omitempty"   jsonschema:"Comma-separated list of column names to include in order"`
	Delimiter *string `json:"delimiter,omitempty" jsonschema:"Field separator character. Default: comma"`
}

type ImportInput struct {
	Source     string  `json:"source"                jsonschema:"Path to the source file or - for stdin"`
	Output     string  `json:"output"                jsonschema:"Path for the output .nxb file"`
	Format     string  `json:"format"                jsonschema:"Input format: json csv or xml"`
	OnConflict *string `json:"on_conflict,omitempty" jsonschema:"Schema conflict resolution: error (default) coerce-string or first-wins"`
}

type CompileInput struct {
	Source string  `json:"source"           jsonschema:"Path to the .nxs source file"`
	Output *string `json:"output,omitempty" jsonschema:"Output .nxb path. Defaults to source.nxb"`
}

// ── Tool registration ─────────────────────────────────────────────────────────

func registerTools(s *mcp.Server, resolver *Resolver) {
	mcp.AddTool(s, &mcp.Tool{
		Name:        "nxs_inspect",
		Description: "Inspect a .nxb binary file: returns schema keys, record count, preamble metadata, and decoded records as JSON.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, in InspectInput) (*mcp.CallToolResult, any, error) {
		records := "3"
		if in.Records != nil {
			records = *in.Records
		}
		verifyHash := in.VerifyHash != nil && *in.VerifyHash
		return textResult(handleInspect(ctx, resolver, in.Path, records, verifyHash)), nil, nil
	})

	mcp.AddTool(s, &mcp.Tool{
		Name:        "nxs_schema",
		Description: "Return only the schema of a .nxb file (key names and sigil types) without decoding any records. Fast even on large files.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, in SchemaInput) (*mcp.CallToolResult, any, error) {
		return textResult(handleSchema(ctx, resolver, in.Path)), nil, nil
	})

	mcp.AddTool(s, &mcp.Tool{
		Name:        "nxs_record",
		Description: "Decode and return a single record from a .nxb file by zero-based index.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, in RecordInput) (*mcp.CallToolResult, any, error) {
		return textResult(handleRecord(ctx, resolver, in.Path, in.Index)), nil, nil
	})

	mcp.AddTool(s, &mcp.Tool{
		Name:        "nxs_export_json",
		Description: "Export a .nxb file as JSON. Returns up to `limit` records (default 100). Pass limit=-1 to export all records (may be very large).",
	}, func(ctx context.Context, req *mcp.CallToolRequest, in ExportJSONInput) (*mcp.CallToolResult, any, error) {
		pretty := in.Pretty != nil && *in.Pretty
		ndjson := in.NDJSON != nil && *in.NDJSON
		limit := 100
		if in.Limit != nil {
			limit = *in.Limit
		}
		return textResult(handleExportJSON(ctx, resolver, in.Path, pretty, ndjson, limit)), nil, nil
	})

	mcp.AddTool(s, &mcp.Tool{
		Name:        "nxs_export_csv",
		Description: "Export a .nxb file as CSV.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, in ExportCSVInput) (*mcp.CallToolResult, any, error) {
		columns := ""
		if in.Columns != nil {
			columns = *in.Columns
		}
		delimiter := ""
		if in.Delimiter != nil {
			delimiter = *in.Delimiter
		}
		return textResult(handleExportCSV(ctx, resolver, in.Path, columns, delimiter)), nil, nil
	})

	mcp.AddTool(s, &mcp.Tool{
		Name:        "nxs_import",
		Description: `Convert a JSON, CSV, or XML file into a .nxb binary file.`,
	}, func(ctx context.Context, req *mcp.CallToolRequest, in ImportInput) (*mcp.CallToolResult, any, error) {
		onConflict := ""
		if in.OnConflict != nil {
			onConflict = *in.OnConflict
		}
		return textResult(handleImport(ctx, resolver, in.Source, in.Output, in.Format, onConflict)), nil, nil
	})

	mcp.AddTool(s, &mcp.Tool{
		Name:        "nxs_compile",
		Description: "Compile a .nxs text source file into a .nxb binary file.",
	}, func(ctx context.Context, req *mcp.CallToolRequest, in CompileInput) (*mcp.CallToolResult, any, error) {
		output := ""
		if in.Output != nil {
			output = *in.Output
		}
		return textResult(handleCompile(ctx, resolver, in.Source, output)), nil, nil
	})
}

// textResult wraps a string as a tool text content result.
func textResult(s string) *mcp.CallToolResult {
	return &mcp.CallToolResult{
		Content: []mcp.Content{&mcp.TextContent{Text: s}},
	}
}

// ── Resource registration ─────────────────────────────────────────────────────

func registerResources(s *mcp.Server, resolver *Resolver, dataDir string) {
	s.AddResourceTemplate(
		&mcp.ResourceTemplate{
			URITemplate: "nxb://{path}",
			Name:        "NXB file",
			Description: "A .nxb binary file. Read to get full inspection output (schema + records).",
			MIMEType:    "application/json",
		},
		func(ctx context.Context, req *mcp.ReadResourceRequest) (*mcp.ReadResourceResult, error) {
			uri := req.Params.URI
			const prefix = "nxb://"
			if len(uri) <= len(prefix) {
				return nil, fmt.Errorf("invalid nxb URI: %s", uri)
			}
			path := uri[len(prefix):]
			content, err := readResourceContent(ctx, path, resolver)
			if err != nil {
				return nil, err
			}
			return &mcp.ReadResourceResult{
				Contents: []*mcp.ResourceContents{{URI: uri, MIMEType: "application/json", Text: content}},
			}, nil
		},
	)

	if dataDir == "" {
		return
	}

	s.AddResource(
		&mcp.Resource{
			URI:         "nxb:///.nxb-index",
			Name:        "NXB file index",
			Description: fmt.Sprintf("List of all .nxb files found under %s", dataDir),
			MIMEType:    "application/json",
		},
		func(ctx context.Context, req *mcp.ReadResourceRequest) (*mcp.ReadResourceResult, error) {
			resources, err := ListResources(dataDir, resolver)
			if err != nil {
				return nil, err
			}
			type entry struct {
				URI         string `json:"uri"`
				Path        string `json:"path"`
				RecordCount uint32 `json:"record_count"`
				SizeBytes   int64  `json:"size_bytes"`
			}
			entries := make([]entry, len(resources))
			for i, r := range resources {
				entries[i] = entry{
					URI:         resourceURI(r.Path),
					Path:        r.Path,
					RecordCount: r.RecordCount,
					SizeBytes:   r.SizeBytes,
				}
			}
			b, err := json.Marshal(entries)
			if err != nil {
				return nil, err
			}
			return &mcp.ReadResourceResult{
				Contents: []*mcp.ResourceContents{{URI: req.Params.URI, MIMEType: "application/json", Text: string(b)}},
			}, nil
		},
	)
}
