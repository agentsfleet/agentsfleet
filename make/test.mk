# =============================================================================
# TEST — aggregate orchestrator
# =============================================================================

include make/test-unit.mk
include make/test-integration.mk
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
ZIG_COVERAGE_DIR ?= $(CURDIR)/coverage/zig
# Production lines only — the coverage target excludes test bodies from the
# denominator, so this is the share of shipped code the suites actually execute.
# It reads the unit lanes and the live-service integration suite merged, because
# they cover largely disjoint code and either one alone understates the truth by
# tens of points. Raise it only in the same commit as the tests that clear it.
ZIG_COVERAGE_MIN_LINES ?= 83
BENCH_MODE ?= bench
# Use native target for memleak — avoids cross-compile dynamic linker mismatch
# when OpenSSL is linked. Valgrind needs the system's ld-linux, not Zig's bundled one.
# Use baseline CPU so valgrind can execute SHA/AVX instructions it can't emulate.
MEMLEAK_TARGET ?=
MEMLEAK_CPU    ?= baseline

.PHONY: test-unit-all

test-unit-all: test-unit-agentsfleetd test-unit-agentsfleet-runner test-unit-agentsfleet-lib test-coverage-all  ## Run all unit lanes (Zig + multi-package coverage)
	@echo "✓ All unit lanes passed"
