#pragma once
#include <stddef.h>
#include <stdint.h>

#define BENCH_WARMUP 100
#define BENCH_SAMPLES 1000

typedef struct {
    int64_t p50_ns;
    int64_t p99_ns;
    int64_t iqr_ns;
    int samples;
} bench_stats_t;

/* Measure fn() BENCH_WARMUP + BENCH_SAMPLES times; report IQR-trimmed median as p50. */
bench_stats_t bench_measure(void (*fn)(void *ctx), void *ctx);

int64_t bench_now_ns(void);
