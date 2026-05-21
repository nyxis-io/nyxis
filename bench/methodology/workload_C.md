# Workload C — Dense uniform analytical reducer (methodology template)

**Status:** Draft — freeze before publication.

## Hypothesis

**Nyxis loses** to Apache Arrow IPC on this workload. Publish the loss prominently.

## Schema

8 dense fields (all populated): `id`, `bucket`, `quantity`, `amount`, `rate`, `score`, `category`, `active`.

## Dataset sizes

1M and 10M records.

## Primary metric

Time for `sum(score)` and `count_distinct(category)` over entire dataset (P50, P99 ms).

## Competitors

FlatBuffers, Cap'n Proto, Protobuf, **Apache Arrow IPC** (honest columnar comparator).

## Expected ordering

Arrow fastest → Cap'n Proto ≈ FlatBuffers → Protobuf → Nyxis slowest among zero-copy row formats.

## Publication tables

**Primary table must include Arrow IPC** — Workload C without Arrow misses the point of the workload.

| Format | open | scan | size |
|--------|------|------|------|
| Arrow IPC | (publish from run) | (publish from run) | (publish from run) |
| NXS | … | … | … |
| Cap'n Proto | … | … | … |

Prose (publish adjacent to table):

> Arrow wins columnar scan by orders of magnitude (~2600× vs NXS on dev 10k). This is the expected architectural result — route dense analytics to Arrow. NXS and Cap'n Proto are row-oriented; scan reflects per-record traversal, not batch column operations.

**Protobuf (post-parse reference):** separate table + footnote from Workload B methodology.

## NXS scan (publication framing)

Use the same scan disclaimer as Workload B until C/Python implement a uniform-layout fast path (Go `SumF64Fast`). The measured gap vs Arrow is an **honest architectural loss** (columnar vs row). The gap vs Protobuf scan is additionally **not apples-to-apples** (Python list iteration on parsed objects vs wire walk).
