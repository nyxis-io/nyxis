package main

import (
	"encoding/json"
	"fmt"
)

// ToolError is returned by all tool handlers on non-zero exit.
type ToolError struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
	Stderr  string `json:"stderr,omitempty"`
}

func (e *ToolError) Error() string {
	return fmt.Sprintf("exit %d: %s", e.Code, e.Message)
}

func (e *ToolError) JSON() string {
	b, _ := json.Marshal(e)
	return string(b)
}

// exitCodeMessage maps Rust binary exit codes to human-readable messages.
func exitCodeMessage(code int) string {
	switch code {
	case 0:
		return "success"
	case 1:
		return "usage error: invalid arguments or flags"
	case 2:
		return "I/O error: could not read or write the file"
	case 3:
		return "format error: bad magic bytes, schema hash mismatch, out-of-bounds read, or malformed input"
	case 4:
		return "schema conflict: two records disagree on a key's type (use --on-conflict to override)"
	case 5:
		return "bad magic: file does not start with the NYXB header"
	default:
		return fmt.Sprintf("unexpected exit code %d", code)
	}
}
