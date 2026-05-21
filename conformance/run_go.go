//go:build ignore

// NXS conformance runner for Go.
// Usage: cd go && go run ../conformance/run_go.go ../conformance/

package main

import (
	"encoding/binary"
	"encoding/json"
	"fmt"
	"math"
	"os"
	"path/filepath"
	"sort"
	"strings"

	nxs "github.com/nyxis-io/nyxis-drivers/go"
)

const MAGIC_LIST uint32 = 0x4E59584C // NYXL

func approxEq(a, b float64) bool {
	if a == b {
		return true
	}
	diff := math.Abs(a - b)
	mag := math.Max(math.Abs(a), math.Abs(b))
	if mag < 1e-300 {
		return diff < 1e-300
	}
	return diff/mag < 1e-9
}

type expected struct {
	RecordCount *int                     `json:"record_count,omitempty"`
	Keys        []string                 `json:"keys,omitempty"`
	Records     []map[string]interface{} `json:"records,omitempty"`
	Error       string                   `json:"error,omitempty"`
}

func readList(data []byte, off int) (interface{}, bool) {
	if off+16 > len(data) {
		return nil, false
	}
	magic := binary.LittleEndian.Uint32(data[off : off+4])
	if magic != MAGIC_LIST {
		return nil, false
	}
	elemSigil := data[off+8]
	elemCount := int(binary.LittleEndian.Uint32(data[off+9 : off+13]))
	dataStart := off + 16
	out := make([]interface{}, elemCount)
	for i := 0; i < elemCount; i++ {
		elemOff := dataStart + i*8
		if elemOff+8 > len(data) {
			break
		}
		switch elemSigil {
		case 0x3D: // = int
			v := int64(binary.LittleEndian.Uint64(data[elemOff : elemOff+8]))
			out[i] = float64(v) // JSON numbers
		case 0x7E: // ~ float
			bits := binary.LittleEndian.Uint64(data[elemOff : elemOff+8])
			out[i] = math.Float64frombits(bits)
		}
	}
	return out, true
}

// resolveSlotRaw walks the bitmask/offset-table for data[objOffset] and returns
// the absolute byte offset of slot's value, or -1 if absent.
func resolveSlotRaw(data []byte, objOffset, slot int) int {
	p := objOffset + 8
	cur := 0
	tableIdx := 0
	var b byte
	for {
		if p >= len(data) {
			return -1
		}
		b = data[p]
		p++
		bits := b & 0x7F
		for i := 0; i < 7; i++ {
			if cur == slot {
				if (bits>>i)&1 == 0 {
					return -1
				}
				_ = true // found
				goto doneMask
			}
			if (bits>>i)&1 == 1 {
				tableIdx++
			}
			cur++
		}
		if b&0x80 == 0 {
			return -1
		}
	}
doneMask:
	for b&0x80 != 0 {
		if p >= len(data) {
			break
		}
		b = data[p]
		p++
	}
	rel := int(binary.LittleEndian.Uint16(data[p+tableIdx*2:]))
	return objOffset + rel
}

func getFieldValue(data []byte, reader *nxs.Reader, tailStart, ri, slot int, sigilByte byte) (interface{}, bool) {
	// Get the object offset from tail index
	abs := int(binary.LittleEndian.Uint64(data[tailStart+ri*10+2 : tailStart+ri*10+10]))

	off := resolveSlotRaw(data, abs, slot)
	if off < 0 {
		return nil, false // absent
	}

	// Check for list magic
	if off+4 <= len(data) {
		maybe := binary.LittleEndian.Uint32(data[off : off+4])
		if maybe == MAGIC_LIST {
			lst, ok := readList(data, off)
			return lst, ok
		}
	}

	switch sigilByte {
	case 0x3D: // = int
		v := int64(binary.LittleEndian.Uint64(data[off : off+8]))
		return float64(v), true
	case 0x7E: // ~ float
		bits := binary.LittleEndian.Uint64(data[off : off+8])
		return math.Float64frombits(bits), true
	case 0x3F: // ? bool
		return data[off] != 0, true
	case 0x22: // " str
		if off+4 > len(data) {
			return nil, false
		}
		length := int(binary.LittleEndian.Uint32(data[off : off+4]))
		if off+4+length > len(data) {
			return nil, false
		}
		return string(data[off+4 : off+4+length]), true
	case 0x40: // @ time
		v := int64(binary.LittleEndian.Uint64(data[off : off+8]))
		return float64(v), true
	case 0x5E: // ^ null
		return nil, true // present but null
	default:
		// Try as i64
		if off+8 <= len(data) {
			v := int64(binary.LittleEndian.Uint64(data[off : off+8]))
			return float64(v), true
		}
		return nil, false
	}
}

func valuesMatch(actual, expected interface{}) bool {
	if expected == nil {
		return actual == nil
	}
	switch e := expected.(type) {
	case bool:
		a, ok := actual.(bool)
		return ok && a == e
	case float64:
		switch a := actual.(type) {
		case float64:
			return approxEq(a, e)
		case bool:
			return false
		}
		return false
	case string:
		a, ok := actual.(string)
		return ok && a == e
	case []interface{}:
		a, ok := actual.([]interface{})
		if !ok || len(a) != len(e) {
			return false
		}
		for i := range e {
			if !valuesMatch(a[i], e[i]) {
				return false
			}
		}
		return true
	}
	return false
}

func runPositive(conformanceDir, name string, exp expected) error {
	nxbPath := filepath.Join(conformanceDir, name+".nxb")
	data, err := os.ReadFile(nxbPath)
	if err != nil {
		return fmt.Errorf("read: %w", err)
	}

	reader, err := nxs.NewReader(data)
	if err != nil {
		return fmt.Errorf("open: %w", err)
	}

	// Validate record_count
	if exp.RecordCount != nil && reader.RecordCount() != *exp.RecordCount {
		return fmt.Errorf("record_count: expected %d, got %d", *exp.RecordCount, reader.RecordCount())
	}

	// Validate keys
	for i, expKey := range exp.Keys {
		if i >= len(reader.Keys) {
			return fmt.Errorf("key[%d] missing (expected %q)", i, expKey)
		}
		if reader.Keys[i] != expKey {
			return fmt.Errorf("key[%d]: expected %q, got %q", i, expKey, reader.Keys[i])
		}
	}

	// Validate each record
	for ri, expRec := range exp.Records {
		obj := reader.Record(ri)
		for key, expVal := range expRec {
			slot := -1
			for i, k := range reader.Keys {
				if k == key {
					slot = i
					break
				}
			}
			if slot < 0 {
				return fmt.Errorf("rec[%d].%s: key not in schema", ri, key)
			}

			sigil := byte(0x3D)
			if slot < len(reader.KeySigils) {
				sigil = reader.KeySigils[slot]
			}

			var actual interface{}
			var present bool
			switch sigil {
			case 0x3D, 0x40:
				var v int64
				v, present = obj.GetI64BySlot(slot)
				if present {
					actual = float64(v)
				}
			case 0x7E:
				var v float64
				v, present = obj.GetF64BySlot(slot)
				if present {
					actual = v
				}
			case 0x3F:
				var v bool
				v, present = obj.GetBoolBySlot(slot)
				if present {
					actual = v
				}
			case 0x22:
				var s string
				s, present = obj.GetStrBySlot(slot)
				if present {
					actual = s
				}
			default:
				actual, present = getFieldValue(data, reader, int(reader.TailPtr)+4, ri, slot, sigil)
			}

			if expVal == nil {
				_ = present
				continue
			}
			if !present {
				return fmt.Errorf("rec[%d].%s: field absent (expected %v)", ri, key, expVal)
			}
			if !valuesMatch(actual, expVal) {
				return fmt.Errorf("rec[%d].%s: expected %v (%T), got %v (%T)", ri, key, expVal, expVal, actual, actual)
			}
		}
	}
	return nil
}

func runNegative(conformanceDir, name, expectedCode string) error {
	nxbPath := filepath.Join(conformanceDir, name+".nxb")
	data, err := os.ReadFile(nxbPath)
	if err != nil {
		return fmt.Errorf("read: %w", err)
	}

	_, openErr := nxs.NewReader(data)
	if openErr == nil {
		return fmt.Errorf("expected error %q but reader succeeded", expectedCode)
	}
	errStr := openErr.Error()

	if !strings.Contains(errStr, expectedCode) {
		return fmt.Errorf("expected error %q, got: %s", expectedCode, errStr)
	}
	return nil
}

func main() {
	conformanceDir := "."
	if len(os.Args) > 1 {
		conformanceDir = os.Args[1]
	}

	entries, _ := filepath.Glob(filepath.Join(conformanceDir, "*.expected.json"))
	sort.Strings(entries)

	passed, failed := 0, 0

	for _, jsonPath := range entries {
		base := strings.TrimSuffix(filepath.Base(jsonPath), ".expected.json")
		data, _ := os.ReadFile(jsonPath)
		var exp expected
		json.Unmarshal(data, &exp)

		isNegative := exp.Error != ""
		var runErr error
		if isNegative {
			runErr = runNegative(conformanceDir, base, exp.Error)
		} else {
			runErr = runPositive(conformanceDir, base, exp)
		}

		if runErr == nil {
			fmt.Printf("  PASS  %s\n", base)
			passed++
		} else {
			fmt.Fprintf(os.Stderr, "  FAIL  %s — %s\n", base, runErr)
			failed++
		}
	}

	fmt.Printf("\n%d passed, %d failed\n", passed, failed)
	if failed > 0 {
		os.Exit(1)
	}
}
