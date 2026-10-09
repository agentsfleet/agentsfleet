# =============================================================================
# TEST — aggregate orchestrator
# =============================================================================

include make/test-unit.mk
include make/test-infra.mk
include make/test-integration-rustd.mk
include make/acceptance.mk
include make/dry.mk
include make/bench.mk

.PHONY: test-unit-all

test-unit-all: test-unit-rustd test-coverage-all  ## Run all unit lanes (Rust workspace + multi-package coverage)
	@echo "✓ All unit lanes passed"
