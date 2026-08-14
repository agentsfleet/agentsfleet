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
# tens of points. Raise it only in the same commit as the tests that clear it —
# 91 was set ahead of the tests and gated nothing but red, because the measured
# merged figure has never reached it.
ZIG_COVERAGE_MIN_LINES ?= 89
# Components whose reports MUST carry measured lines, one definition site per
# platform. kcov 43 reads the product line tables of only `runner` and `lib` of
# the eight component binaries on Linux: a kcov run with no include or exclude
# filter at all returns nothing but `/opt/zig/lib/compiler_rt/*` for the others,
# while their debug info carries correctly-rooted product units the filter would
# match. The filters are not the cause and the same sources measure every
# component on macOS, so the Linux lane grades what kcov hands it and names the
# rest as unmeasured on every run. `deadline` and `s3` have been seen collecting
# on one run and not the next, so they stay out of the required list — only
# these two have collected every run. The lists are the regression signal: a
# component that collects today and stops fails the gate instead of quietly
# shrinking the denominator.
ifeq ($(shell uname -s),Linux)
ZIG_COVERAGE_REQUIRED_COMPONENTS ?= runner lib
else
ZIG_COVERAGE_REQUIRED_COMPONENTS ?= agentsfleetd runner lib logging deadline s3 integration
endif
# Use baseline CPU so valgrind can execute SHA/AVX instructions it can't emulate.
MEMLEAK_CPU ?= baseline

.PHONY: test-unit-all

test-unit-all: test-unit-agentsfleetd test-unit-agentsfleet-runner test-unit-agentsfleet-lib test-coverage-all  ## Run all unit lanes (Zig + multi-package coverage)
	@echo "✓ All unit lanes passed"
