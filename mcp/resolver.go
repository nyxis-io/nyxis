package main

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
)

// Resolver finds Rust binary paths. Resolution order:
//  1. binDir/<name>  (from --bin-dir flag)
//  2. <exe-dir>/../rust/target/release/<name>  (dev layout)
//  3. exec.LookPath(name)  (system PATH)
type Resolver struct {
	binDir string
	// exeDir is the directory of the running process, resolved once.
	exeDir string
}

// NewResolver creates a Resolver. binDir may be empty.
func NewResolver(binDir string) *Resolver {
	exe, _ := os.Executable()
	return &Resolver{
		binDir: binDir,
		exeDir: filepath.Dir(exe),
	}
}

// Find returns the absolute path of a named binary.
func (r *Resolver) Find(name string) (string, error) {
	// 1. Explicit bin-dir
	if r.binDir != "" {
		p := filepath.Join(r.binDir, name)
		if isExecutable(p) {
			return p, nil
		}
	}

	// 2. Dev layout: adjacent to the MCP binary inside the repo
	devPath := filepath.Join(r.exeDir, "..", "rust", "target", "release", name)
	devPath = filepath.Clean(devPath)
	if isExecutable(devPath) {
		return devPath, nil
	}

	// 3. System PATH
	p, err := exec.LookPath(name)
	if err == nil {
		return p, nil
	}

	return "", fmt.Errorf("binary %q not found: set --bin-dir or add it to PATH", name)
}

func isExecutable(path string) bool {
	info, err := os.Stat(path)
	if err != nil {
		return false
	}
	return !info.IsDir() && info.Mode()&0o111 != 0
}
