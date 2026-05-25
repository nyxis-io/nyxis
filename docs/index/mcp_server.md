---
room: mcp_server
subdomain: (top-level)
source_paths: [mcp/]
see_also: ["rust/bins.md", "rust/convert.md", "../_root.md"]
hot_paths: [main.go, tools.go, resolver.go]
architectural_health: normal
security_tier: sensitive
---

# mcp/ — NXS MCP Server

Subdomain: mcp/
Source paths: mcp/

## TASK → LOAD

| Task | Load |
|------|------|
| Start the stdio MCP server | server.md |
| Add a tool that shells out to an nxs binary | server.md |
| List .nxb resources from a data directory | server.md |

---

# errors.go

DOES: Defines MCP-facing ToolError with exit code, message, and stderr; serializes to JSON for tool responses.
SYMBOLS:
- ToolError { Code int, Message string, Stderr string }
- (e ToolError) JSON() → string
- exitCodeMessage(code int) → string
PATTERNS: error-mapping

---

# main.go

DOES: Boots nxs-mcp stdio MCP server; registers tools and resources; resolves CLI binaries via Resolver; handles SIGINT/SIGTERM shutdown.
SYMBOLS:
- main()
- Types: InspectInput, SchemaInput, RecordInput, ExportJSONInput, ExportCSVInput, ImportInput, CompileInput (+more tool input structs)
CONFIG: data-dir, bin-dir flags
DEPENDS: mcp/tools.go, mcp/resources.go, mcp/resolver.go
PATTERNS: stdio-transport, signal-shutdown

---

# resolver.go

DOES: Locates nxs-inspect, nxs-export, nxs-import, and nxs binaries from bin-dir or PATH with caching.
SYMBOLS:
- Resolver struct
- NewResolver(binDir string) → *Resolver
- (r Resolver) Find(name string) → (string, error)
USE WHEN: Tool handlers need a resolved absolute path to a Rust CLI binary

---

# resources.go

DOES: Registers MCP resources for .nxb files under data-dir; reads file bytes for resource URIs.
SYMBOLS:
- registerResources(s, resolver, dataDir string)
- (+resource list/read handlers)
DEPENDS: mcp/resolver.go

---

# tools.go

DOES: Implements MCP tools by execing nxs binaries (inspect, schema, record, export JSON/CSV, import, compile); maps non-zero exits to ToolError JSON.
SYMBOLS:
- runBinary(ctx, resolver, binary string, args []string) → (string, error)
- toolResult(out string, err error) → string
- handleInspect(ctx, resolver, path, records string, verifyHash bool) → string
- registerTools(s, resolver)
- (+handlers for export/import/compile)
DEPENDS: mcp/resolver.go, mcp/errors.go
PATTERNS: process-bridge

---

# tools_test.go

DOES: Unit tests for tool argument validation and ToolError JSON formatting.
SYMBOLS:
- (+test functions for inspect/export paths and error codes)
