# =============================================================================
# TEST — aggregate orchestrator
# =============================================================================

include make/test-unit.mk
include make/test-infra.mk
include make/test-integration-rustd.mk
include make/acceptance.mk
include make/dry.mk
include make/bench.mk

# The per-worktree Zig cache path. `make/dev.mk` _clean removes it; nothing else
# reads it. Not exported: this names the directory _clean is responsible for,
# and only that.
ZIG_LOCAL_CACHE_DIR ?= $(CURDIR)/.tmp/zig-local-cache

.PHONY: test-unit-all

test-unit-all: test-unit-rustd test-coverage-all  ## Run all unit lanes (Rust workspace + multi-package coverage)
	@echo "✓ All unit lanes passed"
