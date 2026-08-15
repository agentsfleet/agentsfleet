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
# ---------------------------------------------------------------------------
# Coverage floors, targets and denominator minimums — ONE definition site each.
# The checker accepts all of them only as arguments, so no recipe and no Python
# module can hold a second copy.
#
# Floors are ENFORCED and raise-only: move one in the same commit as the tests
# that measurably clear it, never ahead. 91 was once set ahead of the tests and
# gated nothing but red. Targets are PUBLISHED, never enforced — the gap between
# floor and target is printed every run so the destination stays visible without
# an unmet target turning the build red.
#
# Production lines only: the coverage target excludes test bodies AND test
# support from the denominator, so these are shares of shipped code. They read
# the unit lanes and the live-service integration suite merged, because those
# cover largely disjoint code and either alone understates the truth by tens of
# points.
# ---------------------------------------------------------------------------
# Was ZIG_COVERAGE_MIN_LINES, which named a percentage. It now sits beside a
# real measured-line minimum, and two variables a letter apart meaning entirely
# different things is how the wrong one gets edited.
ZIG_COVERAGE_MIN_PCT ?= 89
ZIG_COVERAGE_TARGET_PCT ?= 95
# Per-folder enforced floors. Measured on the union at the time each was set;
# they ratchet toward the targets below as tests land.
ZIG_COVERAGE_FOLDER_FLOORS ?= agentsfleetd=89 runner=93 lib=93
# The quality bar for every product folder.
ZIG_COVERAGE_FOLDER_TARGETS ?= agentsfleetd=95 runner=95 lib=95
# One floor under the shape of the whole report, deliberately NOT one per
# component. The failure being caught is collapse — kcov once returned 24 files
# where the tree holds 558 — and a pair of numbers at roughly half the measured
# figures catches that by a mile. Per-component minimums were tried and cut:
# they were fourteen numbers to maintain, they duplicated the
# require-component assertion that already fails a component contributing
# nothing, and they turned every honest deletion of dead code into a red gate.
# Set these low on purpose. They are a collapse alarm, not a growth ratchet.
ZIG_COVERAGE_MIN_FILES ?= 300
ZIG_COVERAGE_MIN_MEASURED_LINES ?= 18000
# Product roots that must carry a measured line, whatever the rate. A union at
# 98% holding one tree is not a measurement of the codebase.
ZIG_COVERAGE_REQUIRED_ROOTS ?= agentsfleetd runner lib
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
ZIG_COVERAGE_REQUIRED_COMPONENTS ?= agentsfleetd runner lib logging deadline s3 runner_integration integration
else
ZIG_COVERAGE_REQUIRED_COMPONENTS ?= agentsfleetd runner lib logging deadline s3 integration
endif
# Use baseline CPU so valgrind can execute SHA/AVX instructions it can't emulate.
MEMLEAK_CPU ?= baseline

.PHONY: test-unit-all

test-unit-all: test-unit-agentsfleetd test-unit-agentsfleet-runner test-unit-agentsfleet-lib test-coverage-all  ## Run all unit lanes (Zig + multi-package coverage)
	@echo "✓ All unit lanes passed"
