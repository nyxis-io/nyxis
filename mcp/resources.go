package main

import (
	"context"
	"encoding/binary"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"
)

const nxsMagicFile uint32 = 0x4E595842 // NYXB

// FileResource holds lightweight metadata extracted from a .nxb header.
type FileResource struct {
	Path        string
	RecordCount uint32
	SizeBytes   int64
}

// readHeader reads just the preamble of a .nxb file to get record count.
// This avoids spawning a subprocess during resource listing.
func readHeader(path string) (*FileResource, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()

	info, err := f.Stat()
	if err != nil {
		return nil, err
	}
	size := info.Size()
	if size < 32 {
		return nil, fmt.Errorf("file too small to be a valid .nxb")
	}

	// Read preamble (32 bytes) — io.ReadFull ensures the buffer is fully populated.
	preamble := make([]byte, 32)
	if _, err := io.ReadFull(f, preamble); err != nil {
		return nil, err
	}
	if binary.LittleEndian.Uint32(preamble[0:4]) != nxsMagicFile {
		return nil, fmt.Errorf("not a .nxb file: bad magic")
	}

	tailPtr := binary.LittleEndian.Uint64(preamble[16:24])
	if int64(tailPtr)+4 > size {
		return nil, fmt.Errorf("tail pointer out of bounds")
	}

	// Read record count at tailPtr
	countBuf := make([]byte, 4)
	if _, err := f.ReadAt(countBuf, int64(tailPtr)); err != nil {
		return nil, err
	}
	recordCount := binary.LittleEndian.Uint32(countBuf)

	return &FileResource{
		Path:        path,
		RecordCount: recordCount,
		SizeBytes:   size,
	}, nil
}

// ListResources walks dataDir for *.nxb files and returns lightweight metadata.
// filepath.WalkDir is used instead of filepath.Walk to avoid an extra os.Lstat per entry.
func ListResources(dataDir string, _ *Resolver) ([]FileResource, error) {
	var resources []FileResource
	err := filepath.WalkDir(dataDir, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return nil // skip unreadable entries
		}
		if d.IsDir() || filepath.Ext(path) != ".nxb" {
			return nil
		}
		abs, err := filepath.Abs(path)
		if err != nil {
			return nil
		}
		fr, err := readHeader(abs)
		if err != nil {
			// Include the file with zero count rather than failing the whole listing
			info, _ := d.Info()
			var size int64
			if info != nil {
				size = info.Size()
			}
			resources = append(resources, FileResource{Path: abs, SizeBytes: size})
			return nil
		}
		resources = append(resources, *fr)
		return nil
	})
	return resources, err
}

// resourceURI returns the MCP resource URI for a .nxb file path.
func resourceURI(absPath string) string {
	return "nxb://" + absPath
}

// readResourceContent shells out to nxs-inspect --json for full file details.
func readResourceContent(ctx context.Context, path string, resolver *Resolver) (string, error) {
	return runBinary(ctx, resolver, "nxs-inspect", []string{"--json", path})
}
