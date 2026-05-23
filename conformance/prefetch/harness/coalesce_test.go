package harness

import "testing"

func TestRecorderCoalesce(t *testing.T) {
	ranges := CoalescePageIndices([]int{3, 4, 6, 7, 12}, 1)
	if len(ranges) != 3 {
		t.Fatalf("got %d ranges, want 3: %#v", len(ranges), ranges)
	}
	if ranges[0] != [2]int{3, 4} || ranges[1] != [2]int{6, 7} || ranges[2] != [2]int{12, 12} {
		t.Fatalf("unexpected ranges: %#v", ranges)
	}
	bytes := ByteRanges(ranges, 65536, 1<<20)
	if len(bytes) != 3 {
		t.Fatalf("byte ranges: %#v", bytes)
	}
	if bytes[0][1] != 2*65536 {
		t.Fatalf("first range length: %d", bytes[0][1])
	}
}

func TestCoalesceDedupeIndices(t *testing.T) {
	ranges := CoalescePageIndices([]int{3, 3, 4}, 1)
	if len(ranges) != 1 || ranges[0] != [2]int{3, 4} {
		t.Fatalf("dedupe: %#v", ranges)
	}
}
