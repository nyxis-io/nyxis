# NXS cross-format harness (C)
DRV := ../../../../nyxis-drivers/c
CC ?= cc
BENCH_CFLAGS ?= -O3 -std=c99 -Wall -Wextra
CFLAGS := $(BENCH_CFLAGS) -I$(DRV) -I.
LDFLAGS ?= -lm -pthread

.PHONY: all clean

NXS_SRCS := $(DRV)/nxs.c $(DRV)/nxs_prefetch.c $(DRV)/nxs_column_prefetch.c

all: harness

harness: harness.c stats.c $(NXS_SRCS) $(DRV)/nxs.h stats.h
	$(CC) $(CFLAGS) -o harness harness.c stats.c $(NXS_SRCS) $(LDFLAGS)

clean:
	rm -f harness
