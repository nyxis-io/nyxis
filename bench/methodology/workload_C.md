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
