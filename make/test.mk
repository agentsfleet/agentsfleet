# =============================================================================
# TEST — aggregate orchestrator
# =============================================================================

include make/test-unit.mk
include make/test-integration.mk
include make/test-e2e.mk
include make/test-bench.mk

ZIG_GLOBAL_CACHE_DIR ?= $(CURDIR)/.tmp/zig-global-cache
ZIG_LOCAL_CACHE_DIR  ?= $(CURDIR)/.tmp/zig-local-cache
ZOMBIED_COVERAGE_MIN_LINES ?= 35
BENCH_MODE ?= bench
# Use native target for memleak — avoids cross-compile dynamic linker mismatch
# when OpenSSL is linked. Valgrind needs the system's ld-linux, not Zig's bundled one.
# Use baseline CPU so valgrind can execute SHA/AVX instructions it can't emulate.
MEMLEAK_TARGET ?=
MEMLEAK_CPU    ?= baseline

.PHONY: test

test: test-zombied test-unit-zombiectl test-unit-website test-unit-app  ## Run all unit tests (zombied + zombiectl + website + app)
	@echo "✓ All unit tests passed"
