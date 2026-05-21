# NXS cross-format harness (C)
DRV := ../../../../nyxis-drivers/c
CC ?= cc
BENCH_CFLAGS ?= -O3 -std=c99 -Wall -Wextra
CFLAGS := $(BENCH_CFLAGS) -I$(DRV) -I.
LDFLAGS ?=

.PHONY: all clean

all: harness

harness: harness.c stats.c $(DRV)/nxs.c $(DRV)/nxs.h stats.h
	$(CC) $(CFLAGS) -o harness harness.c stats.c $(DRV)/nxs.c $(LDFLAGS)

clean:
	rm -f harness
