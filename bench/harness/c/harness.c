#ifndef _POSIX_C_SOURCE
#define _POSIX_C_SOURCE 200809L
#endif

/*
 * Cross-format benchmark harness (C). Uniform CLI per BENCHMARK_SUITE.md §5.2.
 *
 * NXS is fully implemented; other formats return a clear error until linked.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <sys/stat.h>

#include "stats.h"
#include "../../../../nyxis-drivers/c/nxs.h"

typedef enum { WL_A, WL_B, WL_C } workload_t;
typedef enum { FMT_NXS } format_t;
typedef enum { MET_SIZE, MET_SELECTIVE, MET_OPEN, MET_ACCESS, MET_SCAN } metric_t;

typedef struct {
    workload_t workload;
    format_t format;
    metric_t metric;
    uint32_t records;
    double population;
    const char *data_dir;
    const char *path;
} config_t;

static uint8_t *map_file(const char *path, size_t *out_size) {
    FILE *f = fopen(path, "rb");
    if (!f) return NULL;
    fseek(f, 0, SEEK_END);
    *out_size = (size_t)ftell(f);
    rewind(f);
    if (*out_size == 0) {
        fclose(f);
        uint8_t *buf = malloc(1);
        if (!buf) {
            return NULL;
        }
        buf[0] = 0;
        return buf;
    }
    uint8_t *buf = malloc(*out_size);
    if (!buf) { fclose(f); return NULL; }
    if (fread(buf, 1, *out_size, f) != *out_size) {
        free(buf);
        fclose(f);
        return NULL;
    }
    fclose(f);
    return buf;
}

static int64_t file_size_bytes(const char *path) {
    struct stat st;
    if (stat(path, &st) != 0) return -1;
    return (int64_t)st.st_size;
}

static void emit_json(const config_t *cfg, bench_stats_t *st, int64_t extra) {
    const char *wl = cfg->workload == WL_A ? "A" : cfg->workload == WL_B ? "B" : "C";
    const char *fmt = "nxs";
    const char *met = cfg->metric == MET_SIZE ? "size"
                    : cfg->metric == MET_SELECTIVE ? "selective"
                    : cfg->metric == MET_OPEN ? "open"
                    : cfg->metric == MET_ACCESS ? "access" : "scan";
    if (cfg->metric == MET_SIZE) {
        printf("{\"workload\":\"%s\",\"format\":\"%s\",\"records\":%u,\"metric\":\"%s\","
               "\"bytes\":%lld,\"population\":%.2f}\n",
               wl, fmt, cfg->records, met, (long long)extra, cfg->population);
        return;
    }
    printf("{\"workload\":\"%s\",\"format\":\"%s\",\"records\":%u,\"metric\":\"%s\","
           "\"p50_ns\":%lld,\"p99_ns\":%lld,\"iqr_ns\":%lld,\"samples\":%d,\"population\":%.2f}\n",
           wl, fmt, cfg->records, met,
           (long long)st->p50_ns, (long long)st->p99_ns, (long long)st->iqr_ns,
           st->samples, cfg->population);
}

typedef struct {
    nxs_reader_t *r;
    uint32_t rec_idx;
    const char *field;
} nxs_ctx_t;

/* Cold open: parse header + read record 0 field each iteration (uniform with Rust harness). */
typedef struct {
    const uint8_t *data;
    size_t len;
    const char *field;
} nxs_open_ctx_t;

static void do_open(void *p) {
    nxs_open_ctx_t *c = (nxs_open_ctx_t *)p;
    nxs_reader_t r;
    if (nxs_open(&r, c->data, c->len) != NXS_OK) return;
    nxs_object_t obj;
    nxs_record(&r, 0, &obj);
    double v;
    nxs_get_f64(&obj, c->field, &v);
    nxs_close(&r);
}

static void do_access(void *p) {
    nxs_ctx_t *c = (nxs_ctx_t *)p;
    uint32_t n = c->r->record_count;
    uint32_t idx = (c->rec_idx * 997u + 1u) % (n ? n : 1u);
    c->rec_idx = idx;
    nxs_object_t obj;
    nxs_record(c->r, idx, &obj);
    double v;
    nxs_get_f64(&obj, c->field, &v);
}

static void do_scan(void *p) {
    nxs_ctx_t *c = (nxs_ctx_t *)p;
    volatile double sink = nxs_sum_f64(c->r, c->field);
    (void)sink;
}

/* Workload A selective read: five canonical fields (see workload_A.md). */
static void do_selective(void *p) {
    nxs_ctx_t *c = (nxs_ctx_t *)p;
    uint32_t n = c->r->record_count;
    uint32_t idx = (c->rec_idx * 997u + 1u) % (n ? n : 1u);
    c->rec_idx = idx;
    nxs_object_t obj;
    nxs_record(c->r, idx, &obj);
    int64_t i64;
    double f64;
    int b;
    char sbuf[256];
    nxs_get_i64(&obj, "i01", &i64);
    nxs_get_str(&obj, "s21", sbuf, sizeof(sbuf));
    nxs_get_f64(&obj, "f36", &f64);
    nxs_get_bool(&obj, "b46", &b);
    nxs_get_i64(&obj, "i10", &i64);
    (void)i64;
    (void)f64;
    (void)b;
    (void)sbuf;
}

static int run_nxs(config_t *cfg) {
    if (cfg->metric == MET_SELECTIVE && cfg->workload != WL_A) {
        fprintf(stderr, "selective metric only for workload A\n");
        return 2;
    }

    if (cfg->metric == MET_SIZE) {
        int64_t sz = file_size_bytes(cfg->path);
        if (sz < 0) {
            fprintf(stderr, "cannot stat %s\n", cfg->path);
            return 1;
        }
        bench_stats_t dummy = {0};
        emit_json(cfg, &dummy, sz);
        return 0;
    }

    size_t len = 0;
    uint8_t *data = map_file(cfg->path, &len);
    if (!data) {
        fprintf(stderr, "cannot read %s\n", cfg->path);
        return 1;
    }

    nxs_reader_t reader;
    if (nxs_open(&reader, data, len) != NXS_OK) {
        fprintf(stderr, "nxs_open failed\n");
        free(data);
        return 1;
    }

    const char *field = "score";
    if (cfg->workload == WL_C) field = "score";
    if (cfg->workload == WL_A) field = "f36";

    bench_stats_t st;
    if (cfg->metric == MET_OPEN) {
        nxs_open_ctx_t octx = { .data = data, .len = len, .field = field };
        st = bench_measure(do_open, &octx);
    } else {
        nxs_ctx_t ctx = { .r = &reader, .rec_idx = 0, .field = field };
        void (*fn)(void *) = cfg->metric == MET_ACCESS ? do_access
                         : cfg->metric == MET_SELECTIVE ? do_selective
                         : do_scan;
        st = bench_measure(fn, &ctx);
    }
    emit_json(cfg, &st, 0);

    nxs_close(&reader);
    free(data);
    return 0;
}

static int parse_args(int argc, char **argv, config_t *cfg) {
    memset(cfg, 0, sizeof(*cfg));
    cfg->population = -1.0;
    cfg->data_dir = "bench/data/bin";

    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--workload") == 0 && i + 1 < argc) {
            char w = argv[++i][0];
            cfg->workload = w == 'A' || w == 'a' ? WL_A : w == 'B' || w == 'b' ? WL_B : WL_C;
        } else if (strcmp(argv[i], "--format") == 0 && i + 1 < argc) {
            if (strcmp(argv[++i], "nxs") != 0) {
                fprintf(stderr, "C harness: only --format nxs is implemented in this build\n");
                return -1;
            }
            cfg->format = FMT_NXS;
        } else if (strcmp(argv[i], "--records") == 0 && i + 1 < argc) {
            cfg->records = (uint32_t)strtoul(argv[++i], NULL, 10);
        } else if (strcmp(argv[i], "--population") == 0 && i + 1 < argc) {
            cfg->population = strtod(argv[++i], NULL);
        } else if (strcmp(argv[i], "--metric") == 0 && i + 1 < argc) {
            const char *m = argv[++i];
            if (strcmp(m, "size") == 0) cfg->metric = MET_SIZE;
            else if (strcmp(m, "selective") == 0) cfg->metric = MET_SELECTIVE;
            else if (strcmp(m, "open") == 0) cfg->metric = MET_OPEN;
            else if (strcmp(m, "access") == 0) cfg->metric = MET_ACCESS;
            else if (strcmp(m, "scan") == 0) cfg->metric = MET_SCAN;
            else { fprintf(stderr, "unknown metric %s\n", m); return -1; }
        } else if (strcmp(argv[i], "--data-dir") == 0 && i + 1 < argc) {
            cfg->data_dir = argv[++i];
        } else if (strcmp(argv[i], "--path") == 0 && i + 1 < argc) {
            cfg->path = argv[++i];
        }
    }
    return 0;
}

static void build_default_path(config_t *cfg, char *buf, size_t bufsz) {
    if (cfg->path) return;
    const char *wl =
        cfg->workload == WL_A ? "A" : cfg->workload == WL_B ? "B" : "C";
    if (cfg->workload == WL_A && cfg->population >= 0.0) {
        int pct = (int)(cfg->population * 100.0 + 0.5);
        snprintf(buf, bufsz, "%s/workload_%s_nxs_%u_pop%02d.nxb",
                 cfg->data_dir, wl, cfg->records, pct);
    } else {
        snprintf(buf, bufsz, "%s/workload_%s_nxs_%u.nxb",
                 cfg->data_dir, wl, cfg->records);
    }
}

int main(int argc, char **argv) {
    config_t cfg;
    char path_buf[1024];
    if (parse_args(argc, argv, &cfg) != 0) return 2;
    if (cfg.records == 0) {
        fprintf(stderr, "--records required\n");
        return 2;
    }
    build_default_path(&cfg, path_buf, sizeof(path_buf));
    if (!cfg.path) cfg.path = path_buf;

    return run_nxs(&cfg);
}
