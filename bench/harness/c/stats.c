#ifndef _POSIX_C_SOURCE
#define _POSIX_C_SOURCE 200809L
#endif

#include "stats.h"
#include <stdlib.h>
#include <string.h>
#include <time.h>

int64_t bench_now_ns(void) {
    struct timespec ts;
#ifdef __APPLE__
    clock_gettime(CLOCK_MONOTONIC, &ts);
#else
    clock_gettime(CLOCK_MONOTONIC_RAW, &ts);
#endif
    return (int64_t)ts.tv_sec * 1000000000LL + (int64_t)ts.tv_nsec;
}

static int cmp_i64(const void *a, const void *b) {
    int64_t x = *(const int64_t *)a;
    int64_t y = *(const int64_t *)b;
    return (x > y) - (x < y);
}

static int64_t percentile(int64_t *sorted, int n, double p) {
    if (n <= 0) return 0;
    double idx = p * (double)(n - 1);
    int lo = (int)idx;
    int hi = lo + 1 < n ? lo + 1 : lo;
    double frac = idx - (double)lo;
    return (int64_t)((double)sorted[lo] * (1.0 - frac) + (double)sorted[hi] * frac);
}

bench_stats_t bench_measure(void (*fn)(void *ctx), void *ctx) {
    bench_stats_t out = {0, 0, 0, BENCH_SAMPLES};
    int64_t *samples = calloc(BENCH_SAMPLES, sizeof(int64_t));
    if (!samples) {
        return out;
    }

    for (int i = 0; i < BENCH_WARMUP; i++) {
        fn(ctx);
    }

    for (int i = 0; i < BENCH_SAMPLES; i++) {
        int64_t t0 = bench_now_ns();
        fn(ctx);
        int64_t t1 = bench_now_ns();
        samples[i] = t1 - t0;
    }

    qsort(samples, BENCH_SAMPLES, sizeof(int64_t), cmp_i64);

    int q1_idx = BENCH_SAMPLES / 4;
    int q3_idx = (3 * BENCH_SAMPLES) / 4;
    int64_t q1 = samples[q1_idx];
    int64_t q3 = samples[q3_idx];
    out.iqr_ns = q3 - q1;

    int trim_lo = q1_idx;
    int trim_hi = q3_idx;
    int trimmed = trim_hi - trim_lo + 1;
    if (trimmed < 1) {
        trim_lo = 0;
        trim_hi = BENCH_SAMPLES - 1;
        trimmed = BENCH_SAMPLES;
    }

    int64_t *trimmed_buf = samples + trim_lo;
    qsort(trimmed_buf, (size_t)trimmed, sizeof(int64_t), cmp_i64);

    out.p50_ns = percentile(trimmed_buf, trimmed, 0.50);
    out.p99_ns = percentile(trimmed_buf, trimmed, 0.99);

    free(samples);
    return out;
}
