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
MEMLEAK_TARGET ?= x86_64-linux

.PHONY: test

test: test-unit _test_e2e  ## Run full test suite (test-unit + backend/API e2e)
	@echo "✓ Full test suite passed"
