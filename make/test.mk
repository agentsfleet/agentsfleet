# =============================================================================
# TEST — aggregate orchestrator
# =============================================================================

include make/test-unit.mk
include make/test-infra.mk
include make/test-integration-rustd.mk
include make/acceptance.mk
include make/dry.mk
include make/bench.mk

# The GLOBAL cache is shared by every worktree on this machine; the LOCAL cache
# stays per-worktree. That is the split Zig intends: the global cache holds
# content-addressed build artifacts for the dependency graph (pg.zig, http.zig,
# nullclaw, ...), which are identical across worktrees of the same repo, while
# the local cache holds this checkout's own compilation.
#
# Pointing the global cache at $(CURDIR) gave each worktree a private copy and
# defeated it — four checkouts here held four 65-123 MB caches, each recompiling
# the same dependencies from scratch. Concurrent access is the case the global
# cache is built for (its default, ~/.cache/zig, is shared by every Zig project
# on the machine): entries are content-addressed and lock-protected.
#
# Both are `?=`, and an environment variable beats `?=`, so CI keeps overriding
# these to workspace-local paths it can cache between runs — see
# .github/workflows/test-integration.yml.
ZIG_GLOBAL_CACHE_DIR ?= $(HOME)/.cache/agentsfleet/zig-global-cache
ZIG_LOCAL_CACHE_DIR  ?= $(CURDIR)/.tmp/zig-local-cache
# Components whose reports MUST carry measured lines, one definition site per
# platform. A component that collects today and stops fails the gate, instead of
# quietly shrinking the denominator.
#
# Linux carried a short list while Zig's self-hosted backend emitted debug info
# libdw refuses. Test binaries now compile through LLVM, which fixes it at the
# source (docs/architecture/testing.md §Coverage). The list ratchets on evidence:
# add a component in the commit where a green run shows it collecting.
#
# Ratcheted to the full Linux set on the run that earned it — job 94963891177
# reported `measured over 8 of 8 components — every component collected`, every
# one carrying lines (agentsfleetd 26392, integration 23104, runner 4588,
# runner_integration 4136, deadline 307, lib 594, logging 276, s3 28). Naming
# all eight is what keeps the LLVM fix honest: with only `runner lib` required,
# the six that were silently dark could go dark again and the gate would stay
# green. runner_integration is Linux-only, which is why this list is longer.
ifeq ($(shell uname -s),Linux)
else
endif





.PHONY: test-unit-all

test-unit-all: test-unit-rustd test-coverage-all  ## Run all unit lanes (Zig + multi-package coverage)
	@echo "✓ All unit lanes passed"
