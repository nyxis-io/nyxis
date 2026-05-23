// Package harness provides shared prefetch test utilities for cross-driver conformance.
package harness

import "sort"

// CoalescePageIndices merges sorted unique page indices when the gap between consecutive
// indices is at most gapPages (Adaptive-prefetch-spec §7.2).
func CoalescePageIndices(indices []int, gapPages int) [][2]int {
	if len(indices) == 0 {
		return nil
	}
	seen := make(map[int]struct{}, len(indices))
	uniq := make([]int, 0, len(indices))
	for _, p := range indices {
		if _, ok := seen[p]; ok {
			continue
		}
		seen[p] = struct{}{}
		uniq = append(uniq, p)
	}
	sort.Ints(uniq)

	var out [][2]int
	start := uniq[0]
	end := uniq[0]
	for i := 1; i < len(uniq); i++ {
		if uniq[i]-end <= gapPages {
			end = uniq[i]
			continue
		}
		out = append(out, [2]int{start, end})
		start = uniq[i]
		end = uniq[i]
	}
	out = append(out, [2]int{start, end})
	return out
}

// ByteRanges converts inclusive page index ranges to byte offsets and lengths.
func ByteRanges(ranges [][2]int, pageSize int, fileSize int64) [][2]int64 {
	out := make([][2]int64, 0, len(ranges))
	for _, r := range ranges {
		start := int64(r[0]) * int64(pageSize)
		end := int64(r[1]+1) * int64(pageSize)
		if end > fileSize {
			end = fileSize
		}
		if start >= fileSize {
			continue
		}
		out = append(out, [2]int64{start, end - start})
	}
	return out
}
